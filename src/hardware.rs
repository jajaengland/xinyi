use crate::hardware_model::{Device, Snapshot};
use crate::pci::read;

static mut INVENTORY: Snapshot = Snapshot::new();

// Run once at boot. Opening the app never probes live devices.
pub fn init() {
    let mut inventory = Snapshot::new();
    'scan: for bus in 0..=u8::MAX {
        for slot in 0..32 {
            if read(bus, slot, 0, 0) as u16 == 0xffff {
                continue;
            }
            let header = read(bus, slot, 0, 0x0c);
            let functions = if header & 0x0080_0000 != 0 { 8 } else { 1 };
            for function in 0..functions {
                let id = read(bus, slot, function, 0);
                if id as u16 == 0xffff {
                    continue;
                }
                let class = read(bus, slot, function, 8);
                let index = inventory.count as usize;
                let header_type = (read(bus, slot, function, 0x0c) >> 16) & 0x7f;
                let subsystem = if header_type == 0 {
                    read(bus, slot, function, 0x2c)
                } else {
                    0
                };
                let Some(entry) = inventory.devices.get_mut(index) else {
                    inventory.truncated = 1;
                    break 'scan;
                };
                *entry = Device {
                    bus,
                    slot,
                    function,
                    class: (class >> 24) as u8,
                    subclass: (class >> 16) as u8,
                    interface: (class >> 8) as u8,
                    vendor: id as u16,
                    device: (id >> 16) as u16,
                    subsystem_vendor: subsystem as u16,
                    subsystem_device: (subsystem >> 16) as u16,
                    ..Device::EMPTY
                };
                if (entry.class, entry.subclass, entry.interface) == (1, 6, 1) {
                    ahci_ports(entry);
                }
                inventory.count += 1;
            }
        }
    }
    unsafe {
        core::ptr::addr_of_mut!(INVENTORY).write(inventory);
    }
}

pub fn sata_devices() -> impl Iterator<Item = (Device, usize)> {
    let inventory = unsafe { &*core::ptr::addr_of!(INVENTORY) };
    inventory.devices[..inventory.count as usize]
        .iter()
        .copied()
        .filter(|device| device.ahci == 2)
        .flat_map(|device| {
            (0..32).filter_map(move |port| (device.ports[port] == 3).then_some((device, port)))
        })
}

// AHCI 1.3.1: read the already-enabled ABAR only. Do not change PCI
// command bits, reset the HBA, wake links, or submit ATA commands.
pub(crate) fn ahci_base(entry: Device) -> Option<u32> {
    let (bus, slot, function) = (entry.bus, entry.slot, entry.function);
    let header_type = (read(bus, slot, function, 0x0c) >> 16) & 0x7f;
    if header_type != 0
        || read(bus, slot, function, 4) & 2 == 0
        || crate::multiboot::mmap_range().is_none()
    {
        return None;
    }
    let bar = read(bus, slot, function, 0x24);
    // ABAR is a 32-bit memory BAR. Bound the supervisor MMIO window and
    // avoid firmware ranges that the physical allocator treats as RAM.
    let base = bar & !15;
    if bar & 7 != 0 || base < 0x8000_0000 || base & 0x3ff != 0 || base.checked_add(0x1100).is_none()
    {
        return None;
    }
    let mut overlaps_ram = false;
    crate::multiboot::for_each_region(|region| {
        overlaps_ram |= region.kind == 1
            && region.base < u64::from(base) + 0x1100
            && region.base.saturating_add(region.length) > u64::from(base);
    });
    if overlaps_ram {
        return None;
    }
    let register = |offset: u32| unsafe { core::ptr::read_volatile((base + offset) as *const u32) };
    if register(4) & (1 << 31) == 0 || register(0x10) >> 16 != 1 {
        return None;
    }
    Some(base)
}

