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
    }
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
    bootdisk::install(&DISK, plan, bytes, MBR, STAGE2)
}
