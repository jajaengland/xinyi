#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
use alloc::vec::Vec;
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::window::{self, Draw};
#[global_allocator]
static HEAP: xinyi::heap::Allocator = xinyi::heap::Allocator::new();
#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! {
    xinyi::exit(12)
}

const fn color(i: usize) -> u32 {
    let x = (i % 256) as u32;
    let y = (i / 256) as u32;
    (x << 16) | ((y * 2) << 8) | ((x ^ y) & 255)
}
const fn asset() -> [u32; 32768] {
    let mut pixels = [0; 32768];
    let mut i = 0;
    while i < pixels.len() {
        pixels[i] = color(i);
        i += 1;
    }
    pixels
}
// Real file-backed data, not zero-filled BSS or padding appended by a test.
static PALETTE: [u32; 32768] = asset();
static mut INITIALIZED: [u32; 2048] = [0x13579bdf; 2048];
static mut ZEROED: [u32; 16384] = [0; 16384];
fn main() -> u32 {
    let pid = xinyi::getpid();
    for i in 0..32768 {
        assert_eq!(
            unsafe { core::ptr::read_volatile(PALETTE.as_ptr().add(i)) },
            color(i)
        );
    }
    unsafe {
        let initialized = core::ptr::addr_of_mut!(INITIALIZED) as *mut u32;
        let zeroed = core::ptr::addr_of_mut!(ZEROED) as *mut u32;
        for i in 0..2048 {
            assert_eq!(initialized.add(i).read_volatile(), 0x13579bdf);
            initialized.add(i).write_volatile(pid);
        }
        for i in 0..16384 {
            assert_eq!(zeroed.add(i).read_volatile(), 0);
            zeroed.add(i).write_volatile(pid ^ i as u32);
        }
    }
    let mut heap = Vec::new();
    heap.resize(98304, 0u8);
    for (i, b) in heap.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    let mut arguments = [0; 128];
    let n = xinyi::arguments(&mut arguments).max(0) as usize;
    if &arguments[..n] == b"check" {
        xinyi::print("Large ELF checks passed!\n");
        return 0;
    }
    if &arguments[..n] == b"fault" {
        unsafe {
            core::ptr::write_volatile(PALETTE.as_ptr() as *mut u32, 0);
        }
        return 99;
    }
    let w = window::open_sized(b"Large app demo", 512, 352);
    if w < 0 {
        return 1;
    }
    let w = w as u32;
    let mut page = 0usize;
    loop {
        // Each instance must keep independent writable globals, BSS and heap.
        for i in [0, 1024, 2047] {
            assert_eq!(
                unsafe {
                    (core::ptr::addr_of!(INITIALIZED) as *const u32)
                        .add(i)
                        .read_volatile()
                },
                pid
            );
        }
        for i in [0, 8192, 16383] {
            assert_eq!(
                unsafe {
                    (core::ptr::addr_of!(ZEROED) as *const u32)
                        .add(i)
                        .read_volatile()
                },
                pid ^ i as u32
            );
        }
        assert_eq!(heap[98303], (98303 % 251) as u8);
        window::draw(
            w,
            &[
                Draw::rect(0, 0, 512, 352, 0xffffff),
                Draw::text(8, 8, 0, b"128 KiB asset + data/BSS + private 96 KiB heap"),
                Draw::text(8, 320, 0, b"Left/Right: palette page    Escape: close"),
            ],
        );
        let mut commands = [Draw::default(); 16];
        for batch in 0..128 {
            for j in 0..16 {
                let at = batch * 16 + j;
                let c = unsafe { core::ptr::read_volatile(PALETTE.as_ptr().add(page * 2048 + at)) };
                commands[j] = Draw::rect((at % 64) as u32 * 8, 48 + (at / 64) as u32 * 8, 8, 8, c);
            }
            if window::draw(w, &commands) < 0 {
                return 2;
            }
        }
        window::present(w);
        let mut event = window::Event::default();
        if window::event(w, &mut event, true) < 0 || event.kind == window::CLOSE {
            return 0;
        }
        if event.kind == window::KEY {
            match event.code as u16 {
                27 => return 0,
                xinyi::key::LEFT => page = page.wrapping_sub(1) % 16,
                xinyi::key::RIGHT => page = (page + 1) % 16,
                _ => (),
            }
        }
    }
}
