// Shell-only installation state. Destructive tests attach private QEMU fixtures.
use crate::{
    blockdev::BlockDevice,
    bootdisk::{self, InstallPlan, Kernel},
    diskfs,
    diskfs::DISK,
    fat16::{Error, Result},
};
const MBR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/disk_mbr.bin"));
const STAGE2: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/disk_stage2.bin"));
static mut PENDING: Option<InstallPlan> = None;
struct AppCopy {path:crate::fs_path::Path,frames:Option<crate::loader::Frames>,size:usize}
static mut COPIES:[Option<AppCopy>;64]=[const {None};64];
extern "C" {
    static _kernel_start: u8;
    static _kernel_file_end: u8;
    static _kernel_end: u8;
    static _kernel_snapshot_start: u8;
    fn _start();
}
fn image() -> Result<(&'static [u8], Kernel)> {
    let start = core::ptr::addr_of!(_kernel_start) as u32;
    let bytes = core::ptr::addr_of!(_kernel_file_end) as u32 - start;
    let memory = core::ptr::addr_of!(_kernel_end) as u32 - start;
    if start != bootdisk::LOAD || bytes == 0 || bytes as usize > bootdisk::MAX_IMAGE {
        return Err(Error::InvalidKernel);
    }
    let data = unsafe {
        core::slice::from_raw_parts(core::ptr::addr_of!(_kernel_snapshot_start), bytes as usize)
    };
    let kernel = Kernel {
        entry: _start as *const () as u32,
        bytes,
        memory,
        checksum: bootdisk::checksum(data),
    };
    kernel.validate()?;
    Ok((data, kernel))
}
pub fn cancel() {
    unsafe {
        *core::ptr::addr_of_mut!(PENDING) = None;
        *core::ptr::addr_of_mut!(COPIES) = [const {None};64];
    }
}
pub fn prepare_friendly(destination:Option<usize>)->Result<InstallPlan> {
    cancel();
    if destination.is_none()&&diskfs::boot_selected() {return Err(Error::FirmwareDestination);}
    let mut copies:[Option<AppCopy>;64]=[const {None};64];
    if diskfs::is_mounted() {
        let root=crate::fs_path::Path::parse("/APPS",&crate::fs_path::Path::ROOT)?;
        let tree=match crate::app_tree::Tree::scan(root) {Ok(tree)=>Some(tree),Err(Error::NotFound)=>None,Err(e)=>return Err(e)};
        if let Some(tree)=tree {
            for (index,item) in tree.items[..tree.len].iter().enumerate() {
                let frames=if item.directory {None}else {
                    let frames=crate::loader::Frames::new(item.size.max(1).div_ceil(4096)).ok_or(Error::OutOfMemory)?;
                    let mut path=[0;105];let n=item.path.encode(&mut path);
                    let data=unsafe{core::slice::from_raw_parts_mut(frames.base() as *mut u8,(frames.top()-frames.base()) as usize)};
                    if diskfs::read_image(core::str::from_utf8(&path[..n]).unwrap(),data)?!=item.size {return Err(Error::Corrupt);}
                    Some(frames)
                };
                copies[index]=Some(AppCopy{path:item.path,frames,size:item.size});
            }
        }
        if destination.is_none() {diskfs::unmount()?;}
    }
    if let Some(number)=destination {diskfs::use_drive(number,false)?;}
    let plan=prepare()?;
    let count=copies.iter().flatten().count();
    crate::println!("Application files to copy from /APPS: {} (matching destination files are replaced).",count);
    if count==0 {crate::println!("No application source is mounted; this is a kernel-only installation.");}
    unsafe {*core::ptr::addr_of_mut!(COPIES)=copies;}
    Ok(plan)
}
pub fn prepare() -> Result<InstallPlan> {
    cancel();
    if diskfs::is_mounted() {
        return Err(Error::Mounted);
    }
    let (_, kernel) = image()?;
    diskfs::init_disk()?;
    let mut plan = bootdisk::plan(&DISK, kernel)?;
    plan.target = diskfs::target()?;
    unsafe {
        *core::ptr::addr_of_mut!(PENDING) = Some(plan);
    }
    Ok(plan)
}
pub fn confirm() -> Result<()> {
    let plan = unsafe { *core::ptr::addr_of!(PENDING) };
    let copies=unsafe {core::mem::replace(&mut *core::ptr::addr_of_mut!(COPIES),[const {None};64])};
    cancel();
    let plan = plan.ok_or(Error::InstallConfirmationRequired)?;
    if diskfs::is_mounted() {
        return Err(Error::Mounted);
    }
    diskfs::init_disk()?;
    if DISK.block_count() != plan.disk_sectors || diskfs::target()? != plan.target {
        return Err(Error::Io);
    }
    let (bytes, kernel) = image()?;
    if kernel != plan.kernel {
        return Err(Error::InvalidKernel);
    }
    bootdisk::install(&DISK, plan, bytes, MBR, STAGE2)?;
    if copies.iter().any(Option::is_some) {
        let result=(|| {
            let fs=crate::fat16::Fat16::new(&DISK)?;
            match fs.mkdir("/APPS") {Ok(())|Err(Error::Exists)=>(),Err(e)=>return Err(e)}
            for app in copies.iter().flatten() {
                let mut path=[0u8;105];let n=app.path.encode(&mut path);
                let path=core::str::from_utf8(&path[..n]).map_err(|_|Error::InvalidName)?;
                if let Some(frames)=&app.frames {
                    match fs.create(path) {Ok(())|Err(Error::Exists)=>(),Err(e)=>return Err(e)}
                    let data=unsafe{core::slice::from_raw_parts(frames.base() as *const u8,app.size)};
                    fs.save_image(path,data)?;
                }else {match fs.mkdir(path) {Ok(())|Err(Error::Exists)=>(),Err(e)=>return Err(e)}}
            }Ok(())
        })();
        if let Err(error)=result {crate::println!("Kernel installed, but copying applications failed: {}",error.message());return Err(error);}
    }
    Ok(())
}
