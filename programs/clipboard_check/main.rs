#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
fn main()->u32 {
 let mut args=[0;128];let argc=xinyi::arguments(&mut args);if argc==4&&&args[..4]==b"fill" {assert_eq!(xinyi::clipboard_set(&[b'a';1024]),1024);xinyi::print("Clipboard filled to capacity\n");return 0;}if argc==5&&&args[..5]==b"empty" {assert_eq!(xinyi::clipboard_get(&mut []),0);xinyi::print("Clipboard empty after boot\n");return 0;}
 let mut clip=[0;1024];assert_eq!(xinyi::clipboard_set(b"shared text"),11);
 assert_eq!(xinyi::syscall(67,0x100000,1,0),-14);assert_eq!(xinyi::syscall(67,0x40000000,1025,0),-22);assert_eq!(xinyi::clipboard_set(&[0]),-22);
 assert_eq!(xinyi::clipboard_get(&mut clip),11);assert_eq!(&clip[..11],b"shared text");assert_eq!(xinyi::clipboard_get(&mut []),11);
 assert_eq!(xinyi::clipboard_get(&mut [0;2]),-28);assert_eq!(xinyi::syscall(68,0x40000000,1024,0),-14);assert_eq!(xinyi::syscall(68,0,1025,0),-22);assert_eq!(xinyi::syscall(68,0,0,1),-22);
 assert_eq!(xinyi::clipboard_set(&[b'a';1024]),1024);assert_eq!(xinyi::clipboard_get(&mut clip),1024);assert!(clip.iter().all(|&b|b==b'a'));assert_eq!(xinyi::clipboard_set(b""),0);
 let text=b"hello"; assert_eq!(xinyi::syscall(69,0,0,0),-38);
 assert_eq!(xinyi::syscall(70,0x40000000,105,0),-14);assert_eq!(xinyi::syscall(70,0,104,0),-22);assert_eq!(xinyi::syscall(71,text.as_ptr() as u32,text.len() as u32,1),-22);assert_eq!(xinyi::syscall(71,0x100000,1,0),-14);assert_eq!(xinyi::launch(b"hello"),-16);
 xinyi::print("Clipboard/terminal API checks passed!\n");0
}
