#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
unsafe extern "C" {
    static _image_end: u8;
}

fn main() -> u32 {
    if xinyi::getpid() == 0 {
        return 1;
    }
    if xinyi::syscall(4, 0, 0x100000, 16) != -9 {
        return 2;
    }
    for pointer in [
        0, 0x100000, 0xb8000, 0x40010000, 0x403fbfff, 0x40400000, 0xfffffff0,
    ] {
        if xinyi::syscall(4, 1, pointer, 32) != -14 {
            return 3;
        }
    }
    let end = core::ptr::addr_of!(_image_end) as u32;
    if xinyi::syscall(4, 1, end - 1, 2) != -14 {
        return 4;
    }
    if xinyi::syscall(4, 1, 0x403fffff, 2) != -14 {
        return 5;
    }
    if xinyi::syscall(4, 1, 0x403fc000, 1025) != -22 {
        return 6;
    }
    if xinyi::syscall(4, 1, 0xffffffff, 0) != 0 {
        return 7;
    }
    let stack_text = *b"Stack output works!\n";
    if xinyi::write(&stack_text) != stack_text.len() as i32 {
        return 8;
    }
    let stderr = b"Stderr output works!\n";
    if xinyi::syscall(4, 2, stderr.as_ptr() as u32, stderr.len() as u32) != stderr.len() as i32 {
        return 9;
    }
    xinyi::print("Console checks passed!\n");
    0
}
