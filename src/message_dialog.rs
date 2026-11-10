//! Bounded app-modal notifications; owner windows retain their original surfaces.
use crate::{desktop_ui::Rect,window_protocol as api};
pub const MAX:usize=192;
pub struct Notice {pub active:bool,pub kind:u32,pub bytes:[u8;MAX],pub len:usize,pub offset:usize,pressed:bool,buttons:u32}
impl Notice {
 pub const fn new()->Self {Self{active:false,kind:0,bytes:[0;MAX],len:0,offset:0,pressed:false,buttons:0}}
 pub fn announces(&self,kind:u32,bytes:&[u8])->bool{!self.active||(kind>=self.kind&&(kind!=self.kind||self.bytes[..self.len]!=*bytes))}
 pub fn set(&mut self,kind:u32,bytes:&[u8])->bool {if kind>2||bytes.is_empty()||bytes.len()>MAX||!bytes.iter().all(|b|(32..=126).contains(b)){return false;}if self.active&&self.kind==kind&&self.bytes[..self.len]==*bytes{return true;}if self.active&&kind<self.kind{return true;}self.bytes[..bytes.len()].copy_from_slice(bytes);self.len=bytes.len();self.offset=0;self.kind=kind;self.active=true;self.pressed=false;true}
 pub fn rect(width:i32,height:i32)->Rect{let w=(width-16).min(480).max(120);let h=(height-8).min(224).max(64);Rect{x:(width-w)/2,y:(height-h)/2,w,h}}
 pub fn body(width:i32,height:i32)->Rect{let r=Self::rect(width,height);let top=if r.h<120{24}else{46};let footer=if r.h<120{24}else{36};Rect{x:r.x+12,y:r.y+top,w:r.w-24,h:(r.h-top-footer-4).max(16)}}
 pub fn button(width:i32,height:i32)->Rect{let r=Self::rect(width,height);let h=if r.h<120{20}else{26};let w=(r.w-24).min(108);Rect{x:r.x+r.w-w-12,y:r.y+r.h-h-8,w,h}}
 fn line(bytes:&[u8],width:i32)->usize{let fit=crate::canvas::text::fit(bytes,width as u32,crate::canvas::text::Font::Ui).max(1).min(bytes.len());if fit==bytes.len(){return fit;}bytes[..fit].iter().rposition(|b|*b==b' ').filter(|n|*n>fit/2).map_or(fit,|n|n+1)}
 fn end(&self,width:i32,height:i32)->usize{let r=Self::body(width,height);let mut at=self.offset;for _ in 0..r.h/16{if at==self.len{break;}at+=Self::line(&self.bytes[at..self.len],r.w);}at}
 pub fn event(&mut self,e:api::Event,width:i32,height:i32)->Option<api::Event> {
  if !self.active{return Some(e);}let mut accept=false;
  match e.kind {api::KEY=>accept=matches!(e.code as u16,10|27|32),api::CLOSE=>accept=true,
   api::POINTER=>{let inside=Self::button(width,height).contains(e.x,e.y);if e.buttons&1!=0&&self.buttons&1==0{self.pressed=inside;}accept=e.buttons&1==0&&self.buttons&1!=0&&self.pressed&&inside;self.buttons=e.buttons;},
   api::RESET=>{self.buttons=e.buttons;self.pressed=false;return Some(e);},api::RESIZE|api::DRAG_END=>return Some(e),_=>()}
  if accept {if self.end(width,height)<self.len&&!matches!(e.code as u16,27)&&e.kind!=api::CLOSE{self.offset=self.end(width,height);self.pressed=false;}else{self.active=false;return Some(api::Event{kind:api::RESET,buttons:e.buttons,..api::Event::default()});}}None
 }
 pub fn paint(&self,c:&mut crate::canvas::Canvas<'_>,client:Rect){
  if !self.active{return;}let mut r=Self::rect(client.w,client.h);r.x+=client.x;r.y+=client.y;
  let color=match self.kind{2=>0x923d46,1=>0x886013,_=>0x285c7a};let title=if r.h<120{20}else{30};
  c.rect(r.x+3,r.y+3,r.w,r.h,0x122333);c.rect(r.x,r.y,r.w,r.h,0xf3f6f8);c.rect(r.x,r.y,r.w,title,color);
  c.label(r.x+12,r.y+(title-16)/2,(r.w-24) as u32,0xffffff,match self.kind{2=>b"Error",1=>b"Warning",_=>b"Information"});
  let body=Self::body(client.w,client.h);let end=self.end(client.w,client.h);let mut at=self.offset;
  for row in 0..body.h/16{if at==end{break;}let n=Self::line(&self.bytes[at..end],body.w);c.label(client.x+body.x,client.y+body.y+row*16,body.w as u32,0x20384d,&self.bytes[at..at+n]);at+=n;}
  let mut b=Self::button(client.w,client.h);b.x+=client.x;b.y+=client.y;c.rect(b.x,b.y,b.w,b.h,color);c.label(b.x+12,b.y+(b.h-16)/2,(b.w-24) as u32,0xffffff,if end<self.len{b"Next"}else{b"OK"});
 }
}
