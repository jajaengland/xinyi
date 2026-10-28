#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
use alloc::vec::Vec;
#[path="../../sdk/xinyi.rs"]mod xinyi;
#[global_allocator]static HEAP:xinyi::heap::Allocator=xinyi::heap::Allocator::new();
#[alloc_error_handler]fn oom(_:core::alloc::Layout)->!{xinyi::exit(12)}
#[repr(C)]struct Request{path:u32,path_length:u32,data:u32,length:u32,volume:u32}
fn main()->u32{let v=xinyi::directory::volume();let mut data=Vec::new();data.resize(65536,0);for(i,b)in data.iter_mut().enumerate(){*b=(i%95+32) as u8;}
 assert_eq!(xinyi::files::save(b"/BIG.TXT",&data),65536);assert_eq!(xinyi::files::size_on(b"/BIG.TXT",v),65536);let mut out=Vec::new();out.resize(65536,0);assert_eq!(xinyi::files::read(b"/BIG.TXT",&mut out),65536);assert_eq!(out,data);let mut tiny=[0x55;16];assert_eq!(xinyi::files::read(b"/BIG.TXT",&mut tiny),-28);assert_eq!(tiny,[0x55;16]);assert_eq!(xinyi::files::save(b"/EMPTY.TXT",b""),0);assert_eq!(xinyi::files::read(b"/EMPTY.TXT",&mut[]),0);
 assert_eq!(xinyi::files::save_on(b"/BIG.TXT",b"wrong",v.wrapping_sub(1)),-116);assert_eq!(xinyi::files::read_on(b"/BIG.TXT",&mut out,v.wrapping_sub(1)),-116);assert_eq!(xinyi::files::save(b"/TOOBIG.TXT",&vec_bytes(65537)),-22);
 let mut legacy=[0;1024];assert_eq!(xinyi::read_file(b"/BIG.TXT",&mut legacy),-27);assert_eq!(xinyi::save_file(b"/BIG.TXT",b"wrong"),-27);
 let mut r=Request{path:b"/BIG.TXT".as_ptr() as u32,path_length:8,data:0x40000000,length:65536,volume:v};let ptr=&mut r as *mut _ as u32;assert_eq!(xinyi::syscall(88,ptr,20,0),-14);assert_eq!(xinyi::syscall(88,0x100000,20,0),-14);assert_eq!(xinyi::syscall(88,ptr,19,0),-22);assert_eq!(xinyi::syscall(89,ptr,20,1),-22);r.length=65537;assert_eq!(xinyi::syscall(89,&r as *const _ as u32,20,0),-22);assert_eq!(xinyi::files::read(b"/BIG.TXT",&mut out),65536);assert_eq!(out,data);
 assert_eq!(xinyi::files::save(b"/BINARY.TXT",b"bad\0text"),8);data.fill(9);assert_eq!(xinyi::files::save(b"/TABS.TXT",&data),65536);
 xinyi::print("Large file API checks passed!\n");0}
fn vec_bytes(n:usize)->Vec<u8>{let mut v=Vec::new();v.resize(n,0);v}
