//! Bounded physical desktop sizes; geometry is shared by rendering and input.
use core::sync::atomic::{AtomicU32,Ordering};
static SCALE:AtomicU32=AtomicU32::new(100);
static SIZE:AtomicU32=AtomicU32::new((800<<16)|600);
pub const MODES:[(u16,u16);4]=[(800,600),(1024,768),(1280,720),(1280,1024)];
pub fn supported(w:u16,h:u16)->bool {MODES.contains(&(w,h))}
pub fn set(w:u16,h:u16) {set_scaled(w,h,100);}
pub fn set_scaled(w:u16,h:u16,percent:u16) {
    assert!(supported(w,h));assert!(logical_size(w,h,percent).is_some());
    SIZE.store((w as u32)<<16|h as u32,Ordering::Relaxed);SCALE.store(percent as u32,Ordering::Relaxed);
}
pub fn scale()->u16 {SCALE.load(Ordering::Relaxed) as u16}
pub fn valid_scale(percent:u16)->bool {matches!(percent,100|125|150)}
pub fn logical_size(w:u16,h:u16,percent:u16)->Option<(u16,u16)> {
    if !supported(w,h)||!valid_scale(percent) {return None;}
    let (lw,lh)=(w as u32*100/percent as u32,h as u32*100/percent as u32);
    if lw<800||lh<600 {None}else {Some((lw as u16,lh as u16))}
}
pub fn physical_width()->i32 {(SIZE.load(Ordering::Relaxed)>>16) as i32}
pub fn physical_height()->i32 {(SIZE.load(Ordering::Relaxed)&65535) as i32}
// Ceil boundaries and floor samples are inverse mappings: adjacent logical
// spans tile every physical pixel exactly, including the final partial cell.
pub fn edge(position:usize,physical:usize,logical:usize)->usize {(position*physical).div_ceil(logical)}
pub fn sample(position:usize,physical:usize,logical:usize)->usize {position*logical/physical}
#[derive(Default)]
pub struct PointerMotion {x:i32,y:i32}
impl PointerMotion {
    pub const fn new()->Self {Self{x:0,y:0}}
    pub fn reset(&mut self) {*self=Self::new();}
    pub fn delta(&mut self,dx:i16,dy:i16)->(i32,i32) {
        self.x+=dx as i32*width();self.y+=dy as i32*height();
        let (x,y)=(self.x/physical_width(),self.y/physical_height());
        self.x%=physical_width();self.y%=physical_height();(x,y)
    }
}
pub fn width()->i32 {physical_width()*100/scale() as i32}
pub fn height()->i32 {physical_height()*100/scale() as i32}
pub fn taskbar()->i32 {((height()-56)/16)*16}
pub fn encode(w:u16,h:u16)->[u8;8] {let a=w.to_le_bytes();let b=h.to_le_bytes();[b'X',b'D',b'P',b'1',a[0],a[1],b[0],b[1]]}
pub fn decode(bytes:&[u8])->Option<(u16,u16)> {
    if bytes.len()!=8||&bytes[..4]!=b"XDP1" {return None;}
    let w=u16::from_le_bytes([bytes[4],bytes[5]]);let h=u16::from_le_bytes([bytes[6],bytes[7]]);
    supported(w,h).then_some((w,h))
}

pub fn encode_scale(percent:u16)->[u8;5] {[b'X',b'U',b'S',b'1',percent as u8]}
pub fn decode_scale(data:&[u8])->Option<u16> {
    if data.len()==5&&&data[..4]==b"XUS1"&&valid_scale(data[4] as u16) {Some(data[4] as u16)}else {None}
}
