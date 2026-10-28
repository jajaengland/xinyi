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
}
static mut SLEEP: Option<Sleep> = None;
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
    if address < 0x1000 || end > 0x40000000 {
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
    if n < 36 || n > 65536 || !ram(address, n) || &head[..4] != signature {
        return None;
    }
    let bytes = core::slice::from_raw_parts(address as *const u8, n);
    checksum(bytes).then_some(bytes)
}
fn discover() -> Option<Sleep> {
    unsafe {
        let ebda = (core::ptr::read_volatile(0x40e as *const u16) as usize) * 16;
        let mut root = None;
        for (start, end) in [(ebda, ebda.saturating_add(1024)), (0xe0000, 0x100000)] {
            if start < 0x80000 || end > 0x100000 {
                continue;
            }
            for address in (start..end.saturating_sub(19)).step_by(16) {
                let b = core::slice::from_raw_parts(address as *const u8, 20);
                if &b[..8] == b"RSD PTR " && checksum(b) {
                    root = u32le(b, 16);
                    break;
                }
            }
            if root.is_some() {
                break;
            }
        }
        let root = table(root?, b"RSDT")?;
        let mut fadt = None;
        for at in (36..root.len()).step_by(4) {
            if let Some(t) = table(u32le(root, at)?, b"FACP") {
                fadt = Some(t);
                break;
            }
        }
        let f = fadt?;
        if f.len() < 116 || u32le(f, 112)? & (1 << 20) != 0 || f[89] < 2 {
            return None;
        }
        let a = u32le(f, 64)?;
        let b = u32le(f, 68)?;
        let smi = u32le(f, 48)?;
        if a == 0 || a > 65534 || b > 65534 || smi > 65535 {
            return None;
        }
        let dsdt = table(u32le(f, 40)?, b"DSDT")?;
        let aml = &dsdt[36..];
        // Unqualified names are accepted only with the known QEMU firmware vendor.
        let virtual_firmware = f.get(10..16) == Some(b"BOCHS ") || f.get(10..16) == Some(b"BXPC  ");
        if !virtual_firmware && aml.windows(4).any(|v| v == b"_PTS" || v == b"_GTS") {
            return None;
        }
        let (ta, tb) = crate::acpi_sleep::sleep_types(aml, virtual_firmware)?;
        Some(Sleep {
            a: a as u16,
            b: b as u16,
            ta,
            tb,
            smi: smi as u16,
            enable: f[52],
        })
    }
}
pub fn init() {
    unsafe {
        SLEEP = discover();
    }
    crate::debug::write_str(if unsafe { SLEEP.is_some() } {
        "Power: static ACPI S5 available\n"
    } else {
        "Power: ACPI soft-off unavailable; halt fallback\n"
    });
}
unsafe fn read(port: u16) -> u16 {
    let v;
    asm!("in ax,dx",in("dx")port,out("ax")v,options(nomem,nostack,preserves_flags));
    v
}
pub fn shutdown() {
    if let Err(e) = crate::diskfs::sync() {
        crate::println!("Shutdown cancelled: {}", e.message());
        return;
    }
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
            if read(s.a) & 1 != 0 {
                asm!("cli", options(nostack));
                let a = (read(s.a) & !0x3c00) | s.ta | 0x2000;
                asm!("out dx,ax",in("dx")s.a,in("ax")a,options(nomem,nostack));
                if s.b != 0 {
                    let b = (read(s.b) & !0x3c00) | s.tb | 0x2000;
                    asm!("out dx,ax",in("dx")s.b,in("ax")b,options(nomem,nostack));
                }
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
