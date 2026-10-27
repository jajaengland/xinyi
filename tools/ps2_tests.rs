#![allow(dead_code)]
use std::{collections::VecDeque,sync::Mutex};
static FIFO:Mutex<VecDeque<(u8,u8)>>=Mutex::new(VecDeque::new());
static KEYS:Mutex<Vec<u8>>=Mutex::new(Vec::new());
static POINTER:Mutex<Vec<u8>>=Mutex::new(Vec::new());
static BAD:Mutex<[usize;2]>=Mutex::new([0;2]);
mod bios {
    pub unsafe fn input(port:u16)->u8 {let mut fifo=super::FIFO.lock().unwrap();match port {0x64=>fifo.front().map_or(0,|v|v.0),0x60=>fifo.pop_front().unwrap().1,_=>0}}
    pub unsafe fn output(_:u16,_:u8) {}
}
mod keyboard {pub fn push_scancode(b:u8) {super::KEYS.lock().unwrap().push(b);}pub fn bad_byte() {super::BAD.lock().unwrap()[0]+=1;}}
mod mouse {pub fn receive(b:u8) {super::POINTER.lock().unwrap().push(b);}pub fn bad_byte() {super::BAD.lock().unwrap()[1]+=1;}}
#[path="../src/ps2.rs"] mod ps2;
#[test] fn shared_fifo_routes_aux_bytes_rejects_corruption_and_bounds_irq_work() {
    FIFO.lock().unwrap().extend([(1,0x1e),(0x21,8),(1,0x9e),(0x21,2),(0x21,3),(0xc1,0xaa),(0xe1,0xbb)]);
    ps2::drain();assert_eq!(*KEYS.lock().unwrap(),[0x1e,0x9e]);assert_eq!(*POINTER.lock().unwrap(),[8,2,3]);assert_eq!(*BAD.lock().unwrap(),[1,1]);
    FIFO.lock().unwrap().extend(std::iter::repeat_n((0x21,0),40));ps2::drain();assert_eq!(FIFO.lock().unwrap().len(),8);
}
