#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
fn main()->u32 {
    let path=b"api.txt";let p=path.as_ptr() as u32;let length=path.len() as u32;
    let mut args=[0;128];
    if xinyi::arguments(&mut args)!=5||&args[..5]!=b"probe" {return 1;}
    if xinyi::syscall(59,0x40000000,128,0)!=-14 {return 2;}
    if xinyi::syscall(57,p,length,0x40000000)!=-14 {return 3;}
    if xinyi::syscall(57,0x100000,length,0x403fc000)!=-14 {return 4;}
    if xinyi::syscall(57,p,106,0x403fc000)!=-22 {return 5;}
    if xinyi::syscall(58,p,length|(1025<<16),0x403fc000)!=-22 {return 6;}
    if xinyi::syscall(58,p,length|(10<<16),0x100000)!=-14 {return 7;}
    if xinyi::save_file(path,b"api works")!=9 {return 8;}
    let mut data=[0xaa;1024];
    if xinyi::read_file(path,&mut data)!=9||&data[..9]!=b"api works"||data[9..].iter().any(|&v|v!=0xaa) {return 9;}
    if xinyi::syscall(58,p,length,0xffffffff)!=0 {return 10;}
    if xinyi::read_file(path,&mut data)!=0 {return 11;}
    xinyi::print("File API checks passed!\n");0
}
