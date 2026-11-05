//! Advisory metadata review; the kernel remains the installer authority.
use crate::xinyi::{self,app_manifest::{Manifest,Access}};
pub fn load(id:&[u8],volume:u32)->Result<Manifest,i32> {
    if id.is_empty()||id.len()>8||!id[0].is_ascii_alphabetic()||!id.iter().all(|b|b.is_ascii_alphanumeric()||*b==b'_') {return Err(-8);}
    if volume!=xinyi::directory::volume() {return Err(-116);}
    let mut path=[0;32];let n=id.len();path[..n].copy_from_slice(id);path[n]=b'/';path[n+1..n+1+n].copy_from_slice(id);path[2*n+1]=b'.';
    let size=2*n+5;let mut executable=[0;12];executable[..n].copy_from_slice(id);executable[n]=b'.';
    let mut found=false;
    for ext in [b"BIN",b"ELF"] {
        path[2*n+2..size].copy_from_slice(ext);
        let bytes=xinyi::files::stream_size_on(&path[..size],volume);
        if bytes>=0 {executable[n+1..n+4].copy_from_slice(ext);found=true;break;}
        if bytes != -2 {return Err(bytes);}
    }
    if !found {return Err(-8);}
    path[2*n+2..size].copy_from_slice(b"APP");
    let bytes=xinyi::files::stream_size_on(&path[..size],volume);
    if bytes<0 {return Err(bytes);}if bytes==0||bytes>512 {return Err(-8);}
    let mut data=[0;512];let read=xinyi::files::read_at_on(&path[..size],0,&mut data[..bytes as usize],volume);
    if read!=bytes {return Err(if read<0 {read}else {-8});}
    if volume!=xinyi::directory::volume() {return Err(-116);}
    Manifest::parse(&data[..bytes as usize],&executable[..n+4]).map_err(|_|-8)
}
pub fn profile(access:Access)->(&'static[u8],&'static[u8]) {
    match access {
        Access::Trusted=>(b"Full access",b"Can read/write files, launch apps, use shared data, sound and shell services."),
        Access::ReadOnly=>(b"Read-only",b"Can read any file on this drive. Cannot write files or launch apps."),
        Access::Media=>(b"Media",b"Can read files and play audio. Cannot write files or launch apps."),
        Access::Isolated=>(b"Isolated",b"No file, audio, shared clipboard or app-launch access."),
    }
}
