#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
fn main()->u32 {
 use xinyi::{syscall,window};let w=window::open(b"Dialog boundary checks");assert!(w>0);
 assert_eq!(syscall(105,3,1,1),-22);assert_eq!(syscall(105,0,1,0),-22);assert_eq!(syscall(105,0,1,193),-22);
 assert_eq!(syscall(105,0,u32::MAX,2),-14);
 let bad=b"bad\n";assert_eq!(syscall(105,2,bad.as_ptr() as u32,bad.len() as u32),-22);
 let message=b"This isolated program can display its own information dialog. Its file and launch privileges remain disabled. Invalid pointers, lengths, severity and nonprintable text were refused.";
 assert_eq!(syscall(105,0,message.as_ptr() as u32,message.len() as u32),0);
 let mut resets=0;while resets<2 {let mut e=window::Event::default();assert!(window::event(w as u32,&mut e,true)>=0);assert_eq!(e.kind,window::RESET);resets+=1;}
 assert_eq!(xinyi::permissions::current(),0);0
}
