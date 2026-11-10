//! Snapshot BMP wallpaper into an owned, pre-scaled cache outside drawing/IRQ code.
use crate::{wallpaper_model::{Request,Placement},loader::Frames,fat16::{Error,Result},canvas::Canvas};
struct Cache{frames:Frames,placement:Placement,width:u32,height:u32}
struct State{request:Request,cache:Option<Cache>}
static mut CURRENT:Option<State>=None;
fn prepare(r:&Request)->Result<Option<Cache>>{
 if r.length==0{return Ok(None);}let path=core::str::from_utf8(&r.path[..r.length as usize]).map_err(|_|Error::InvalidName)?;
 let size=crate::diskfs::file_size(path)?;if size<54||size>crate::bmp::MAX_FILE{return Err(Error::TooLarge);}
 let file=Frames::new(size.div_ceil(4096)).ok_or(Error::OutOfMemory)?;
 let data=unsafe{core::slice::from_raw_parts_mut(file.base() as *mut u8,size)};
 if crate::diskfs::read_image(path,data)?!=size{return Err(Error::Corrupt);}
 let image=crate::bmp::parse(data).ok_or(Error::InvalidApplication)?;
 let(width,height)=(crate::display_layout::width() as u32,crate::display_layout::taskbar() as u32);
 let placement=Placement::new(image.width,image.height,width,height,r.mode).ok_or(Error::TooLarge)?;
 let pixels=placement.width as usize*placement.height as usize;
 let frames=Frames::new((pixels*4).div_ceil(4096)).ok_or(Error::OutOfMemory)?;
 let output=unsafe{core::slice::from_raw_parts_mut(frames.base() as *mut u32,pixels)};
 for y in 0..placement.height{for x in 0..placement.width{let(sx,sy)=placement.source(x,y);output[(y*placement.width+x) as usize]=image.pixel(data,sx,sy);}if y%16==15{crate::disk_io::cpu_checkpoint();}}
 if r.volume!=crate::diskfs::volume_epoch(){return Err(Error::Corrupt);}Ok(Some(Cache{frames,placement,width,height}))
}
pub fn release(){unsafe{CURRENT=None;}}
pub fn load(){
 release();let mut bytes=[0;128];let size=match crate::diskfs::file_size("/WALL.CFG"){Ok(s)if s<=128=>s,_=>return};
 if crate::diskfs::read_at("/WALL.CFG",0,&mut bytes[..size])!=Ok(size){return;}
 let Some(mut request)=Request::parse(&bytes[..size])else{crate::debug::write_str("Wallpaper: invalid preference; default background\n");return;};request.volume=crate::diskfs::volume_epoch();
 let cache=match prepare(&request){Ok(c)=>c,Err(e)=>{crate::debug::write_fmt(format_args!("Wallpaper: fallback: {}\n",e.message()));None}};
 unsafe{CURRENT=Some(State{request,cache});}
}
pub fn paint(c:&mut Canvas<'_>)->bool {unsafe{
 let Some(s)=CURRENT.as_ref()else{return false;};let Some(cache)=s.cache.as_ref().filter(|cache|s.request.volume==crate::diskfs::volume_epoch()&&cache.width==crate::display_layout::width() as u32&&cache.height==crate::display_layout::taskbar() as u32)else{return false;};
 let p=cache.placement;let pixels=core::slice::from_raw_parts(cache.frames.base() as *const u32,p.width as usize*p.height as usize);c.blit(p.x as i32,p.y as i32,p.width as usize,p.height as usize,pixels)
}}
pub fn syscall(cs:u32,op:u32,pointer:u32,size:u32)->u32{
 let Some(image)=crate::usermode::active_image(cs)else{return u32::MAX;};if op>4||size!=112{return (-22i32) as u32;}
 if op==0{if !crate::user_memory::writable(pointer,112){return (-14i32) as u32;}let mut request=Request::new();request.volume=crate::diskfs::volume_epoch();let mut active=0;
  unsafe{if let Some(s)=CURRENT.as_ref().filter(|s|s.request.volume==request.volume){request=s.request;active=i32::from(s.cache.is_some());}core::ptr::write_unaligned(pointer as *mut Request,request);}return active as u32;
 }
 if !crate::usermode::profile().allows(106,op){return (-13i32) as u32;}
 if !crate::user_memory::readable(pointer,112,image){return (-14i32) as u32;}let request=unsafe{core::ptr::read_unaligned(pointer as *const Request)};
 if !request.valid()||((op==3||op==4)!=(request.length==0)){return (-22i32) as u32;}if request.volume!=crate::diskfs::volume_epoch(){return (-116i32) as u32;}
 unsafe{core::arch::asm!("sti",options(nostack));}
 let result=(||{let cache=prepare(&request)?;if request.volume!=crate::diskfs::volume_epoch(){return Err(Error::Corrupt);}if op==2||op==4{let mut bytes=[0;128];let n=request.encode(&mut bytes);crate::diskfs::save_large("/WALL.CFG",&bytes[..n])?;}if request.volume!=crate::diskfs::volume_epoch(){return Err(Error::Corrupt);}unsafe{CURRENT=Some(State{request,cache});}Ok(())})();
 if result.is_ok(){crate::debug::write_fmt(format_args!("Wallpaper: applied mode={} path={} saved={}\n",request.mode,core::str::from_utf8(&request.path[..request.length as usize]).unwrap(),op==2||op==4));if crate::multitask::active(){crate::multi_window::redraw();}else{crate::window_api::redraw();}}
 unsafe{core::arch::asm!("cli",options(nostack));}match result{Ok(())=>0,Err(_)if request.volume!=crate::diskfs::volume_epoch()=>(-116i32) as u32,Err(e)=>crate::files_api::error(e) as u32}
}
