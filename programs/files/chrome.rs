//! Shared painting/hit geometry for the Files command and navigation bars.
#[derive(Clone,Copy)]
pub struct Rect {pub x:u32,pub y:u32,pub w:u32,pub h:u32}
impl Rect {pub fn contains(self,x:i32,y:i32)->bool {x>=self.x as i32&&y>=self.y as i32&&x<(self.x+self.w) as i32&&y<(self.y+self.h) as i32}}
pub const PRIMARY:[(usize,&[u8]);7]=[(0,b"Up"),(1,b"Open"),(2,b"New file"),(3,b"New folder"),(5,b"Refresh"),(8,b"View"),(20,b"More")];
pub const NAV:[(usize,&[u8]);3]=[(15,b"Back"),(16,b"Forward"),(18,b"Places")];
pub fn primary(width:u32,i:usize)->Rect {let cell=(width-16)/7;Rect{x:8+i as u32*cell,y:4,w:if i==6 {width-8-(8+6*cell)}else{cell-4},h:26}}
pub fn nav(i:usize)->Rect {Rect{x:8+i as u32*68,y:36,w:64,h:24}}
pub fn page(width:u32,height:u32,next:bool)->Rect {Rect{x:width-if next {76}else{152},y:height-32,w:68,h:16}}
pub fn hit(width:u32,height:u32,x:i32,y:i32)->Option<usize> {
    for (i,(action,_)) in PRIMARY.iter().enumerate() {if primary(width,i).contains(x,y) {return Some(*action);}}
    for (i,(action,_)) in NAV.iter().enumerate() {if nav(i).contains(x,y) {return Some(*action);}}
    for next in [false,true] {if page(width,height,next).contains(x,y) {return Some(if next {7}else{6});}}
    None
}