fn ahci_ports(entry: &mut Device) {
    entry.ahci = 1;
    let Some(base) = ahci_base(*entry) else {
        return;
    };
    let register = |offset: u32| unsafe { core::ptr::read_volatile((base + offset) as *const u32) };
    let implemented = register(0x0c);
    for (port, state) in entry.ports.iter_mut().enumerate() {
        if implemented & (1 << port) == 0 {
            continue;
        }
        let start = 0x100 + port as u32 * 0x80;
        *state = match register(start + 0x28) & 15 {
            0 => 1, // no device
            1 => 2, // detected without an established link
            3 => match register(start + 0x24) {
                0x0000_0101 => 3, // SATA disk
                0xeb14_0101 => 4, // ATAPI
                _ => 5,           // link is up, device type unknown
            },
            4 => 6, // offline
            _ => 5,
        };
    }
    entry.ahci = 2;
}

pub fn query(cs: u32, op: u32, pointer: u32, size: u32) -> u32 {
    if crate::usermode::active_image(cs).is_none() {
        return u32::MAX;
    }
    if matches!(op, 1 | 2) {
        if size != 0 {
            return (-22i32) as u32;
        }
        if !crate::usermode::profile().allows(107, op) {
            return (-13i32) as u32;
        }
        if pointer != crate::diskfs::volume_epoch() {
            return (-116i32) as u32;
        }
        unsafe {
            core::arch::asm!("sti", options(nostack));
        }
        let result = save_report(op == 2);
        unsafe {
            core::arch::asm!("cli", options(nostack));
        }
        return match result {
            Ok(index) => index,
            Err(error) => crate::files_api::error(error) as u32,
        };
    }
    if op != 0 || size as usize != core::mem::size_of::<Snapshot>() {
        return (-22i32) as u32;
    }
    if !crate::user_memory::writable(pointer, size) {
        return (-14i32) as u32;
    }
    unsafe {
        core::ptr::write_unaligned(pointer as *mut Snapshot, snapshot());
    }
    0
}

fn snapshot() -> Snapshot {
    let mut snapshot = unsafe { core::ptr::addr_of!(INVENTORY).read() };
    snapshot.audio = crate::audio::active_pci().unwrap_or(crate::hardware_model::NO_AUDIO);
    snapshot.display = crate::graphics::backend();
    if snapshot.display != 0 {
        snapshot.width = crate::display_layout::physical_width() as u16;
        snapshot.height = crate::display_layout::physical_height() as u16;
    }
    snapshot.storage = if !crate::diskfs::is_mounted() {
        0
    } else if crate::diskfs::boot_selected() {
        1
    } else if crate::diskfs::sata_selected() {
        3
    } else {
        2
    };
    snapshot.network = u8::from(crate::network::enabled());
    snapshot.mouse = u8::from(crate::mouse::enabled());
    snapshot
}

const REPORT_LIMIT: usize = 12288;
struct Report {
    bytes: [u8; REPORT_LIMIT],
    len: usize,
    safe_to_save: bool,
}

