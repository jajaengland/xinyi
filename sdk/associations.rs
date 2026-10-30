//! Filesystem manifest associations. No implicit executable or unknown-file fallback.
use super::{app_manifest::{self,Manifest},directory,files};
pub const NO_HANDLER:i32=-2;
pub const AMBIGUOUS:i32=-17;
pub fn open(path:&[u8],volume:u32)->i32 {
    if super::permissions::current()&super::permissions::LAUNCH as i32==0 {return -13;}
    if path.is_empty()||path.len()>105||!path.iter().all(|b|(33..=126).contains(b)) {return -22;}
    let leaf=path.rsplit(|b|matches!(*b,b'/'|b'\\')).next().unwrap();
    let ext=leaf.rsplit(|b|*b==b'.').next().filter(|_|leaf.contains(&b'.')).unwrap_or(b"");
    if ext.eq_ignore_ascii_case(b"BIN")||ext.eq_ignore_ascii_case(b"ELF") {return -22;}
    let mut offset=0;let mut selected=None;let mut candidates=0;
    loop {
        if directory::volume()!=volume {return -116;}
        let mut list=directory::Listing::new();let n=directory::list(b"/APPS",offset,&mut list);
        if n<0 {return n;}
        for e in &list.entries[..list.count as usize] {
            let name=&e.name[..e.length as usize];
            let mut executable=[0;30];executable[..6].copy_from_slice(b"/APPS/");
            let executable_len;
            if e.flags&1!=0 {
                if name.is_empty()||name.len()>8||!name[0].is_ascii_alphabetic()||!name.iter().all(|b|b.is_ascii_alphanumeric()||*b==b'_') {continue;}
                executable[6..6+name.len()].copy_from_slice(name);executable[6+name.len()]=b'/';
                executable[7+name.len()..7+name.len()*2].copy_from_slice(name);let at=7+name.len()*2;executable[at]=b'.';
                executable[at+1..at+4].copy_from_slice(b"BIN");executable_len=at+4;
                let size=files::stream_size_on(&executable[..executable_len],volume);
                if size==-116 {return size;}
                if size==-2 {executable[at+1..at+4].copy_from_slice(b"ELF");}
                else if size<0 {continue;}
            }else {
                if !app_manifest::executable(name) {continue;}
                executable[6..6+name.len()].copy_from_slice(name);executable_len=6+name.len();
            }
            let leaf=executable[..executable_len].rsplit(|b|*b==b'/').next().unwrap();
            candidates+=1;if candidates>16 {return -27;}

            let mut manifest_path=[0;30];let length=app_manifest::sidecar(&executable[..executable_len],&mut manifest_path).unwrap();
            let mut bytes=[0;app_manifest::MAX_BYTES];let count=files::stream_size_on(&manifest_path[..length],volume);
            if count==-116 {return count;}
            if count<=0||count as usize>bytes.len() {continue;}
            let n=files::read_at_on(&manifest_path[..length],0,&mut bytes[..count as usize],volume);
            if n==-116 {return n;}if n!=count {continue;}
            let Ok(manifest)=Manifest::parse(&bytes[..count as usize],leaf) else {continue;};
            if manifest.handles(ext) {
                if selected.is_some() {return AMBIGUOUS;}
                selected=Some((manifest,executable,executable_len));
            }
        }
        offset+=list.count as u16;if offset as u32>=list.total {break;}if list.count==0||offset>=64 {return -27;}
    }
    let Some((manifest,executable,executable_len))=selected else {return NO_HANDLER;};
    let mut command=[0;128];let prefix=manifest.access.prefix();
    let n=prefix.len()+executable_len+1+path.len();if n>command.len(){return -22;}
    command[..prefix.len()].copy_from_slice(prefix);let mut at=prefix.len();
    command[at..at+executable_len].copy_from_slice(&executable[..executable_len]);at+=executable_len;command[at]=b' ';at+=1;
    command[at..n].copy_from_slice(path);
    super::launch_on(&command[..n],volume)
}
