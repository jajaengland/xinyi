use crate::{diskfs,fat16::{Error,MAX_FILE_SIZE},user_buffer,usermode};
fn error(error:Error)->u32 {
    (match error {Error::NotFound=>-2,Error::ReadOnly=>-13,Error::TooLarge=>-27,
        Error::Full=>-28,Error::InvalidName|Error::PathTooDeep=>-22,_=>-5i32}) as u32
}
pub fn syscall(cs:u32,number:u32,first:u32,second:u32,third:u32)->u32 {
    let Some(image_size)=usermode::active_image(cs) else{return u32::MAX;};
    if number==59 {
        let (args,length)=usermode::arguments();
        if second<length as u32 {return (-22i32) as u32;}
        if !user_buffer::writable(first,length as u32) {return (-14i32) as u32;}
        if length!=0 {unsafe {core::ptr::copy_nonoverlapping(args.as_ptr(),first as *mut u8,length);}}
        return length as u32;
    }
    if !diskfs::is_mounted() {return (-19i32) as u32;}
    let path_length=if number==58 {second&0xffff}else{second};
    let length=second>>16;
    if path_length==0 || path_length>crate::fs_path::MAX_PATH as u32 || length>MAX_FILE_SIZE as u32 {return (-22i32) as u32;}
    if !user_buffer::readable(first,path_length,image_size) {return (-14i32) as u32;}
    let mut path=[0;crate::fs_path::MAX_PATH];
    unsafe {core::ptr::copy_nonoverlapping(first as *const u8,path.as_mut_ptr(),path_length as usize);}
    let Ok(path)=core::str::from_utf8(&path[..path_length as usize]) else {return (-22i32) as u32;};
    let mut data=[0;MAX_FILE_SIZE];
    if number==57 {
        if !user_buffer::writable(third,MAX_FILE_SIZE as u32) {return (-14i32) as u32;}
        unsafe {core::arch::asm!("sti",options(nostack));}
        let result=diskfs::read(path,&mut data);
        unsafe {core::arch::asm!("cli",options(nostack));}
        match result {
            Ok(size)=>{if size!=0 {unsafe {core::ptr::copy_nonoverlapping(data.as_ptr(),third as *mut u8,size);}}size as u32},
            Err(e)=>error(e),
        }
    } else {
        if !user_buffer::readable(third,length,image_size) {return (-14i32) as u32;}
        if length!=0 {unsafe {core::ptr::copy_nonoverlapping(third as *const u8,data.as_mut_ptr(),length as usize);}}
        unsafe {core::arch::asm!("sti",options(nostack));}
        let result=match diskfs::write(path,&data[..length as usize]) {
            Err(Error::NotFound)=>diskfs::create(path).and_then(|_|diskfs::write(path,&data[..length as usize])),
            result=>result,
        };
        unsafe {core::arch::asm!("cli",options(nostack));}
        match result {Ok(())=>length,Err(e)=>error(e)}
    }
}
