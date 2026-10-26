// Mounted volume and RAM fallback. Shell-only access; no filesystem calls in IRQs.
use crate::{
    ata::AtaDisk,
    blockdev::BlockDevice,
    fat16::{self, Error, Fat16, FormatPlan, Result, MAX_FILE_SIZE},
    println, vfs,
};
pub(crate) static DISK: AtaDisk = AtaDisk::new(0);
static mut SELECTED: Option<crate::disk_target::Target> = None;
static mut MOUNTED: Option<Fat16<'static>> = None;
static mut FORMAT_PENDING: Option<FormatPlan> = None;
static mut CWD: crate::fs_path::Path = crate::fs_path::Path::ROOT;
fn reset_directory() {
    unsafe {
        *core::ptr::addr_of_mut!(CWD) = crate::fs_path::Path::ROOT;
    }
}
fn absolute(text: &str, buffer: &mut [u8; crate::fs_path::MAX_PATH]) -> Result<usize> {
    let base = unsafe { *core::ptr::addr_of!(CWD) };
    Ok(crate::fs_path::Path::parse(text, &base)?.encode(buffer))
}
pub fn pwd(buffer: &mut [u8; crate::fs_path::MAX_PATH]) -> usize {
    unsafe { (*core::ptr::addr_of!(CWD)).encode(buffer) }
}
pub fn cd(text: &str) -> Result<()> {
    let base = unsafe { *core::ptr::addr_of!(CWD) };
    let path = crate::fs_path::Path::parse(text, &base)?;
    let mut buffer = [0; crate::fs_path::MAX_PATH];
    let length = path.encode(&mut buffer);
    let name = core::str::from_utf8(&buffer[..length]).unwrap();
    let result = match unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        Some(fs) => fs.is_directory(name),
        None => vfs::is_directory(name),
    };
    finish(result)?;
    unsafe {
        *core::ptr::addr_of_mut!(CWD) = path;
    }
    Ok(())
}
pub fn target() -> Result<crate::disk_target::Target> {
    unsafe { *core::ptr::addr_of!(SELECTED) }
        .or_else(crate::pci::default_target)
        .ok_or(Error::Io)
}
pub fn restore_boot_target() {
    if let Some(target) = crate::multiboot::disk_target() {
        unsafe {
            *core::ptr::addr_of_mut!(SELECTED) = Some(target);
        }
    }
}
pub fn init_disk() -> Result<()> {
    let selected = match target() {
        Ok(target) => target,
        Err(error) => {
            DISK.unavailable();
            return Err(error);
        }
    };
    let Some(channel) = crate::pci::target_channel(selected) else {
        DISK.unavailable();
        return Err(Error::Io);
    };
    DISK.configure(channel, selected.drive());
    if DISK.init() {
        Ok(())
    } else {
        Err(Error::Io)
    }
}
pub fn report_target() {
    if let Ok(t) = target() {
        if t.bdf == u32::MAX {
            println!(
                "Selected: legacy {} {}",
                if t.secondary() {
                    "secondary"
                } else {
                    "primary"
                },
                if t.drive() == 0 { "master" } else { "slave" }
            );
        } else {
            println!(
                "Selected: PCI {:02x}:{:02x}.{} {} {}",
                t.bdf >> 16,
                (t.bdf >> 8) & 255,
                t.bdf & 255,
                if t.secondary() {
                    "secondary"
                } else {
                    "primary"
                },
                if t.drive() == 0 { "master" } else { "slave" }
            );
        }
    }
    DISK.report();
}
pub fn select_command(arguments: &str) {
    if is_mounted() {
        println!("Unmount the filesystem before changing the disk target.");
        return;
    }
    let parts: [&str; 3] = {
        let mut words = arguments.split_whitespace();
        let (Some(a), Some(b), Some(c), None) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            println!("Usage: disk select BB:DD.F|legacy primary|secondary master|slave");
            return;
        };
        [a, b, c]
    };
    let secondary = match parts[1] {
        "primary" => false,
        "secondary" => true,
        _ => {
            println!("Use primary or secondary.");
            return;
        }
    };
    let slave = match parts[2] {
        "master" => false,
        "slave" => true,
        _ => {
            println!("Use master or slave.");
            return;
        }
    };
    let t = if parts[0] == "legacy" {
        crate::disk_target::Target::legacy(secondary, slave)
    } else {
        let Some(bdf) = crate::disk_target::parse_bdf(parts[0]) else {
            println!("Invalid PCI address (example 00:1f.2).");
            return;
        };
        crate::disk_target::Target {
            bdf,
            id: crate::pci::read((bdf >> 16) as u8, (bdf >> 8) as u8, bdf as u8, 0),
            position: secondary as u8 | (slave as u8) << 1,
        }
    };
    let Some(channel) = crate::pci::target_channel(t) else {
        println!("Selection refused: unavailable PCI IDE channel. Previous target unchanged.");
        return;
    };
    let candidate = AtaDisk::at(channel, t.drive());
    if !candidate.init() {
        println!("Selection refused: disk did not identify. Previous target unchanged.");
        candidate.report();
        return;
    }
    unsafe {
        *core::ptr::addr_of_mut!(SELECTED) = Some(t);
    }
    DISK.adopt(&candidate);
    println!("Disk target selected (no disk sectors written).");
    report_target();
}
pub fn cancel_format() {
    unsafe {
        *core::ptr::addr_of_mut!(FORMAT_PENDING) = None;
    }
}
pub fn prepare_format() -> Result<FormatPlan> {
    cancel_format();
    if is_mounted() {
        return Err(Error::Mounted);
    }
    init_disk()?;
    crate::fat16::require_full_disk(&DISK)?;
    let plan = FormatPlan::new(DISK.block_count())?;
    unsafe {
        *core::ptr::addr_of_mut!(FORMAT_PENDING) = Some(plan);
    }
    Ok(plan)
}
pub fn confirm_format() -> Result<()> {
    let plan = unsafe { *core::ptr::addr_of!(FORMAT_PENDING) };
    cancel_format(); // Every attempt consumes the confirmation, including errors.
    let plan = plan.ok_or(Error::ConfirmationRequired)?;
    if is_mounted() {
        return Err(Error::Mounted);
    }
    init_disk()?;
    fat16::format_disk(&DISK, plan)
}
pub fn is_mounted() -> bool {
    unsafe { (&*core::ptr::addr_of!(MOUNTED)).is_some() }
}
pub fn mount() -> Result<u32> {
    if let Some(fs) = unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        return Ok(fs.start_sector());
    }
    init_disk()?;
    let fs = Fat16::new(&DISK)?;
    let start = fs.start_sector();
    unsafe {
        *core::ptr::addr_of_mut!(MOUNTED) = Some(fs);
    }
    reset_directory();
    Ok(start)
}
pub fn unmount() -> Result<()> {
    let ok = !is_mounted() || DISK.flush();
    unsafe {
        *core::ptr::addr_of_mut!(MOUNTED) = None;
    }
    reset_directory();
    if ok {
        Ok(())
    } else {
        Err(Error::Io)
    }
}
fn finish<T>(result: Result<T>) -> Result<T> {
    if matches!(result, Err(Error::Io | Error::Corrupt)) && is_mounted() {
        unsafe {
            *core::ptr::addr_of_mut!(MOUNTED) = None;
        }
        println!("Disk filesystem detached after error; file commands now use RAM.");
        reset_directory();
    }
    result
}
pub fn create(name: &str) -> Result<()> {
    let mut buffer = [0; crate::fs_path::MAX_PATH];
    let length = absolute(name, &mut buffer)?;
    let name = core::str::from_utf8(&buffer[..length]).unwrap();
    let result = match unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        Some(fs) => fs.create(name),
        None => vfs::create(name),
    };
    finish(result)
}
pub fn write(name: &str, data: &[u8]) -> Result<()> {
    let mut buffer = [0; crate::fs_path::MAX_PATH];
    let length = absolute(name, &mut buffer)?;
    let name = core::str::from_utf8(&buffer[..length]).unwrap();
    let result = match unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        Some(fs) => fs.write(name, data),
        None => vfs::write(name, data),
    };
    finish(result)
}
pub fn delete(name: &str) -> Result<()> {
    let mut buffer = [0; crate::fs_path::MAX_PATH];
    let length = absolute(name, &mut buffer)?;
    let name = core::str::from_utf8(&buffer[..length]).unwrap();
    let result = match unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        Some(fs) => fs.delete(name),
        None => vfs::delete(name),
    };
    finish(result)
}
pub fn read(name: &str, output: &mut [u8; MAX_FILE_SIZE]) -> Result<usize> {
    let mut buffer = [0; crate::fs_path::MAX_PATH];
    let length = absolute(name, &mut buffer)?;
    let name = core::str::from_utf8(&buffer[..length]).unwrap();
    let result = match unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        Some(fs) => fs.read(name, output),
        None => vfs::read(name, output),
    };
    finish(result)
}
pub fn mkdir(name: &str) -> Result<()> {
    let mut buffer = [0; crate::fs_path::MAX_PATH];
    let length = absolute(name, &mut buffer)?;
    let name = core::str::from_utf8(&buffer[..length]).unwrap();
    let result = match unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        Some(fs) => fs.mkdir(name),
        None => vfs::mkdir(name),
    };
    finish(result)
}
pub fn rmdir(name: &str) -> Result<()> {
    let base = unsafe { *core::ptr::addr_of!(CWD) };
    let path = crate::fs_path::Path::parse(name, &base)?;
    if path.contains(&base) {
        return Err(Error::DirectoryBusy);
    }
    let mut buffer = [0; crate::fs_path::MAX_PATH];
    let length = path.encode(&mut buffer);
    let name = core::str::from_utf8(&buffer[..length]).unwrap();
    let result = match unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        Some(fs) => fs.rmdir(name),
        None => vfs::rmdir(name),
    };
    finish(result)
}
pub fn list(path: &str, visit: impl FnMut(&[u8; 11], usize, bool)) -> Result<()> {
    let mut buffer = [0; crate::fs_path::MAX_PATH];
    let length = absolute(path, &mut buffer)?;
    let path = core::str::from_utf8(&buffer[..length]).unwrap();
    let result = match unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        Some(fs) => fs.list_entries(path, visit),
        None => vfs::list_entries(path, visit),
    };
    finish(result)
}
pub fn test() {
    // Do not re-identify a device beneath an active mounted filesystem.
    if !is_mounted() && init_disk().is_err() {
        println!("No ATA disk found (or device timed out).");
        DISK.report();
        println!("Run disk scan for PCI/controller and other drive positions.");
        return;
    }
    DISK.report();
    println!("ATA disk: {} sectors (512 bytes each)", DISK.block_count());
    let mut sector = [0; 512];
    if !DISK.read_block(0, &mut sector) {
        println!("Failed to read sector 0");
        return;
    }
    println!("Read sector 0 successfully");
    if sector[510..512] == [0x55, 0xaa] {
        println!("Valid boot signature");
    }
}

