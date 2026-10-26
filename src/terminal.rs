// Foreground program services. IRQs only update time/the scancode queue;
// no second task or IRQ can access the terminal writer during these calls.
use crate::{interrupts, keyboard, user_buffer, usermode, vga};
const INVALID: u32 = (-22i32) as u32;
static mut USED: bool = false;
static mut SAVED_COLOR: u8 = 0x0a;
static mut SAVED_CURSOR: bool = true;

pub fn begin() {
    unsafe { USED = false; SAVED_COLOR = vga::color(); SAVED_CURSOR = vga::cursor_visible(); }
}
pub fn finish() {
    crate::speaker::stop();
    if unsafe { USED } {
        vga::set_color(unsafe { SAVED_COLOR });
        vga::set_cursor_visible(unsafe { SAVED_CURSOR });
        vga::move_cursor(0, 24);
        crate::println!();
    }
}
fn touched() { unsafe { USED = true; } }

// Syscall entry has IF=0. Enable only around HLT so IRQ0/IRQ1 can wake us;
// return to Rust with IF=0. The outer IRET restores the caller's flags.
fn wait_interrupt() {
    unsafe { core::arch::asm!("sti; hlt; cli", options(nostack)); }
}

pub fn syscall(cs: u32, number: u32, first: u32, second: u32, third: u32) -> u32 {
    let Some(image_size) = usermode::active_image(cs) else { return u32::MAX; };
    match number {
        50 => {
            if first > 1 { return INVALID; }
            loop {
                if let Some(event) = keyboard::poll_event() { return event; }
                // Prefixes, releases and modifiers also consume queue bytes.
                if keyboard::queued_scancodes() != 0 { continue; }
                if first == 0 { return 0; }
                wait_interrupt();
            }
        }
        51 => interrupts::timer_ticks(), // 100 Hz, wraps after about 497 days.
        52 => {
            if first > 60_000 { return INVALID; }
            // One alignment tick prevents an almost-due PIT interrupt from
            // making the requested delay shorter than its nominal duration.
            let ticks = if first == 0 { 0 } else { (first + 9) / 10 + 1 };
            let started = interrupts::timer_ticks();
            while interrupts::timer_ticks().wrapping_sub(started) < ticks {
                wait_interrupt();
            }
            0
        }
        53 => match first {
            0 | 2 if second <= 0x7f => {
                touched();
                vga::set_color(second as u8);
                if first == 0 { vga::clear_screen(); }
                0
            }
            1 if second & 0xffff < 80 && second >> 16 < 25 => {
                touched();
                vga::move_cursor((second & 0xffff) as usize, (second >> 16) as usize);
                0
            }
            3 => 80 | (25 << 16),
            4 => {
                let (x, y) = vga::cursor_position();
                x as u32 | ((y as u32) << 16)
            }
            5 if second <= 1 => { touched(); vga::set_cursor_visible(second != 0); 0 }
            _ => INVALID,
        },
        54 => {
            let x = first & 255;
            let y = (first >> 8) & 255;
            let color = (first >> 16) & 255;
            if first >> 24 != 0 || x >= 80 || y >= 25 || color > 0x7f || third > 80 - x {
                return INVALID;
            }
            if !user_buffer::readable(second, third, image_size) { return (-14i32) as u32; }
            let mut copy = [0; 80];
            if third != 0 {
                unsafe { core::ptr::copy_nonoverlapping(second as *const u8, copy.as_mut_ptr(), third as usize); }
            }
            touched();
            vga::draw_text(x as usize, y as usize, color as u8, &copy[..third as usize]);
            third
        }
        55 => {
            let x = first & 255;
            let y = (first >> 8) & 255;
            if first >> 16 != 0 || x >= 80 || y >= 25 || third > 80 - x { return INVALID; }
            let bytes = third * 2;
            if !user_buffer::readable(second, bytes, image_size) { return (-14i32) as u32; }
            let mut copy = [0; 160];
            if bytes != 0 {
                unsafe { core::ptr::copy_nonoverlapping(second as *const u8, copy.as_mut_ptr(), bytes as usize); }
            }
            // Validate every colour before touching even the first cell.
            if copy[..bytes as usize].chunks_exact(2).any(|cell| cell[1] > 0x7f) { return INVALID; }
            touched();
            for (index, cell) in copy[..bytes as usize].chunks_exact(2).enumerate() {
                vga::draw_text(x as usize + index, y as usize, cell[1], &cell[..1]);
            }
            third
        }
        56 => crate::speaker::beep(first, second),
        _ => u32::MAX,
    }
}
