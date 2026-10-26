//! Minimal freestanding Rust interface for Xinyi's current flat-image ABI.
#![allow(dead_code)]
use core::arch::asm;

pub fn syscall(number: u32, first: u32, second: u32, third: u32) -> i32 {
    let result: u32;
    unsafe {
        // Preserve LLVM's EBX/base register; INT 80 preserves other GPRs.
        asm!("push ebx", "mov ebx, {arg:e}", "int 0x80", "pop ebx",
            arg = in(reg) first, inout("eax") number => result,
            in("ecx") second, in("edx") third);
    }
    result as i32
}
pub fn write(bytes: &[u8]) -> i32 {
    syscall(4, 1, bytes.as_ptr() as u32, bytes.len() as u32)
}
pub fn print(text: &str) {
    let _ = write(text.as_bytes());
}
pub fn getpid() -> u32 {
    syscall(20, 0, 0, 0) as u32
}
pub fn exit(code: u32) -> ! {
    let _ = syscall(1, code, 0, 0);
    unsafe {
        asm!("ud2", options(noreturn));
    }
}
#[no_mangle]
#[link_section = ".text.start"]
pub extern "C" fn _start() -> ! {
    exit(crate::main());
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    exit(101);
}
