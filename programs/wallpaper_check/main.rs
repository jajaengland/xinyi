#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
fn main()->u32{
 use xinyi::{wallpaper::{self,Request},syscall};let mut args=[0;128];let n=xinyi::arguments(&mut args).max(0) as usize;
 let mut r=Request::new();assert!(wallpaper::get(&mut r)>=0);
 if &args[..n]==b"denied"{assert_eq!(wallpaper::apply(&r,false),-13);return 0;}
 assert_eq!(syscall(106,0,1,111),-22);assert_eq!(syscall(106,5,1,112),-22);assert_eq!(syscall(106,0,u32::MAX,112),-14);assert_eq!(syscall(106,1,u32::MAX,112),-14);
 r.volume=r.volume.wrapping_add(1);assert_eq!(wallpaper::apply(&r,false),-116);r.volume=xinyi::directory::volume();r.mode=3;assert_eq!(wallpaper::apply(&r,false),-22);r.mode=0;r.length=u16::MAX;assert_eq!(wallpaper::apply(&r,false),-22);
 xinyi::print("Wallpaper boundary checks passed!\n");0
}
