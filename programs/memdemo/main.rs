#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use alloc::{boxed::Box, vec::Vec};
#[global_allocator]
static ALLOCATOR: xinyi::heap::Allocator = xinyi::heap::Allocator::new();
static mut INITIAL: u32 = 12345;
static mut ZERO: [u8; 8192] = [0; 8192];
#[alloc_error_handler]
fn allocation_failed(_: core::alloc::Layout) -> ! {
    xinyi::print("Allocation failed.\n");
    xinyi::exit(12)
}
fn main() -> u32 {
    unsafe {
        assert_eq!(core::ptr::addr_of!(INITIAL).read_volatile(), 12345);
        assert!((*core::ptr::addr_of!(ZERO)).iter().all(|&b| b == 0));
        core::ptr::addr_of_mut!(INITIAL).write_volatile(54321);
        (*core::ptr::addr_of_mut!(ZERO))[0] = 42;
    }
    let mut values = Vec::new();
    for n in 0..4096u32 {
        values.push(n);
    }
    assert_eq!(values.iter().copied().sum::<u32>(), 4095 * 4096 / 2);
    let text = Box::new(*b"ELF data/BSS and heap checks passed!\n");
    xinyi::write(&*text);
    drop(values);
    drop(text);
    let mut second = Vec::new();
    second.resize(20000, 7u8);
    assert!(second.iter().all(|&v| v == 7));
    0
}
