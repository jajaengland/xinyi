//! Bounded legacy ACPI soft-off; unsupported firmware falls back to a halted CPU.
use core::arch::asm;
#[derive(Clone, Copy)]
struct Sleep {
    a: u16,
    b: u16,
    ta: u16,
    tb: u16,
    smi: u16,
    enable: u8,
    hp_ec: bool,
}
static mut SLEEP: Option<Sleep> = None;
static mut S5_SCAN: (usize, usize, bool) = (0, 0, false);
pub fn s5_scan() -> (usize, usize, bool) {
    unsafe { core::ptr::addr_of!(S5_SCAN).read() }
}
static mut STATUS: &str = "Not initialized";

pub fn status() -> &'static str {
    unsafe { core::ptr::addr_of!(STATUS).read() }
}

fn u64le(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?))
}
fn u32le(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}
fn checksum(b: &[u8]) -> bool {
    b.iter().fold(0u8, |a, b| a.wrapping_add(*b)) == 0
}
fn ram(address: u32, len: usize) -> bool {
    let Some(end) = (address as u64).checked_add(len as u64) else {
        return false;
    };
    if address < 0x1000 || end > (1u64 << 32) {
        return false;
    }
    let mut valid = false;
    crate::multiboot::for_each_region(|r| {
        if matches!(r.kind, 1..=4)
            && address as u64 >= r.base
            && r.base.checked_add(r.length).is_some_and(|n| end <= n)
        {
            valid = true;
        }
    });
    valid
}
unsafe fn table(address: u32, signature: &[u8]) -> Option<&'static [u8]> {
    if !ram(address, 36) {
        return None;
    }
    let head = core::slice::from_raw_parts(address as *const u8, 36);
    let n = u32le(head, 4)? as usize;
    if n < 36 || n > 512 * 1024 || !ram(address, n) || &head[..4] != signature {
        return None;
    }
    let bytes = core::slice::from_raw_parts(address as *const u8, n);
    checksum(bytes).then_some(bytes)
}
fn root_pointer() -> Option<&'static [u8]> {
    unsafe {
        let ebda = usize::from(core::ptr::read_volatile(0x40e as *const u16)) * 16;
        for (start, end) in [(ebda, ebda.saturating_add(1024)), (0xe0000, 0x100000)] {
            if start < 0x80000 || end > 0x100000 {
                continue;
            }
            for address in (start..end.saturating_sub(19)).step_by(16) {
                let legacy = core::slice::from_raw_parts(address as *const u8, 20);
                if &legacy[..8] != b"RSD PTR " || !checksum(legacy) {
                    continue;
                }
                if legacy[15] >= 2 && address + 36 <= end {
                    let header = core::slice::from_raw_parts(address as *const u8, 36);
                    let size = u32le(header, 20).unwrap_or(0) as usize;
                    if (36..=4096).contains(&size) && address + size <= end {
                        let extended = core::slice::from_raw_parts(address as *const u8, size);
                        if checksum(extended) {
                            return Some(extended);
                        }
                    }
                }
                return Some(legacy);
            }
        }
    }
    None
}

fn root_entries(root: u64, stride: usize) -> Option<&'static [u8]> {
    let address = u32::try_from(root).ok()?;
    let root = unsafe { table(address, if stride == 8 { b"XSDT" } else { b"RSDT" })? };
    let entries = &root[36..];
    (entries.len() % stride == 0).then_some(entries)
}

fn tables(signature: &'static [u8]) -> impl Iterator<Item = &'static [u8]> {
    let pointer = root_pointer().unwrap_or(&[]);
    let extended = u64le(pointer, 24).and_then(|root| root_entries(root, 8));
    let legacy = || u32le(pointer, 16).and_then(|root| root_entries(u64::from(root), 4));
    let (entries, stride) = match extended {
        Some(entries) => (entries, 8),
        None => (legacy().unwrap_or(&[]), 4),
    };
    entries.chunks_exact(stride).filter_map(move |entry| {
        let address = if stride == 8 {
            u32::try_from(u64le(entry, 0)?).ok()?
        } else {
            u32le(entry, 0)?
        };
        unsafe { table(address, signature) }
    })
}

pub(crate) fn fadt() -> Option<&'static [u8]> {
    tables(b"FACP").next()
}

