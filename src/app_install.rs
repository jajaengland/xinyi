//! App folders are validated and copied privately before their final name is published.
use crate::{diskfs,fat16::{Error,Result},fs_path::{Path,MAX_PATH},app_tree::Tree};
pub fn valid_id(id:&[u8])->bool {
    (1..=8).contains(&id.len())&&id[0].is_ascii_alphabetic()&&id.iter().all(|b|b.is_ascii_alphanumeric()||*b==b'_')
}
pub fn executable(folder:Path)->Result<([u8;MAX_PATH],usize)> {
    let mut id=[0;12];
    if folder.depth==0 {return Err(Error::InvalidName);}
    let length=crate::fat16::display_name(&folder.names[folder.depth-1],&mut id);
    if !valid_id(&id[..length]) {return Err(Error::InvalidName);}
    executable_named(folder,&id[..length])
}
fn executable_named(folder:Path,id:&[u8])->Result<([u8;MAX_PATH],usize)> {
    let mut root=[0;MAX_PATH];let n=folder.encode(&mut root);let length=id.len();
    for ext in [b"BIN",b"ELF"] {
        let count=n+1+length+4;if count>MAX_PATH {return Err(Error::PathTooDeep);}
        root[n]=b'/';root[n+1..n+1+length].copy_from_slice(id);root[n+1+length]=b'.';root[n+2+length..count].copy_from_slice(ext);
        let text=core::str::from_utf8(&root[..count]).unwrap();
        match diskfs::file_size(text) {Ok(_)=>return Ok((root,count)),Err(Error::NotFound)=>(),Err(e)=>return Err(e)}
    }Err(Error::NotFound)
}
fn text(path:&Path,bytes:&mut[u8;MAX_PATH])->usize {path.encode(bytes)}
pub fn install(source:&str)->Result<()> {install_or_update(source,false)}
pub fn update(source:&str)->Result<()> {install_or_update(source,true)}
fn install_or_update(source:&str,updating:bool)->Result<()> {
    let source=Path::parse(source,&diskfs::directory())?;
    let apps=Path::parse("/APPS",&Path::ROOT)?;
    if source.depth==0||apps.contains(&source) {return Err(Error::InvalidName);}
    let (executable,n)=executable(source)?;
    let manifest=crate::app_metadata::load(core::str::from_utf8(&executable[..n]).unwrap()).map_err(|_|Error::InvalidApplication)?.ok_or(Error::InvalidApplication)?;
    let mut destination=apps;destination.names[1]=source.names[source.depth-1];destination.depth=2;
    let mut dst=[0;MAX_PATH];let dn=text(&destination,&mut dst);let dst=core::str::from_utf8(&dst[..dn]).unwrap();
    let mut id=[0;12];let idn=crate::fat16::display_name(&destination.names[1],&mut id);
    if updating {
        diskfs::directory_exists(dst)?;
        if crate::multitask::app_running(&id[..idn]) {return Err(Error::DirectoryBusy);}
        let (old,oldn)=executable_named(destination,&id[..idn])?;
        let old=crate::app_metadata::load(core::str::from_utf8(&old[..oldn]).unwrap()).map_err(|_|Error::InvalidApplication)?.ok_or(Error::InvalidApplication)?;
        if !crate::app_manifest::permits_update(&old,&manifest) {return Err(Error::InvalidApplication);}
        vacant("/APPS/_BACKUP")?;
        match diskfs::file_size("/APPS/_UPD.ID") {Err(Error::NotFound)=>(),Ok(_)=>return Err(Error::Exists),Err(e)=>return Err(e)}
    } else {
        match diskfs::directory_exists(dst) {Ok(())=>return Err(Error::Exists),Err(Error::NotFound)=>(),Err(e)=>return Err(e)}
        let mut storage=crate::app_catalog::Storage::new().ok_or(Error::OutOfMemory)?;let catalog=storage.catalog();match crate::multi_window::collect_catalog(catalog) {Ok(())|Err(Error::NotFound)=>(),Err(e)=>return Err(e)};
        if catalog.entries[..catalog.len].iter().any(|e|e.label[..e.label_len].eq_ignore_ascii_case(&id[..idn])) {return Err(Error::Exists);}
        if catalog.len==crate::app_catalog::MAX_APPS {return Err(Error::Full);}
    }
    let tree=Tree::scan(source)?;
    let stage=Path::parse("/APPS/_STAGE",&Path::ROOT)?;
    for i in 0..tree.len {tree.mapped(i,stage)?;}
    let frames=crate::loader::Frames::new(crate::loader::MAX_BINARY_BYTES/4096).ok_or(Error::OutOfMemory)?;
    let buffer=unsafe{core::slice::from_raw_parts_mut(frames.base() as *mut u8,crate::loader::MAX_BINARY_BYTES)};
    let count=diskfs::read_image(core::str::from_utf8(&executable[..n]).unwrap(),buffer)?;
    crate::program_image::parse(&buffer[..count]).map_err(|_|Error::InvalidApplication)?;
    // Any reserved staging folder is left untouched for explicit recovery.
    match diskfs::mkdir("/APPS") {Ok(())|Err(Error::Exists)=>(),Err(e)=>return Err(e)}
    diskfs::mkdir("/APPS/_STAGE")?;
    let result=(|| {
        for i in 1..tree.len {
            let item=tree.items[i];let target=tree.mapped(i,stage)?;let mut from=[0;MAX_PATH];let mut to=[0;MAX_PATH];
            let a=item.path.encode(&mut from);let b=target.encode(&mut to);let from=core::str::from_utf8(&from[..a]).unwrap();let to=core::str::from_utf8(&to[..b]).unwrap();
            if item.directory {diskfs::mkdir(to)?;}else {
                let count=diskfs::read_image(from,buffer)?;if count!=item.size {return Err(Error::Corrupt);}
                diskfs::create(to)?;diskfs::save_image(to,&buffer[..count])?;
            }
        }
        // Reload copied metadata; a malformed folder never becomes launchable.
        let mut copied=executable;let sn=source.encode(&mut [0;MAX_PATH]);
        let tail=n-sn;copied.copy_within(sn..n,12);copied[..12].copy_from_slice(b"/APPS/_STAGE");
        let copied_path=core::str::from_utf8(&copied[..12+tail]).unwrap();
        let copied=crate::app_metadata::load(copied_path).map_err(|_|Error::InvalidApplication)?.ok_or(Error::InvalidApplication)?;
        if copied!=manifest {return Err(Error::InvalidApplication);}
        let count=diskfs::read_image(copied_path,buffer)?;
        crate::program_image::parse(&buffer[..count]).map_err(|_|Error::InvalidApplication)?;
        diskfs::sync()?;
        if updating {
            if crate::multitask::app_running(&id[..idn]) {return Err(Error::DirectoryBusy);}
            diskfs::create("/APPS/_UPD.ID")?;
            diskfs::save_large("/APPS/_UPD.ID",&id[..idn])?;
            diskfs::sync()?;
            diskfs::transfer(dst,"/APPS/_BACKUP",0)?;
            if let Err(e)=diskfs::transfer("/APPS/_STAGE",dst,0) {
                let _=diskfs::transfer("/APPS/_BACKUP",dst,0);
                return Err(e);
            }
            Ok(())
        } else {diskfs::transfer("/APPS/_STAGE",dst,0)}
    })();
    if result.is_err() && diskfs::is_mounted() && (!updating || vacant("/APPS/_BACKUP").is_ok()) {
        // Snapshot the actual partial copy, so absent files do not stop rollback.
        if let Ok(partial)=Tree::scan(stage) {let _=partial.remove(stage);}
        if updating {let _=diskfs::delete("/APPS/_UPD.ID");}
    }
    result
}
fn vacant(path:&str)->Result<()> {
    match diskfs::directory_exists(path) {Ok(())=>Err(Error::Exists),Err(Error::NotFound)=>Ok(()),Err(e)=>Err(e)}
}
/// A retained backup is explicitly finalized or restored before another update.
pub fn finish_update(source:&str,rollback:bool)->Result<()> {
    let folder=Path::parse(source,&diskfs::directory())?;
    if folder.depth!=2 || folder.names[0]!=*b"APPS       " {return Err(Error::InvalidName);}
    let mut id=[0;12];let n=crate::fat16::display_name(&folder.names[1],&mut id);
    if !valid_id(&id[..n]) {return Err(Error::InvalidName);}
    if crate::multitask::app_running(&id[..n]) {return Err(Error::DirectoryBusy);}
    let backup=Path::parse("/APPS/_BACKUP",&Path::ROOT)?;
    let size=diskfs::file_size("/APPS/_UPD.ID")?;
    if size!=n {return Err(Error::InvalidName);}
    let mut owner=[0;8];if diskfs::read_at("/APPS/_UPD.ID",0,&mut owner[..n])?!=n || !owner[..n].eq_ignore_ascii_case(&id[..n]) {return Err(Error::InvalidName);}
    if !rollback {
        match Tree::scan(backup) {Ok(tree)=>tree.remove(backup)?,Err(Error::NotFound)=>(),Err(e)=>return Err(e)}
        return diskfs::delete("/APPS/_UPD.ID");
    }
    let (exe,en)=executable_named(backup,&id[..n])?;
    let exe=core::str::from_utf8(&exe[..en]).unwrap();
    crate::app_metadata::load(exe).map_err(|_|Error::InvalidApplication)?.ok_or(Error::InvalidApplication)?;
    {
        let frames=crate::loader::Frames::new(crate::loader::MAX_BINARY_BYTES/4096).ok_or(Error::OutOfMemory)?;
        let buffer=unsafe{core::slice::from_raw_parts_mut(frames.base() as *mut u8,crate::loader::MAX_BINARY_BYTES)};
        let count=diskfs::read_image(exe,buffer)?;crate::program_image::parse(&buffer[..count]).map_err(|_|Error::InvalidApplication)?;
    }
    let mut target_bytes=[0;MAX_PATH];let tn=folder.encode(&mut target_bytes);let target=core::str::from_utf8(&target_bytes[..tn]).unwrap();
    match diskfs::directory_exists(target) {
        Err(Error::NotFound)=>{diskfs::transfer("/APPS/_BACKUP",target,0)?;return diskfs::delete("/APPS/_UPD.ID");},
        Err(e)=>return Err(e),Ok(())=>(),
    }
    executable(folder)?;vacant("/APPS/_STAGE")?;
    let mut bytes=[0;MAX_PATH];let count=folder.encode(&mut bytes);let source=core::str::from_utf8(&bytes[..count]).unwrap();
    diskfs::transfer(source,"/APPS/_STAGE",0)?;
    if let Err(e)=diskfs::transfer("/APPS/_BACKUP",source,0) {let _=diskfs::transfer("/APPS/_STAGE",source,0);return Err(e);}
    let stage=Path::parse("/APPS/_STAGE",&Path::ROOT)?;
    Tree::scan(stage)?.remove(stage)?;diskfs::delete("/APPS/_UPD.ID")
}
pub fn remove(source:&str)->Result<()> {
    let folder=Path::parse(source,&diskfs::directory())?;
    let mut id=[0;12];
    if folder.depth!=2||folder.names[0]!=*b"APPS       " {return Err(Error::InvalidName);}
    let n=crate::fat16::display_name(&folder.names[1],&mut id);
    if !valid_id(&id[..n]) {return Err(Error::InvalidName);}
    if crate::multitask::app_running(&id[..n]) {return Err(Error::DirectoryBusy);}
    executable(folder)?;
    let tree=Tree::scan(folder)?;
    let mut bytes=[0;MAX_PATH];let n=folder.encode(&mut bytes);let source=core::str::from_utf8(&bytes[..n]).unwrap();
    // Unpublish first. A failed cleanup stays out of the launcher.
    diskfs::transfer(source,"/APPS/_TRASH",0)?;
    tree.remove(Path::parse("/APPS/_TRASH",&Path::ROOT)?)
}
pub fn syscall(cs:u32,pointer:u32,size:u32,reserved:u32)->u32 {
    let Some(image)=crate::usermode::active_image(cs) else{return u32::MAX;};
    if size!=220||reserved!=0 {return (-22i32) as u32;}
    if !crate::user_memory::readable(pointer,size,image) {return (-14i32) as u32;}
    let r=unsafe{core::ptr::read_unaligned(pointer as *const crate::directory_protocol::Transfer)};
    if r.volume!=diskfs::volume_epoch() {return (-116i32) as u32;}
    let n=r.source_length as usize;
    if n==0||n>105||r.destination_length!=0||r.operation>4||!r.source[..n].iter().all(|b|(32..=126).contains(b)) {return (-22i32) as u32;}
    if !diskfs::is_mounted() {return (-19i32) as u32;}
    unsafe{core::arch::asm!("sti",options(nostack));}
    let path=core::str::from_utf8(&r.source[..n]).unwrap();
    let result=match r.operation {0=>install(path),1=>remove(path),2=>update(path),3=>finish_update(path,true),_=>finish_update(path,false)};
    unsafe{core::arch::asm!("cli",options(nostack));}
    match result {Ok(())=>0,Err(e)=>crate::files_api::error(e)}
}
