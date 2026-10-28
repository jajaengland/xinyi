//! Current address-space range checks, including writable ELF data and allocated heap.
pub const MAX_WRITE: u32 = crate::user_buffer::MAX_WRITE;
pub fn writable(pointer: u32, length: u32) -> bool {
    crate::usermode::current_space().is_some_and(|s| s.permits(pointer, length, true))
}
pub fn readable(pointer: u32, length: u32, _image: u32) -> bool {
    length <= MAX_WRITE && readable_range(pointer, length, 0)
}
pub fn readable_range(pointer: u32, length: u32, _image: u32) -> bool {
    crate::usermode::current_space().is_some_and(|s| s.permits(pointer, length, false))
}
pub fn heap(cs: u32, end: u32, reserved: u32, last: u32) -> u32 {
    if crate::usermode::active_image(cs).is_none() {
        return u32::MAX;
    }
    if reserved | last != 0 {
        return (-22i32) as u32;
    }
    let Some(space) = crate::usermode::current_space() else {
        return u32::MAX;
    };
    match space.heap_break(end) {
        Ok(n) => n,
        Err(e) => e as u32,
    }
}
