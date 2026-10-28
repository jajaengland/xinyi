use crate::{theme_model::Theme, user_memory as user_buffer};
static mut CURRENT: Theme = Theme::DEFAULT;
pub fn get() -> Theme {
    unsafe { *core::ptr::addr_of!(CURRENT) }
}
pub fn load() {
    let mut bytes = [0; 1024];
    if let Ok(n) = crate::diskfs::read("/GUI.CFG", &mut bytes) {
        if let Some(t) = Theme::parse(&bytes[..n]) {
            unsafe {
                CURRENT = t;
            }
        }
    }
}
pub fn syscall(cs: u32, op: u32, pointer: u32, size: u32) -> u32 {
    let Some(image) = crate::usermode::active_image(cs) else {
        return u32::MAX;
    };
    if op > 2 || size != 8 {
        return (-22i32) as u32;
    }
    if op == 0 {
        if !user_buffer::writable(pointer, 8) {
            return (-14i32) as u32;
        }
        unsafe {
            core::ptr::write_unaligned(pointer as *mut Theme, get());
        }
        return 0;
    }
    if !user_buffer::readable(pointer, 8, image) {
        return (-14i32) as u32;
    }
    let t = unsafe { core::ptr::read_unaligned(pointer as *const Theme) };
    if !t.valid() {
        return (-22i32) as u32;
    }
    if op == 2 {
        if !crate::diskfs::is_mounted() {
            return (-19i32) as u32;
        }
        unsafe {
            core::arch::asm!("sti", options(nostack));
        }
        let result = match crate::diskfs::create("/GUI.CFG") {
            Ok(()) | Err(crate::fat16::Error::Exists) => {
                crate::diskfs::write("/GUI.CFG", &t.encode())
            }
            Err(e) => Err(e),
        };
        unsafe {
            core::arch::asm!("cli", options(nostack));
        }
        if let Err(e) = result {
            return crate::files_api::error(e);
        }
    }
    unsafe {
        CURRENT = t;
        core::arch::asm!("sti", options(nostack));
    }
    if crate::multitask::active() {
        crate::multi_window::redraw();
    } else {
        crate::window_api::redraw();
    }
    unsafe {
        core::arch::asm!("cli", options(nostack));
    }
    0
}
