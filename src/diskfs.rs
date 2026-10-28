// Mounted volume and RAM fallback. Serialized kernel access; no filesystem calls in IRQs.
use crate::{
    ata::AtaDisk,
    blockdev::BlockDevice,
    fat16::{self, Error, Fat16, FormatPlan, Result, MAX_FILE_SIZE},
    println, vfs,
};
pub(crate) static DISK: AtaDisk = AtaDisk::new(0);
static mut SELECTED: Option<crate::disk_target::Target> = None;
static mut BOOT_SELECTED:bool=false;
pub fn boot_selected()->bool {unsafe {*core::ptr::addr_of!(BOOT_SELECTED)}}
static mut MOUNTED: Option<Fat16<'static>> = None;
static mut FORMAT_PENDING: Option<FormatPlan> = None;
static mut VOLUME_EPOCH:u32=1;
pub fn volume_epoch()->u32 {unsafe {VOLUME_EPOCH}}
static mut CWD: crate::fs_path::Path = crate::fs_path::Path::ROOT;
#[derive(Clone,Copy)]
struct Drive {target:crate::disk_target::Target,channel:crate::pci::IdeChannel,model:[u8;40],sectors:u32,fat:bool,apps:bool}
static mut DRIVES:[Option<Drive>;16]=[None;16];
static mut DISCOVERED:bool=false;
fn discover() {
    if unsafe {DISCOVERED} {return;}
    let mut drives=[None;16];let mut count=0;let mut ide=false;
    let mut probe=|target:crate::disk_target::Target,channel:crate::pci::IdeChannel| {
        if count==16 || drives[..count].iter().flatten().any(|d:&Drive|d.channel==channel&&d.target.drive()==target.drive()) {return;}
        let disk=AtaDisk::at(channel,target.drive());
        if disk.init() {
            let fs=Fat16::new(&disk).ok();
            drives[count]=Some(Drive{target,channel,model:disk.model_name(),sectors:disk.block_count(),fat:fs.is_some(),apps:fs.is_some_and(|fs|fs.is_directory("/APPS").is_ok())});
            count+=1;
        }
    };
    crate::pci::for_each_storage(|c| {
        if c.subclass!=1 {return;} ide=true;
        for secondary in [false,true] {if let Some(channel)=crate::pci::ide_channel(c.interface,c.command,c.bars,secondary) {
            for slave in [false,true] {probe(crate::disk_target::Target{bdf:(c.bus as u32)<<16|(c.slot as u32)<<8|c.function as u32,
                id:(c.device as u32)<<16|c.vendor as u32,position:secondary as u8|(slave as u8)<<1},channel);}
        }}
    });
    if !ide {for secondary in [false,true] {for slave in [false,true] {probe(crate::disk_target::Target::legacy(secondary,slave),
        if secondary {crate::pci::IdeChannel::SECONDARY}else{crate::pci::IdeChannel::PRIMARY});}}}
    unsafe {DRIVES=drives;DISCOVERED=true;}
}
pub fn list_drives() {
    discover();let drives=unsafe {*core::ptr::addr_of!(DRIVES)};
    println!("Available drives (use NUMBER to open):");
    if crate::bios_disk::BOOT.init() {
        println!("  0: BIOS boot drive - {} MiB{}",crate::bios_disk::BOOT.block_count()/2048,if boot_selected() {" [selected]"}else{""});
    }
    for (index,drive) in drives.iter().enumerate() {if let Some(drive)=drive {
        println!("  {}: {} - {} MiB {}{}",index+1,core::str::from_utf8(&drive.model).unwrap_or("ATA disk").trim(),drive.sectors/2048,
            if drive.fat {"FAT16"}else{"unformatted/unsupported"},if !boot_selected()&&target().ok()==Some(drive.target) {" [selected]"}else{""});
    }}
    if drives[0].is_none() {println!("No supported IDE drives found.");}
}
pub fn use_drive(number:usize,mount_it:bool)->Result<()> {
    if number==0 {
        if !mount_it {return Err(Error::FirmwareDestination);}
        if !crate::bios_disk::BOOT.init() {return Err(Error::Io);}
        Fat16::new(&crate::bios_disk::BOOT)?;unmount()?;
        unsafe {BOOT_SELECTED=true;}
        mount()?;println!("Drive 0 ready: BIOS boot filesystem (persistent).");return Ok(());
    }
    discover();
    let drive=number.checked_sub(1).and_then(|i|unsafe {(*core::ptr::addr_of!(DRIVES)).get(i).copied().flatten()}).ok_or(Error::NotFound)?;
    // Verify the new disk/filesystem before detaching the current one.
    let candidate=AtaDisk::at(drive.channel,drive.target.drive());
    if !candidate.init() {return Err(Error::Io);}
    if mount_it {Fat16::new(&candidate)?;}
    unmount()?;
    unsafe {SELECTED=Some(drive.target);BOOT_SELECTED=false;}
    DISK.adopt(&candidate);
    if mount_it {mount()?;println!("Drive {} ready. Files are persistent.",number);}
    else {println!("Selected drive {}.",number);}
    Ok(())
}
pub fn startup_mount() {
    if crate::bios::available() {
        unsafe {BOOT_SELECTED=true;}
        if let Err(error)=use_drive(0,true) {println!("Boot filesystem unavailable: {}. Use disks and use NUMBER to choose another drive.",error.message());}
        return;
    }
    if crate::multiboot::disk_target().is_some() {
        match mount() {Ok(_)=>{println!("System drive ready. Type apps or edit FILE.");return;},Err(_)=>{}}
        // Never silently substitute another drive for a remembered installed target.
        println!("System drive unavailable. Use disks, then use NUMBER.");return;
    }
    discover();let drives=unsafe {*core::ptr::addr_of!(DRIVES)};
    let app_drives=drives.iter().flatten().filter(|d|d.apps).count();
    let fat_drives=drives.iter().flatten().filter(|d|d.fat).count();
    let choice=drives.iter().position(|d|d.is_some_and(|d|if app_drives==1 {d.apps}else{app_drives==0&&fat_drives==1&&d.fat}));
    if let Some(index)=choice {if use_drive(index+1,true).is_ok() {return;}}
    println!("Use disks, then use NUMBER to open a drive. No drive was formatted.");
}
fn reset_directory() {
    unsafe {
        VOLUME_EPOCH=VOLUME_EPOCH.wrapping_add(1).max(1);
        *core::ptr::addr_of_mut!(CWD) = crate::fs_path::Path::ROOT;
    }
}
fn absolute(text: &str, buffer: &mut [u8; crate::fs_path::MAX_PATH]) -> Result<usize> {
    let base = unsafe { *core::ptr::addr_of!(CWD) };
    Ok(crate::fs_path::Path::parse(text, &base)?.encode(buffer))
}
// Dispatcher-only snapshots; paths were already checked by cd/normalization.
pub(crate) fn directory()->crate::fs_path::Path {unsafe {*core::ptr::addr_of!(CWD)}}
pub(crate) fn set_directory(path:crate::fs_path::Path) {unsafe {*core::ptr::addr_of_mut!(CWD)=path;}}
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
    if boot_selected() {println!("Selected: BIOS boot drive 0 (status {:02x})",crate::bios_disk::BOOT.status());return;}
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
        BOOT_SELECTED=false;
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
    if boot_selected() {return Err(Error::FirmwareDestination);}
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
    if boot_selected() {return Err(Error::FirmwareDestination);}
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
    let fs=if boot_selected() {
        if !crate::bios_disk::BOOT.init() {return Err(Error::Io);}
        Fat16::new(&crate::bios_disk::BOOT)?
    }else{init_disk()?;Fat16::new(&DISK)?};
    let start = fs.start_sector();
    let has_apps=fs.is_directory("/APPS").is_ok();
    let selected=if boot_selected() {None}else{target().ok()};
    unsafe {
        *core::ptr::addr_of_mut!(MOUNTED) = Some(fs);
        for drive in (&mut *core::ptr::addr_of_mut!(DRIVES)).iter_mut().flatten() {
            if Some(drive.target)==selected {drive.fat=true;drive.apps=has_apps;}
        }
    }
    reset_directory();
    Ok(start)
}
pub fn sync()->Result<()> {if !is_mounted()||if boot_selected(){crate::bios_disk::BOOT.flush()}else{DISK.flush()}{Ok(())}else{Err(Error::Io)}}
pub fn unmount() -> Result<()> {
    let ok = !is_mounted() || if boot_selected() {crate::bios_disk::BOOT.flush()}else{DISK.flush()};
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
pub fn read_program(name: &str, output: &mut [u8]) -> Result<usize> {
    let mut buffer = [0; crate::fs_path::MAX_PATH];
    let length = absolute(name, &mut buffer)?;
    let name = core::str::from_utf8(&buffer[..length]).unwrap();
    let result = match unsafe { &*core::ptr::addr_of!(MOUNTED) } {
        Some(fs) => fs.read_program(name, output),
        None => match output.get_mut(..MAX_FILE_SIZE) {
            Some(bytes) => vfs::read(name, bytes.try_into().unwrap()),
            None => Err(Error::TooLarge),
        },
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
    if path.contains(&base)||crate::multitask::directory_in_use(&path)||crate::pty::directory_in_use(&path) {
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
    if boot_selected() {
        report_target();let mut sector=[0;512];
        if crate::bios_disk::BOOT.read_block(0,&mut sector)&&sector[510..512]==[0x55,0xaa] {println!("Valid boot signature; BIOS boot filesystem is accessible.");}
        else {println!("BIOS boot-drive read failed: status {:02x}",crate::bios_disk::BOOT.status());}
        return;
    }
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

/// Transfers stay on the mounted volume and never replace an existing name.
pub fn transfer(source:&str,destination:&str,operation:u32)->Result<()> {
    if operation>2 {return Err(Error::InvalidName);}
    let base=directory();let from=crate::fs_path::Path::parse(source,&base)?;let to=crate::fs_path::Path::parse(destination,&base)?;
    let mut src=[0;crate::fs_path::MAX_PATH];let n=from.encode(&mut src);let source=core::str::from_utf8(&src[..n]).unwrap();
    let mut dst=[0;crate::fs_path::MAX_PATH];let n=to.encode(&mut dst);let destination=core::str::from_utf8(&dst[..n]).unwrap();
    let Some(fs)=(unsafe {(&*core::ptr::addr_of!(MOUNTED)).as_ref()}) else{return Err(Error::InvalidVolume);};
    if operation!=1 {
        match fs.is_directory(source) {
            Ok(()) if from.contains(&base)||crate::multitask::directory_in_use(&from)||crate::pty::directory_in_use(&from)=>return Err(Error::DirectoryBusy),
            Err(e) if e!=Error::NotDirectory=>return finish(Err(e)),_=>(),
        }
    }
    let result=if operation==0 {fs.rename(source,destination)}else {
        let frames=crate::loader::Frames::new(16).ok_or(Error::OutOfMemory)?;
        let data=unsafe {core::slice::from_raw_parts_mut(frames.base() as *mut u8,65536)};
        fs.transfer_tree(source,destination,operation==2,data)
    };
    finish(result)
}

/// Exact cwd paths take precedence; bare names then search /APPS, .BIN before .ELF.
pub fn read_executable(name:&str,output:&mut[u8])->Result<usize>{let result=read_program(name,output);if !matches!(result,Err(Error::NotFound))||name.contains('/')||name.contains('\\'){return result;}let mut path=[0u8;105];for ext in [".BIN",".ELF",""]{if name.contains('.')&&!ext.is_empty(){continue;}let n=6+name.len()+ext.len();if n>path.len(){return Err(Error::InvalidName);}path[..6].copy_from_slice(b"/APPS/");path[6..6+name.len()].copy_from_slice(name.as_bytes());path[6+name.len()..n].copy_from_slice(ext.as_bytes());let result=read_program(core::str::from_utf8(&path[..n]).unwrap(),output);if !matches!(result,Err(Error::NotFound)){return result;}}Err(Error::NotFound)}

pub fn file_size(name:&str)->Result<usize>{let mut bytes=[0;crate::fs_path::MAX_PATH];let n=absolute(name,&mut bytes)?;let name=core::str::from_utf8(&bytes[..n]).unwrap();let result=match unsafe{&*core::ptr::addr_of!(MOUNTED)}{Some(fs)=>fs.file_size(name),None=>Err(Error::InvalidVolume)};finish(result)}
pub fn save_large(name:&str,data:&[u8])->Result<()>{let mut bytes=[0;crate::fs_path::MAX_PATH];let n=absolute(name,&mut bytes)?;let name=core::str::from_utf8(&bytes[..n]).unwrap();let result=match unsafe{&*core::ptr::addr_of!(MOUNTED)}{Some(fs)=>fs.save_large(name,data),None=>Err(Error::InvalidVolume)};finish(result)}
