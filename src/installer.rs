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
struct AppCopy {name:[u8;11],frames:crate::loader::Frames,size:usize}
static mut COPIES:[Option<AppCopy>;16]=[const {None};16];
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
        *core::ptr::addr_of_mut!(COPIES) = [const {None};16];
    }
}
pub fn prepare_friendly(destination:Option<usize>)->Result<InstallPlan> {
    cancel();
    if destination.is_none()&&diskfs::boot_selected() {return Err(Error::FirmwareDestination);}
    let mut copies:[Option<AppCopy>;16]=[const {None};16];
    if diskfs::is_mounted() {
        let mut names=[([0;11],0usize);16];let mut count=0;let mut overflow=false;
        match diskfs::list("/APPS",|name,size,directory| {if !directory {if count==16 {overflow=true;}else{names[count]=(*name,size);count+=1;}}}) {
            Ok(())|Err(Error::NotFound)=>(),Err(error)=>return Err(error),
        }
        if overflow {return Err(Error::Full);}
        for (index,(name,size)) in names[..count].iter().enumerate() {
            if *size>crate::fat16::MAX_PROGRAM_SIZE {return Err(Error::ProgramTooLarge);}
            let frames=crate::loader::Frames::new((*size).max(1).div_ceil(4096)).ok_or(Error::OutOfMemory)?;
            let mut path=[0u8;18];path[..6].copy_from_slice(b"/APPS/");
            let mut text=[0;12];let length=crate::fat16::display_name(name,&mut text);path[6..6+length].copy_from_slice(&text[..length]);
            let data=unsafe {core::slice::from_raw_parts_mut(frames.base() as *mut u8,(frames.top()-frames.base()) as usize)};
            let actual=diskfs::read_program(core::str::from_utf8(&path[..6+length]).map_err(|_|Error::InvalidName)?,data)?;
            if actual!=*size {return Err(Error::Corrupt);}
            copies[index]=Some(AppCopy{name:*name,frames,size:*size});
        }
        if destination.is_none() {diskfs::unmount()?;}
    }
    if let Some(number)=destination {diskfs::use_drive(number,false)?;}
    let plan=prepare()?;
    let count=copies.iter().flatten().count();
    crate::println!("Applications to copy from /APPS: {} (matching destination apps are replaced).",count);
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
    let copies=unsafe {core::mem::replace(&mut *core::ptr::addr_of_mut!(COPIES),[const {None};16])};
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
                let mut path=[0u8;18];path[..6].copy_from_slice(b"/APPS/");let mut text=[0;12];
                let length=crate::fat16::display_name(&app.name,&mut text);path[6..6+length].copy_from_slice(&text[..length]);
                let path=core::str::from_utf8(&path[..6+length]).map_err(|_|Error::InvalidName)?;
                match fs.create(path) {Ok(())|Err(Error::Exists)=>(),Err(e)=>return Err(e)}
                let data=unsafe {core::slice::from_raw_parts(app.frames.base() as *const u8,app.size)};
                fs.write_program(path,data)?;
            }Ok(())
        })();
        if let Err(error)=result {crate::println!("Kernel installed, but copying applications failed: {}",error.message());return Err(error);}
    }
    Ok(())
}
