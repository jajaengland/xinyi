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

pub fn arguments(buffer: &mut [u8; 128]) -> i32 {
    syscall(59, buffer.as_mut_ptr() as u32, 128, 0)
}
pub fn read_file(path: &[u8], buffer: &mut [u8; 1024]) -> i32 {
    syscall(
        57,
        path.as_ptr() as u32,
        path.len() as u32,
        buffer.as_mut_ptr() as u32,
    )
}
pub fn save_file(path: &[u8], data: &[u8]) -> i32 {
    if path.len() > 105 || data.len() > 1024 {
        return -22;
    }
    syscall(
        58,
        path.as_ptr() as u32,
        path.len() as u32 | ((data.len() as u32) << 16),
        data.as_ptr() as u32,
    )
}
pub mod key {
    pub const UP: u16 = 0x100;
    pub const DOWN: u16 = 0x101;
    pub const LEFT: u16 = 0x102;
    pub const RIGHT: u16 = 0x103;
    pub const HOME: u16 = 0x104;
    pub const END: u16 = 0x105;
    pub const DELETE: u16 = 0x106;
    pub const PAGE_UP: u16 = 0x107;
    pub const PAGE_DOWN: u16 = 0x108;
    pub const INSERT: u16 = 0x109;
    pub const F1: u16 = 0x120;
    pub const F5: u16 = 0x124;
    pub const SHIFT: u32 = 1 << 16;
    pub const CTRL: u32 = 1 << 17;
    pub const ALT: u32 = 1 << 18;
    pub const CAPS: u32 = 1 << 19;
}
/// Positive result: key code in low 16 bits, modifiers in bits 16..19.
/// Zero means no key when polling; a negative result is an error.
pub fn read_key(wait: bool) -> i32 {
    syscall(50, wait as u32, 0, 0)
}
/// Monotonic 100 Hz counter, wrapping modulo 2^32. Compare with wrapping_sub.
pub fn ticks() -> u32 {
    syscall(51, 0, 0, 0) as u32
}
/// Sleep using 10 ms ticks plus an alignment tick; request at most 60 seconds.
pub fn sleep_ms(milliseconds: u32) -> i32 {
    syscall(52, milliseconds, 0, 0)
}
/// PC-speaker tone, 37..20000 Hz, at most 1000 ms. Zero frequency/duration stops.
/// Waits with the CPU halted and returns after the tone has stopped.
pub fn beep(frequency: u32, milliseconds: u32) -> i32 {
    syscall(56, frequency, milliseconds, 0)
}
pub fn clear_screen(color: u8) -> i32 {
    syscall(53, 0, color as u32, 0)
}
pub fn move_cursor(x: u16, y: u16) -> i32 {
    syscall(53, 1, x as u32 | ((y as u32) << 16), 0)
}
pub fn set_color(color: u8) -> i32 {
    syscall(53, 2, color as u32, 0)
}
/// Dimensions/cursor are packed as x | (y << 16).
pub fn screen_size() -> u32 {
    syscall(53, 3, 0, 0) as u32
}
pub fn cursor_position() -> u32 {
    syscall(53, 4, 0, 0) as u32
}
pub fn show_cursor(visible: bool) -> i32 {
    syscall(53, 5, visible as u32, 0)
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Cell {
    pub character: u8,
    pub color: u8,
}
/// Replace a row of individually coloured cells without moving the cursor.
pub fn draw_cells(x: u8, y: u8, cells: &[Cell]) -> i32 {
    syscall(
        55,
        x as u32 | ((y as u32) << 8),
        cells.as_ptr() as u32,
        cells.len() as u32,
    )
}
/// Replace a single-row span without scrolling or moving the cursor.
pub fn draw_text(x: u8, y: u8, color: u8, text: &[u8]) -> i32 {
    syscall(
        54,
        x as u32 | ((y as u32) << 8) | ((color as u32) << 16),
        text.as_ptr() as u32,
        text.len() as u32,
    )
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

/// One foreground window per program; handles expire at close/exit/fault.
#[path = "../src/window_protocol.rs"]
mod window_protocol;
pub mod window {
    use super::window_protocol as protocol;
    #[allow(unused_imports)]
    pub use protocol::{Draw, Event, CLOSE, HEIGHT, KEY, POINTER, RESET, WIDTH, MAX_COMMANDS};
    pub fn open(title: &[u8]) -> i32 {
        super::syscall(60, title.as_ptr() as u32, title.len() as u32, 0)
    }
    pub fn open_sized(title:&[u8],width:u16,height:u16)->i32 {super::syscall(65,title.as_ptr() as u32,title.len() as u32,width as u32|((height as u32)<<16))}
    pub fn event(handle: u32, event: &mut Event, wait: bool) -> i32 {
        super::syscall(61, handle, event as *mut _ as u32, wait as u32)
    }
    /// Text pointers in Draw commands must remain valid for this call.
    pub fn draw(handle: u32, commands: &[Draw]) -> i32 {
        super::syscall(62, handle, commands.as_ptr() as u32, commands.len() as u32)
    }
    pub fn row(handle: u32, x: u32, y: u32, bg: u32, fg: u32, text: &[u8]) -> i32 {
        if text.len()>80 {return -22;}
        draw(
            handle,
            &[
                Draw::rect(x, y, text.len() as u32 * 8, 16, bg),
                Draw::text(x, y, fg, text),
            ],
        )
    }
    pub fn present(handle: u32) -> i32 {
        super::syscall(63, handle, 0, 0)
    }
    pub fn close(handle: u32) -> i32 {
        super::syscall(64, handle, 0, 0)
    }
}

/// Yield the current scheduled quantum; a no-op for a foreground program.
pub fn yield_now() -> i32 { syscall(66, 0, 0, 0) }

/// Shared ASCII clipboard; at most 1024 bytes. An empty slice clears it.
pub fn clipboard_set(text:&[u8])->i32 {syscall(67,text.as_ptr() as u32,text.len() as u32,0)}
/// Copy the whole clipboard; a zero-capacity slice queries its byte length.
pub fn clipboard_get(output:&mut [u8])->i32 {syscall(68,output.as_mut_ptr() as u32,output.len() as u32,0)}
pub fn working_directory(output:&mut [u8;105])->i32 {syscall(70,output.as_mut_ptr() as u32,105,0)}
/// Queue a desktop app launch, using this process's working directory.
pub fn launch(command:&[u8])->i32 {syscall(71,command.as_ptr() as u32,command.len() as u32,0)}

pub mod pty {
 pub fn open()->i32 {super::syscall(72,0,0,0)}
 pub fn control(handle:u32,operation:u32)->i32 {super::syscall(72,operation,handle,0)}
 pub fn present(handle:u32,window:u32,scroll:u32)->i32 {super::syscall(73,handle,window,scroll)}
 pub fn key(handle:u32,event:u32)->i32 {super::syscall(74,handle,event,0)}
 pub fn paste(handle:u32,text:&[u8])->i32 {super::syscall(75,handle,text.as_ptr() as u32,text.len() as u32)}
 pub fn select(handle:u32,start:u32,end:u32)->i32 {super::syscall(76,handle,start,end)}
 pub fn copy(handle:u32)->i32 {super::syscall(77,handle,0,0)}
 pub fn clear_selection(handle:u32)->i32 {super::syscall(77,handle,1,0)}
}

#[path="../src/directory_protocol.rs"]
mod directory_protocol;
pub mod directory {
 #[allow(unused_imports)]
 pub use super::directory_protocol::{Entry,Listing};
 pub fn list(path:&[u8],offset:u16,out:&mut Listing)->i32 {super::syscall(78,path.as_ptr() as u32,path.len() as u32|((offset as u32)<<16),out as *mut _ as u32)}
 pub fn cd(path:&[u8])->i32 {super::syscall(79,path.as_ptr() as u32,path.len() as u32,0)}
 /// 0 creates a directory, 1 deletes a file, 2 removes an empty directory.
 pub fn action(path:&[u8],operation:u32)->i32 {super::syscall(80,path.as_ptr() as u32,path.len() as u32,operation)}
}
