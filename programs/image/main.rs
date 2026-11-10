#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
use alloc::vec::Vec;
#[path="../../sdk/xinyi.rs"]mod xinyi;
#[path="../../src/bmp.rs"]mod bmp;
use xinyi::{window::{self,Draw},file_dialog,key,files,gui,widgets::{Button,Rect}};
#[global_allocator]static HEAP:xinyi::heap::Allocator=xinyi::heap::Allocator::new();
#[alloc_error_handler]fn oom(_:core::alloc::Layout)->! {xinyi::exit(12)}
static SAMPLE:&[u8]=include_bytes!("../../assets/images/sample.bmp");
struct Loading {volume:u32,read:usize,size:usize}
struct Viewer {
    controls:[Button;4],data:Vec<u8>,tile:Vec<u32>,image:Option<bmp::Image>,loading:Option<Loading>,dialog:Option<file_dialog::Dialog>,
    name:[u8;105],len:usize,width:u32,height:u32,fit:bool,pan_x:u32,pan_y:u32,message:&'static[u8],
}
impl Viewer {
    fn sample(&mut self) {
        self.loading=None;self.image=None;self.data=Vec::new();
        if self.data.try_reserve_exact(SAMPLE.len()).is_err() {self.message=xinyi::gui::report(2,b"Not enough memory for the sample.");return;}
        self.data.extend_from_slice(SAMPLE);self.image=bmp::parse(&self.data);
        self.name[..10].copy_from_slice(b"sample.bmp");self.len=10;self.pan_x=0;self.pan_y=0;
        self.message=b"BMP images | Ctrl+O open | F fit | 1 actual size | Arrows pan | Ctrl+Q close";
    }
    fn load(&mut self,path:[u8;105],len:usize,volume:u32) {
        let size=files::stream_size_on(&path[..len],volume);
        if size<0 {self.message=xinyi::gui::report(2,b"Could not open image. Check its path and drive.");return;}
        if size as usize>bmp::MAX_FILE || size<54 {self.message=xinyi::gui::report(2,b"BMP must be between 54 bytes and 768 KiB.");return;}
        self.loading=None;self.image=None;self.data=Vec::new();
        if self.data.try_reserve_exact(size as usize).is_err() {self.message=xinyi::gui::report(2,b"Not enough memory for this image.");return;}
        self.data.resize(size as usize,0);self.name=path;self.len=len;self.pan_x=0;self.pan_y=0;
        self.loading=Some(Loading{volume,read:0,size:size as usize});self.message=b"Loading BMP... Escape cancels; other windows remain usable.";
    }
    fn step(&mut self) {
        let Some(load)=self.loading.as_mut() else{return;};
        let end=(load.read+16384).min(load.size);
        let result=files::read_at_on(&self.name[..self.len],load.read as u32,&mut self.data[load.read..end],load.volume);
        if result<0 || result as usize!=end-load.read {self.loading=None;self.message=xinyi::gui::report(2,b"Image read failed or the drive changed. Reopen the image.");return;}
        load.read=end;
        if end==load.size {
            self.loading=None;self.image=bmp::parse(&self.data);
            self.message=if self.image.is_some() {b"F fit | 1 actual size | Arrows pan | Ctrl+O open | Ctrl+Q close"}else{xinyi::gui::report(2,b"Unsupported or damaged BMP. Use uncompressed 24-bit or 32-bit RGB.")};
        }
    }
    fn paint(&mut self,w:u32) {
        window::draw(w,&[Draw::rect(0,0,self.width,self.height,0x202b36),
            Draw::rect(0,0,self.width,60,0xf3f6f8),Draw::text(8,40,0,&self.name[self.len.saturating_sub(76)..self.len])]);
        for (i,label) in [b"Open".as_slice(),b"Fit",b"1:1",b"Sample"].iter().enumerate() {
            gui::button_flat(w,&self.controls[i],label,(i==1&&self.fit)||(i==2&&!self.fit));
        }
        window::draw(w,&[Draw::rect(0,59,self.width,1,0xc6d3dc)]);
        if let Some(image)=self.image {
            let aw=self.width-16;let ah=self.height-96;
            let (rw,rh)=if self.fit {image.fit(aw,ah)}else{(image.width.min(aw),image.height.min(ah))};
            self.pan_x=self.pan_x.min(image.width-rw);self.pan_y=self.pan_y.min(image.height-rh);
            let ox=8+(aw-rw)/2;let oy=64+(ah-rh)/2;
            let step=(2048/rw).max(1);let mut y=0;
            while y<rh {
                let rows=step.min(rh-y);self.tile.resize((rw*rows) as usize,0);
                for ty in 0..rows {for x in 0..rw {
                    let sx=if self.fit {x*image.width/rw}else{x+self.pan_x};
                    let sy=if self.fit {(y+ty)*image.height/rh}else{y+ty+self.pan_y};
                    self.tile[(ty*rw+x) as usize]=image.pixel(&self.data,sx,sy);
                }}
                if window::pixels(w,ox,oy+y,rw,rows,&self.tile)<0 {self.message=xinyi::gui::report(2,b"Pixel drawing failed.");break;}
                y+=rows;
            }
            let mut dimensions=*b"0000 x 0000";number(&mut dimensions[..4],image.width);number(&mut dimensions[7..],image.height);
            window::draw(w,&[Draw::text(8,self.height-32,0xffffff,&dimensions),Draw::text(120,self.height-32,0xffffff,if self.fit {b"Fit (no upscaling)"}else{b"Actual size - arrows pan"})]);
        }
        window::draw(w,&[Draw::text(8,self.height-16,0xffffff,&self.message[..self.message.len().min(((self.width-16)/8) as usize)])]);
        if let Some(dialog)=self.dialog.as_ref() {dialog.paint(w);}
        window::present(w);
    }
}
fn number(out:&mut[u8],mut value:u32) {for b in out.iter_mut().rev(){*b=b'0'+(value%10) as u8;value/=10;}}
fn main()->u32 {
    let w=window::open(b"Image viewer");if w<0 {return 1;}let w=w as u32;
    let mut viewer=Viewer{controls:[Button::new(Rect::new(8,8,64,26)),Button::new(Rect::new(80,8,64,26)),Button::new(Rect::new(152,8,64,26)),Button::new(Rect::new(224,8,64,26))],data:Vec::new(),tile:Vec::new(),image:None,loading:None,dialog:None,name:[0;105],len:0,width:640,height:400,fit:true,pan_x:0,pan_y:0,message:b""};
    if viewer.tile.try_reserve_exact(2048).is_err() {return 12;}
    viewer.sample();window::resizable(w,640,400,1280,1024);xinyi::drag_drop::register_open();
    let mut args=[0;128];let n=xinyi::arguments(&mut args);
    if n>0 && n<=105 {let mut path=[0;105];path[..n as usize].copy_from_slice(&args[..n as usize]);viewer.load(path,n as usize,xinyi::directory::volume());}
    loop {
        viewer.step();viewer.paint(w);
        let mut e=window::Event::default();let result=window::event(w,&mut e,viewer.loading.is_none());
        if result<0 || e.kind==window::CLOSE {return 0;}
        if result==0 {xinyi::sleep_ms(10);continue;}
        if e.kind==window::RESIZE {if window::resize(w,e)==0 {viewer.width=e.x as u32;viewer.height=e.y as u32;}continue;}
        if e.kind==window::DROP {
            if viewer.dialog.is_some() {xinyi::drag_drop::finish();viewer.message=xinyi::gui::report(1,b"Close the dialog before dropping an image.");}
            else {match xinyi::drag_drop::open_file(e) {Ok((path,len,volume))=>viewer.load(path,len,volume),Err(_)=>viewer.message=xinyi::gui::report(1,b"Drop one image file from the current drive.")}}
            continue;
        }
        if let Some(dialog)=viewer.dialog.as_mut() {
            match dialog.event(e) {
                file_dialog::Result::Accept{path,len,volume}=>{viewer.dialog=None;viewer.load(path,len,volume);},
                file_dialog::Result::Cancel=>viewer.dialog=None,_=>(),
            }continue;
        }
        if e.kind==window::SCROLL&&!viewer.fit {viewer.pan_y=(viewer.pan_y as i64+(e.code as i32 as i64)*48).clamp(0,u32::MAX as i64) as u32;}
        if e.kind==window::KEY {
            let code=e.code as u16;let ctrl=e.code&key::CTRL!=0;
            if ctrl && matches!(code,81|113) {return 0;}
            if ctrl && matches!(code,79|111) {viewer.loading=None;viewer.dialog=Some(file_dialog::Dialog::new(false));continue;}
            match code {
                27 if viewer.loading.is_some()=>{viewer.loading=None;viewer.message=b"Image load cancelled.";},27=>return 0,
                70|102=>{viewer.fit=true;viewer.pan_x=0;viewer.pan_y=0;},49=>viewer.fit=false,
                key::LEFT=>viewer.pan_x=viewer.pan_x.saturating_sub(32),key::RIGHT=>viewer.pan_x=viewer.pan_x.saturating_add(32),
                key::UP=>viewer.pan_y=viewer.pan_y.saturating_sub(32),key::DOWN=>viewer.pan_y=viewer.pan_y.saturating_add(32),_=>(),
            }
        }
        if e.kind==window::RESET {for b in &mut viewer.controls {b.reset(e.buttons);}}
        if e.kind==window::POINTER {
            let mut action=None;
            for (i,b) in viewer.controls.iter_mut().enumerate() {if b.pointer(e.x,e.y,e.buttons) {action=Some(i);}}
            match action {
                Some(0)=>{viewer.loading=None;viewer.dialog=Some(file_dialog::Dialog::new(false));},
                Some(1)=>{viewer.fit=true;viewer.pan_x=0;viewer.pan_y=0;},Some(2)=>viewer.fit=false,Some(3)=>viewer.sample(),_=>(),
            }
        }
    }
}