// Pavilion x360 board 83C4. Match the inspected firmware image as well
// as the board, since the EC register layout is firmware-specific.
fn hp_pavilion_s5(dsdt: &[u8], pm1a: u16, pm1b: u16) -> bool {
    pm1a == 0x1804
        && pm1b == 0
        && dsdt.len() == 200690
        && dsdt.get(10..24) == Some(b"HPQOEMSLIC-MPC")
        && dsdt.get(0x30fd4..0x30fe0) == Some(b"\x08_S5_\x12\x07\x04\x0a\x07\x00\x00")
        && crate::pci::read(0, 0x1f, 3, 0) == 0x9d71_8086
        && crate::pci::read(0, 0x1f, 3, 0x2c) == 0x83c4_103c
        && crate::bootdisk::checksum(dsdt) == 0x22d9_6d71
}

fn discover() -> Result<Sleep, &'static str> {
    let f = fadt().ok_or("ACPI FADT unavailable")?;
    let flags = u32le(f, 112).ok_or("ACPI FADT is too short")?;
    if flags & (1 << 20) != 0 {
        return Err("Hardware-reduced ACPI is unsupported");
    }
    let (a, b) = crate::acpi_sleep::pm1_ports(f).ok_or("Unsupported ACPI PM1 control registers")?;
    let smi = u32le(f, 48)
        .and_then(|n| u16::try_from(n).ok())
        .unwrap_or(0);
    let dsdt = u64le(f, 140)
        .and_then(|n| u32::try_from(n).ok())
        .and_then(|n| unsafe { table(n, b"DSDT") })
        .or_else(|| unsafe { table(u32le(f, 40)?, b"DSDT") })
        .ok_or("ACPI DSDT unavailable")?;
    let aml = &dsdt[36..];
    // Keep the existing known-firmware exception. Evaluating real firmware
    // methods safely requires an AML interpreter.
    let virtual_firmware = matches!(f.get(10..16), Some(b"BOCHS " | b"BXPC  "));
    let hp_ec = hp_pavilion_s5(dsdt, a, b);
    let sleep = |aml| crate::acpi_sleep::static_sleep_types(aml, virtual_firmware, b != 0);
    let mut types = if hp_ec {
        crate::acpi_sleep::static_sleep_types(aml, true, false)
    } else {
        sleep(aml)
    };
    let contains_methods = |aml: &[u8]| aml.windows(4).any(|v| v == b"_PTS" || v == b"_GTS");
    let mut methods = contains_methods(aml);
    let mut markers = aml.windows(4).filter(|&v| v == b"_S5_").count();
    let mut ssdt_count = 0;
    for ssdt in tables(b"SSDT").take(64) {
        ssdt_count += 1;
        let aml = &ssdt[36..];
        methods |= contains_methods(aml);
        markers += aml.windows(4).filter(|&v| v == b"_S5_").count();
        if let Some(found) = sleep(aml) {
            if types.is_some_and(|old| old != found) {
                return Err("Conflicting static ACPI S5 packages");
            }
            types = Some(found);
        }
    }
    unsafe {
        S5_SCAN = (ssdt_count, markers, methods);
    }
    let (ta, tb) = types.ok_or("No supported static ACPI S5 package")?;
    if !virtual_firmware && !hp_ec && methods {
        return Err("Firmware exposes ACPI shutdown methods that Xinyi cannot run");
    }
    Ok(Sleep {
        a,
        b,
        ta,
        tb,
        smi,
        enable: f[52],
        hp_ec,
    })
}

