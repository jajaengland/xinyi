#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use alloc::{boxed::Box, vec::Vec};
#[global_allocator]
static ALLOCATOR: xinyi::heap::Allocator = xinyi::heap::Allocator::new();
static mut INIT: u32 = 0x12345678;
static mut DATA: [u8; 8192] = [0; 8192];
static mut EVENT: xinyi::window::Event = xinyi::window::Event {
    kind: 0,
    code: 0,
    x: 0,
    y: 0,
    buttons: 0,
};
static mut LIST: xinyi::directory::Listing = xinyi::directory::Listing::new();
#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! {
    xinyi::exit(12)
}
fn main() -> u32 {
    let mut args = [0; 128];
    let n = xinyi::arguments(&mut args).max(0) as usize;
    let mode = &args[..n];
    let pid = xinyi::getpid();
    unsafe {
        assert_eq!(core::ptr::addr_of!(INIT).read_volatile(), 0x12345678);
        for i in 0..8192 {
            assert_eq!(
                core::ptr::addr_of!(DATA)
                    .cast::<u8>()
                    .add(i)
                    .read_volatile(),
                0
            );
        }
        core::ptr::addr_of_mut!(INIT).write_volatile(pid);
    }
    if mode == b"unmapped" {
        core::hint::black_box(unsafe { (0x40100000 as *const u32).read_volatile() });
        return 20;
    }
    if mode == b"code" {
        unsafe {
            (0x40000000 as *mut u8).write_volatile(0);
        }
        return 21;
    }
    if mode == b"guard" {
        core::hint::black_box(unsafe { (0x40200000 as *const u32).read_volatile() });
        return 22;
    }
    if mode == b"freed" {
        assert_eq!(xinyi::heap::program_break(0x40102000), Ok(0x40102000));
        assert_eq!(xinyi::heap::program_break(0x40100000), Ok(0x40100000));
        core::hint::black_box(unsafe { (0x40101000 as *const u32).read_volatile() });
        return 23;
    }
    if mode == b"cap" {
        assert_eq!(xinyi::syscall(87, 0, 1, 0), -22);
        assert_eq!(xinyi::heap::program_break(0x40200001), Err(-22));
        assert_eq!(xinyi::heap::program_break(0x400fffff), Err(-22));
        assert_eq!(xinyi::heap::program_break(0x40200000), Ok(0x40200000));
        for i in 0..256 {
            assert_eq!(
                unsafe { ((0x40100000 + i * 4096) as *const u8).read_volatile() },
                0
            );
        }
        unsafe {
            (0x401fffff as *mut u8).write_volatile(99);
            assert_eq!((0x401fffff as *const u8).read_volatile(), 99);
        }
        assert_eq!(xinyi::heap::program_break(0x40100000), Ok(0x40100000));
        xinyi::print("Heap cap checks passed!\n");
        return 0;
    }
    let mut bytes = Vec::new();
    bytes.resize(32768, pid as u8);
    let mut words = Vec::new();
    for i in 0..8192u32 {
        words.push(i);
    }
    drop(words);
    let aligned = core::alloc::Layout::from_size_align(4000, 4096).unwrap();
    use core::alloc::GlobalAlloc;
    let p = unsafe { ALLOCATOR.alloc(aligned) };
    assert!(!p.is_null());
    assert_eq!(p as usize % 4096, 0);
    unsafe {
        core::ptr::write_bytes(p, 0x5a, 4000);
        ALLOCATOR.dealloc(p, aligned);
    }
    for _ in 0..100 {
        let mut blocks = Vec::new();
        for i in 0..32 {
            blocks.push(Box::new([i as u8; 512]));
        }
        for (i, b) in blocks.iter().enumerate() {
            assert!(b.iter().all(|&v| v == i as u8));
        }
        drop(blocks);
    }
    let too_big = core::alloc::Layout::from_size_align(1048577, 8).unwrap();
    assert!(unsafe { ALLOCATOR.alloc(too_big) }.is_null());
    // Fill the bounded heap with small allocations, then coalesce enough free
    // space for a larger block without growing the break further.
    let mut blocks = Vec::with_capacity(64);
    let block_layout = core::alloc::Layout::from_size_align(32768, 8).unwrap();
    loop {
        let pointer = unsafe { ALLOCATOR.alloc(block_layout) };
        if pointer.is_null() {
            break;
        }
        blocks.push(pointer);
        assert!(blocks.len() < 64);
    }
    assert!(blocks.len() > 10);
    let before = xinyi::heap::program_break(0).unwrap();
    for pointer in blocks.drain(..) {
        unsafe {
            ALLOCATOR.dealloc(pointer, block_layout);
        }
    }
    let large = core::alloc::Layout::from_size_align(524288, 8).unwrap();
    let pointer = unsafe { ALLOCATOR.alloc(large) };
    assert!(!pointer.is_null());
    assert_eq!(xinyi::heap::program_break(0).unwrap(), before);
    unsafe {
        ALLOCATOR.dealloc(pointer, large);
    }
    drop(blocks);
    let mut event = Box::new(xinyi::window::Event::default());
    let mut output = Vec::new();
    output.resize(1024, 0u8);
    if mode == b"gui" || mode.is_empty() {
        let w = xinyi::window::open(b"ELF memory test");
        assert!(w > 0);
        let w = w as u32;
        for _ in 0..200 {
            unsafe {
                assert_eq!(core::ptr::addr_of!(INIT).read_volatile(), pid);
            }
            assert!(bytes.iter().all(|&b| b == pid as u8));
            assert!(xinyi::window::event(w, &mut event, false) >= 0);
            xinyi::window::draw(
                w,
                &[xinyi::window::Draw::text(
                    8,
                    8,
                    0,
                    b"Private ELF globals and heap are intact",
                )],
            );
            xinyi::window::present(w);
            xinyi::sleep_ms(20);
        }
        return 0;
    }
    unsafe {
        let buffer = &mut *core::ptr::addr_of_mut!(DATA);
        buffer[..6].copy_from_slice(b"global");
        assert_eq!(xinyi::save_file(b"/GLOBAL.TXT", &buffer[..6]), 6);
        assert_eq!(
            xinyi::read_file(b"/GLOBAL.TXT", (&mut output[..1024]).try_into().unwrap()),
            6
        );
        assert_eq!(&output[..6], b"global");
        assert_eq!(
            xinyi::directory::list(b"/", 0, &mut *core::ptr::addr_of_mut!(LIST)),
            0
        );
        assert!(xinyi::working_directory((&mut buffer[..105]).try_into().unwrap()) >= 1);
        assert_eq!(xinyi::clipboard_set(b"heap clip"), 9);
        assert_eq!(xinyi::clipboard_get(&mut output[..1024]), 9);
        assert_eq!(&output[..9], b"heap clip");
        let w = xinyi::window::open(b"Global syscall test");
        assert!(w > 0);
        let w = w as u32;
        assert!(xinyi::window::event(w, &mut *core::ptr::addr_of_mut!(EVENT), false) >= 0);
        assert_eq!(xinyi::window::close(w), 0);
    }
    assert_eq!(xinyi::syscall(4, 1, 0x401fffff, 2), -14);
    assert_eq!(xinyi::syscall(4, 1, 0x40200000, 1), -14);
    assert_eq!(xinyi::syscall(68, 0x40000000, 1024, 0), -14);
    xinyi::print("ELF memory API checks passed!\n");
    0
}
