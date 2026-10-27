#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
use xinyi::window::{self,Draw};
fn main()->u32 {
    let mut args=[0;128];let n=xinyi::arguments(&mut args);
    let fault=n==5&&&args[..5]==b"fault";
    assert_eq!(xinyi::syscall(60,0x100000,3,0),-14);
    assert_eq!(window::open(b"bad\n"),-22);
    let h=window::open(b"Window API checks");assert!(h>0);let h=h as u32;
    if fault {unsafe {core::arch::asm!("ud2",options(noreturn));}}
    assert_eq!(window::open(b"second"),-16);
    assert_eq!(xinyi::syscall(62,h,u32::MAX,0),0);
    assert_eq!(xinyi::write(b"hidden text"),-16);
    assert_eq!(xinyi::syscall(61,h,0x40000000,0),-14);
    assert_eq!(xinyi::syscall(61,h,0x100000,0),-14);
    assert_eq!(xinyi::syscall(62,h,0x100000,1),-14);
    assert_eq!(xinyi::syscall(62,h,0,17),-22);
    assert_eq!(window::draw(h,&[Draw::rect(639,399,2,1,0)]),-22);
    assert_eq!(window::draw(h,&[Draw::rect(0,0,8,8,0x1000000)]),-22);
    let bad=Draw{kind:1,x:0,y:0,pointer:0x100000,length:1,..Draw::default()};
    assert_eq!(window::draw(h,&[Draw::rect(0,0,8,8,0x00ff00),bad]),-14);
    assert_eq!(window::draw(h,&[Draw::rect(8,40,16,16,0xff0000)]),1);
    assert_eq!(window::present(h),0);xinyi::sleep_ms(2000);
    assert_eq!(window::close(h),0);
    let next=window::open(b"Reopened window");assert!(next>0&&next as u32!=h);
    assert_eq!(xinyi::syscall(63,h,0,0),-9);assert_eq!(window::close(next as u32),0);
    xinyi::print("Window API checks passed!\n");0
}
