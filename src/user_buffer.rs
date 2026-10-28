// Pure range checks for the current fixed user layout. No pointer is read here.
pub const MAX_WRITE: u32 = 1024;
pub fn writable(pointer: u32, length: u32) -> bool {
    if length == 0 {
        return true;
    }
    pointer >= 0x403fc000
        && pointer
            .checked_add(length)
            .is_some_and(|end| end <= 0x40400000)
}
pub fn readable(pointer: u32, length: u32, image_size: u32) -> bool {
    length <= MAX_WRITE && readable_range(pointer, length, image_size)
}
pub fn readable_range(pointer: u32, length: u32, image_size: u32) -> bool {
    if length == 0 {
        return true;
    }
    if image_size > 65536 {
        return false;
    }
    let Some(end) = pointer.checked_add(length) else {
        return false;
    };
    let Some(image_end) = 0x40000000u32.checked_add(image_size) else {
        return false;
    };
    (pointer >= 0x40000000 && end <= image_end) || (pointer >= 0x403fc000 && end <= 0x40400000)
}
