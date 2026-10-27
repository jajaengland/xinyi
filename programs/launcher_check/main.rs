#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
fn main()->u32 {
    let mut data=[0;1024];let size=xinyi::read_file(b"/APPS/HELLO.BIN",&mut data);assert!(size>0);
    assert_eq!(xinyi::save_file(b"/APPS/ADDED.BIN",&data[..size as usize]),size);
    xinyi::print("Added application from a running program.\n");0
}
