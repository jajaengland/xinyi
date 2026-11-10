use crate::xinyi::files;
#[path="../../src/app_access.rs"]pub mod model;
pub fn load(volume:u32)->Result<model::Table,i32>{
 let size=files::stream_size_on(b"/ACCESS.CFG",volume);if size==-2{return Ok(model::Table::new());}if size<0{return Err(size);}if size as usize>model::MAX_BYTES{return Err(-8);}
 let mut b=[0;model::MAX_BYTES];let n=files::read_at_on(b"/ACCESS.CFG",0,&mut b[..size as usize],volume);if n!=size{return Err(if n<0{n}else{-8});}model::Table::parse(&b[..size as usize]).map_err(|_|-8)
}
pub fn save(id:&[u8],value:Option<model::Restriction>,volume:u32)->i32 {let mut t=match load(volume){Ok(t)=>t,Err(e)=>return e};if t.set(id,value).is_err(){return -22;}let mut b=[0;model::MAX_BYTES];let n=t.encode(&mut b);files::save_on(b"/ACCESS.CFG",&b[..n],volume)}
pub fn label(value:Option<model::Restriction>)->&'static[u8]{match value{None=>b"Declared access",Some(model::Restriction::ReadOnly)=>b"Read-only",Some(model::Restriction::Media)=>b"Media",Some(model::Restriction::Isolated)=>b"Isolated"}}