pub fn scan() {
    if is_mounted() {
        println!("Unmount the filesystem before scanning IDE channels.");
        return;
    }
    println!("IDE scan: IDENTIFY only; target is not changed.");
    let mut ide = 0;
    crate::pci::for_each_storage(|c| {
        println!(
            "PCI {:02x}:{:02x}.{} {:04x}:{:04x} class=01/{:02x} PI={:02x} cmd={:04x}",
            c.bus, c.slot, c.function, c.vendor, c.device, c.subclass, c.interface, c.command
        );
        if c.subclass != 1 {
            println!("  Not an IDE interface (AHCI/RAID are not supported).");
            return;
        }
        ide += 1;
        println!(
            "  BAR0..3 {:08x} {:08x} {:08x} {:08x}; map/pcs={:08x}",
            c.bars[0], c.bars[1], c.bars[2], c.bars[3], c.mapping
        );
        for secondary in [false, true] {
            if let Some(channel) =
                crate::pci::ide_channel(c.interface, c.command, c.bars, secondary)
            {
                for drive in 0..2 {
                    let disk = AtaDisk::at(channel, drive);
                    disk.init();
                    disk.report();
                }
            } else {
                println!(
                    "  {} channel: I/O disabled or invalid native BARs",
                    if secondary { "secondary" } else { "primary" }
                );
            }
        }
    });
    if ide == 0 {
        println!("No PCI IDE controller found; BIOS mode/PCI details above are needed.");
    }
    println!("Scan leaves the selected target unchanged. Use disk select to change it.");
}