impl core::fmt::Write for Report {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        let end = self.len + text.len();
        self.bytes
            .get_mut(self.len..end)
            .ok_or(core::fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

fn report_text(report: &mut Report, extended: bool) -> core::fmt::Result {
    use core::{arch::x86::__cpuid, fmt::Write};
    let s = snapshot();
    writeln!(report, "Xinyi hardware report v1")?;
    let cpu = __cpuid(0);
    let mut vendor = [0; 12];
    for (chunk, value) in vendor.chunks_exact_mut(4).zip([cpu.ebx, cpu.edx, cpu.ecx]) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    let signature = if cpu.eax >= 1 { __cpuid(1).eax } else { 0 };
    writeln!(
        report,
        "CPU: {} (signature {:08x})",
        core::str::from_utf8(&vendor).unwrap_or("Unknown"),
        signature
    )?;
    let last = __cpuid(0x8000_0000).eax;
    if last >= 0x8000_0004 {
        let mut brand = [0; 48];
        for (index, chunk) in brand.chunks_exact_mut(16).enumerate() {
            let leaf = __cpuid(0x8000_0002 + index as u32);
            for (part, value) in chunk
                .chunks_exact_mut(4)
                .zip([leaf.eax, leaf.ebx, leaf.ecx, leaf.edx])
            {
                part.copy_from_slice(&value.to_le_bytes());
            }
        }
        let end = brand.iter().position(|&b| b == 0).unwrap_or(brand.len());
        if let Ok(brand) = core::str::from_utf8(&brand[..end]) {
            writeln!(report, "CPU model: {}", brand.trim())?;
        }
    }
    let mut usable = 0u64;
    crate::multiboot::for_each_region(|region| {
        if region.kind == 1 {
            usable = usable.saturating_add(region.length);
        }
    });
    writeln!(
        report,
        "Firmware-reported usable RAM: {} MiB",
        usable / (1024 * 1024)
    )?;
    writeln!(
        report,
        "Boot: {}",
        if crate::bios::available() {
            "Legacy BIOS"
        } else {
            "No BIOS disk handoff"
        }
    )?;
    writeln!(
        report,
        "Display: {} {}x{} at {}% UI scale",
        match s.display {
            1 => "Bochs",
            2 => "BIOS VESA",
            _ => "inactive",
        },
        s.width,
        s.height,
        crate::display_layout::scale()
    )?;
    writeln!(
        report,
        "Storage: {}",
        match s.storage {
            1 => "BIOS boot drive mounted",
            2 => "Native IDE volume mounted",
            3 => "Native AHCI volume mounted",
            _ => "No mounted volume",
        }
    )?;
    writeln!(
        report,
        "PS/2 mouse: {}",
        if s.mouse != 0 {
            "active"
        } else {
            "unavailable"
        }
    )?;
    writeln!(
        report,
        "Networking: {}",
        if s.network != 0 {
            "RTL8139 active"
        } else {
            "inactive"
        }
    )?;
    writeln!(
        report,
        "Sampled audio: {}",
        if s.audio == crate::hardware_model::NO_AUDIO {
            "inactive"
        } else {
            "active (see PCI list)"
        }
    )?;
    for (address, id) in crate::audio::codec_ids().into_iter().enumerate() {
        if id != 0 {
            writeln!(
                report,
                "HD Audio codec {}: {:04x}:{:04x}",
                address,
                id >> 16,
                id as u16
            )?;
        }
    }
    writeln!(
        report,
        "\nPCI devices ({}{}):",
        s.count,
        if s.truncated != 0 { ", truncated" } else { "" }
    )?;
    for d in &s.devices[..s.count as usize] {
        writeln!(
            report,
            "{:02x}:{:02x}.{} {:04x}:{:04x} class {:02x}/{:02x}/{:02x} subsystem {:04x}:{:04x}{}",
            d.bus,
            d.slot,
            d.function,
            d.vendor,
            d.device,
            d.class,
            d.subclass,
            d.interface,
            d.subsystem_vendor,
            d.subsystem_device,
            if d.address() == s.audio {
                " [active audio]"
            } else {
                ""
            }
        )?;
    }
    if let Some([count, compose, present, compose_peak, present_peak]) =
        crate::graphics_timing::summary()
    {
        writeln!(report, "\nDesktop timing ({} updates, TSC ticks):", count)?;
        writeln!(
            report,
            "Composition: mean {}, peak {}",
            compose, compose_peak
        )?;
        writeln!(
            report,
            "Framebuffer and cursor: mean {}, peak {}",
            present, present_peak
        )?;
    } else {
        writeln!(report, "\nDesktop timing unavailable")?;
    }
    writeln!(report, "Power-off: {}", crate::power::status())?;
    let (ssdt, markers, methods) = crate::power::s5_scan();
    writeln!(
        report,
        "ACPI sleep scan: {} SSDTs, {} S5 name markers, shutdown method names {}",
        ssdt,
        markers,
        if methods { "present" } else { "not found" }
    )?;
    let controllers = s.devices[..s.count as usize].iter().filter(|d| d.ahci != 0);
    if controllers.clone().next().is_some() {
        writeln!(report, "\nAHCI port status (positions 0-31):")?;
        writeln!(report, "- absent port, . empty, d detected without link, S SATA, A ATAPI, x offline, ? unknown")?;
        for d in controllers {
            let map = crate::hardware_model::port_map(&d.ports);
            let status = if d.ahci == 2 {
                core::str::from_utf8(&map).unwrap()
            } else {
                "unavailable (firmware state)"
            };
            writeln!(
                report,
                "{:02x}:{:02x}.{} {}",
                d.bus, d.slot, d.function, status
            )?;
        }
    }
    if extended {
        writeln!(
            report,
            "\nSATA diagnostic (IDENTIFY, boot sector, and read comparison):"
        )?;
        let mut targets = s.devices[..s.count as usize]
            .iter()
            .filter(|d| d.ahci == 2)
            .flat_map(|d| {
                d.ports
                    .iter()
                    .enumerate()
                    .filter_map(move |(i, state)| (*state == 3).then_some((*d, i)))
            })
            .peekable();
        if targets.peek().is_none() {
            writeln!(report, "No detected SATA disk ports.")?;
        }
        for (d, port) in targets.by_ref().take(4) {
            write!(
                report,
                "{:02x}:{:02x}.{} port {}: ",
                d.bus, d.slot, d.function, port
            )?;
            match crate::ahci_check::inspect(d, port) {
                Ok(drive) => {
                    let model = core::str::from_utf8(&drive.model)
                        .unwrap_or("Unknown")
                        .trim();
                    writeln!(
                        report,
                        "{}; {} sectors x {} bytes",
                        model, drive.sectors, drive.sector_bytes
                    )?;
                    match drive.read {
                        Ok(signature) => writeln!(
                            report,
                            "  LBA 0 read OK; 55AA signature {}",
                            if signature { "present" } else { "absent" }
                        )?,
                        Err(reason) => writeln!(report, "  LBA 0 read unavailable: {}", reason)?,
                    }
                    match drive.range_check {
                        Some(Ok(())) => writeln!(report, "  Final four sectors: bulk and individual reads match")?,
                        Some(Err(reason)) => writeln!(report, "  Final four-sector check failed: {}", reason)?,
                        None => writeln!(report, "  Four-sector check skipped (requires sectors of at most 1024 bytes and a successful boot-sector read)")?,
                    }
                }
                Err(reason) => {
                    writeln!(report, "unavailable: {}", reason)?;
                    if reason.starts_with("port could not be restored") {
                        // A BIOS-backed destination could use this same controller.
                        // Keep disk I/O out of the failed diagnostic.
                        report.safe_to_save = false;
                        crate::debug::write_fmt(format_args!("AHCI diagnostic: {}\n", reason));
                        break;
                    }
                }
            }
        }
        if targets.next().is_some() {
            writeln!(report, "Additional SATA ports were not tested.")?;
        }
    }
    Ok(())
}

fn save_report(extended: bool) -> crate::fat16::Result<u32> {
    use crate::fat16::Error;
    if !crate::diskfs::is_mounted() {
        return Err(Error::InvalidVolume);
    }
    let mut report = Report {
        bytes: [0; REPORT_LIMIT],
        len: 0,
        safe_to_save: true,
    };
    report_text(&mut report, extended).map_err(|_| Error::ProgramTooLarge)?;
    if !report.safe_to_save {
        return Err(Error::Io);
    }
    for index in 0..=99 {
        let path = crate::hardware_model::report_path(index);
        let name = core::str::from_utf8(&path).unwrap();
        match crate::diskfs::file_size(name) {
            Ok(_) | Err(Error::NotFile) => continue,
            Err(Error::NotFound) => {
                crate::diskfs::save_large(name, &report.bytes[..report.len])?;
                crate::diskfs::sync()?;
                return Ok(index);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Error::Exists)
}
