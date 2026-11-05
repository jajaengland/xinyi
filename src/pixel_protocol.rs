//! Packed RGB24 tiles. No user pointer is dereferenced by these range checks.
pub const MAX_PIXELS:u32=2048;
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct Blit {pub x:u32,pub y:u32,pub width:u32,pub height:u32,pub pixels:u32}
impl Blit {
    pub fn count(&self,width:u32,height:u32)->Option<u32> {
        let count=self.width.checked_mul(self.height)?;
        (self.width>0 && self.height>0 && count<=MAX_PIXELS
            && self.x.checked_add(self.width)?<=width && self.y.checked_add(self.height)?<=height).then_some(count)
    }
}
const _:()=assert!(core::mem::size_of::<Blit>()==20);
