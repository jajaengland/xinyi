// Simple ELF-like loader for flat binaries
// For now, support loading raw binary files at a fixed address

use crate::{phys, vga, println};

pub const USER_LOAD_ADDR: u32 = 0x100000;
pub const USER_STACK_TOP: u32 = 0x200000;

#[derive(Clone, Copy)]
pub struct ProgramHeader {
    pub entry: u32,
    pub base: u32,
    pub size: u32,
}

pub fn load_binary(data: &[u8], size: usize) -> Option<ProgramHeader> {
    if size == 0 || size > 64 * 1024 {
        return None;
    }

    let load_addr = USER_LOAD_ADDR as *mut u8;
    unsafe {
        for i in 0..size {
            load_addr.add(i).write_volatile(data[i]);
        }
    }

    Some(ProgramHeader {
        entry: USER_LOAD_ADDR,
        base: USER_LOAD_ADDR,
        size: size as u32,
    })
}

pub fn create_user_stack() -> Option<u32> {
    let frame = phys::alloc_frame()? as u32;
    Some(frame + 4096) // stack top
}
