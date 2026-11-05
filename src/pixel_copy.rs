//! Copy before rendering; syscall entry keeps IRQs off while validating user data.
pub fn request(pointer:u32,size:u32,width:u32,height:u32)->Result<(crate::pixel_protocol::Blit,crate::loader::Frames,u32),u32> {
    if size!=20 {return Err((-22i32) as u32);}
    if !crate::user_memory::readable_range(pointer,20,0) {return Err((-14i32) as u32);}
    let r=unsafe {core::ptr::read_unaligned(pointer as *const crate::pixel_protocol::Blit)};
    let count=r.count(width,height).ok_or((-22i32) as u32)?;
    if !crate::user_memory::readable_range(r.pixels,count*4,0) {return Err((-14i32) as u32);}
    let storage=crate::loader::Frames::new((count as usize*4).div_ceil(4096)).ok_or((-12i32) as u32)?;
    unsafe {core::ptr::copy_nonoverlapping(r.pixels as *const u8,storage.base() as *mut u8,count as usize*4);}
    let pixels=unsafe {core::slice::from_raw_parts(storage.base() as *const u32,count as usize)};
    if pixels.iter().any(|p|*p>0xffffff) {return Err((-22i32) as u32);}
    Ok((r,storage,count))
}
pub fn apply(surface:u32,stride:u32,r:crate::pixel_protocol::Blit,storage:&crate::loader::Frames) {
    for row in 0..r.height {unsafe {
        core::ptr::copy_nonoverlapping((storage.base() as *const u32).add((row*r.width) as usize),
            (surface as *mut u32).add(((r.y+row)*stride+r.x) as usize),r.width as usize);
    }}
}