pub fn init() {
    let result = discover();
    unsafe {
        SLEEP = result.ok();
        STATUS = match result {
            Ok(s) if s.hp_ec => "HP Pavilion ACPI S5 with embedded controller preparation",
            Ok(_) => "Static ACPI S5 available",
            Err(reason) => reason,
        };
    }
    if result.is_ok() {
        crate::debug::write_str("Power: static ACPI S5 available\n");
    } else {
        crate::debug::write_fmt(format_args!("Power: {}; halt fallback\n", status()));
    }
}
unsafe fn read(port: u16) -> u16 {
    let v;
    asm!("in ax,dx",in("dx")port,out("ax")v,options(nomem,nostack,preserves_flags));
    v
}
// Execute the ECPS(5) portion of the matched DSDT's _PTS(5).
// EC command/data handshakes are bounded and preserve adjacent field bits.
unsafe fn hp_prepare_s5() -> bool {
    const STATUS: u16 = 0x66;
    const DATA: u16 = 0x62;
    fn wait(mask: u8, expected: u8) -> bool {
        let start = crate::interrupts::timer_ticks();
        for _ in 0..100_000 {
            if unsafe { crate::bios::input(STATUS) } & mask == expected {
                return true;
            }
            if crate::interrupts::timer_ticks().wrapping_sub(start) >= 10 {
                break;
            }
            core::hint::spin_loop();
        }
        false
    }
    unsafe fn send(port: u16, byte: u8) -> bool {
        if !wait(2, 0) {
            return false;
        }
        crate::bios::output(port, byte);
        true
    }
    // Refuse a pending event or an existing response owned by firmware.
    if crate::bios::input(STATUS) & 0x31 != 0 {
        return false;
    }
    for (address, mask, bits) in [(0x76, 0x0f, 5), (0x71, 0x08, 0x08)] {
        if !send(STATUS, 0x80) || !send(DATA, address) || !wait(1, 1) {
            return false;
        }
        let old = crate::bios::input(DATA);
        if !send(STATUS, 0x81)
            || !send(DATA, address)
            || !send(DATA, (old & !mask) | bits)
            || !wait(2, 0)
        {
            return false;
        }
    }
    true
}

pub fn shutdown() {
    if let Err(e) = crate::diskfs::sync() {
        crate::println!("Shutdown cancelled: {}", e.message());
        crate::system_sounds::play(crate::system_sounds::Event::Error);
        return;
    }
    crate::audio::farewell();
    crate::network::shutdown();
    crate::println!("Shutting down...");
    crate::debug::write_str("Power: disks synchronized\n");
    if let Some(s) = unsafe { SLEEP } {
        unsafe {
            if read(s.a) & 1 == 0 && s.smi != 0 && s.enable != 0 {
                asm!("out dx,al",in("dx")s.smi,in("al")s.enable,options(nomem,nostack));
                let start = crate::interrupts::timer_ticks();
                for _ in 0..2_000_000 {
                    if read(s.a) & 1 != 0
                        || crate::interrupts::timer_ticks().wrapping_sub(start) >= 100
                    {
                        break;
                    }
                    core::hint::spin_loop();
                }
            }
            let acpi_enabled = read(s.a) & 1 != 0;
            if acpi_enabled && s.hp_ec && !hp_prepare_s5() {
                crate::debug::write_str("Power: HP embedded controller preparation failed\n");
            } else if acpi_enabled {
                asm!("cli", options(nostack));
                let a = (read(s.a) & !0x3c00) | s.ta | 0x2000;

                if s.b != 0 {
                    let b = (read(s.b) & !0x3c00) | s.tb | 0x2000;
                    asm!("out dx,ax",in("dx")s.a,in("ax")(a & !0x2000),options(nomem,nostack));
                    asm!("out dx,ax",in("dx")s.b,in("ax")(b & !0x2000),options(nomem,nostack));
                    asm!("out dx,ax",in("dx")s.b,in("ax")b,options(nomem,nostack));
                }
                asm!("out dx,ax",in("dx")s.a,in("ax")a,options(nomem,nostack));
            }
        }
    }
    crate::pty::set_current(None);
    crate::graphics::restore();
    crate::println!("Power-off unavailable. It is now safe to turn off the computer.");
    crate::debug::write_str("Power: halted; safe to power off\n");
    loop {
        unsafe {
            asm!("cli; hlt", options(nomem, nostack));
        }
    }
}

/// Synchronize before the existing bounded keyboard-controller reset sequence.
pub fn restart() -> bool {
    if let Err(e) = crate::diskfs::sync() {
        crate::println!("Restart cancelled: {}", e.message());
        return false;
    }
    crate::audio::farewell();
    crate::network::shutdown();
    crate::debug::write_str("Power: restart; disks synchronized\n");
    unsafe {
        for _ in 0..100_000 {
            if crate::bios::input(0x64) & 2 == 0 {
                crate::bios::output(0x64, 0xfe);
                for _ in 0..100_000 {
                    core::hint::spin_loop();
                }
                return false;
            }
        }
    }
    false
}
