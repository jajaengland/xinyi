//! AHCI SATA block I/O and the read-only hardware report diagnostic.
use crate::{hardware_model::Device, interrupts::timer_ticks, loader::Frames, pci};
use core::{
    arch::asm,
    ptr,
    sync::atomic::{compiler_fence, AtomicBool, Ordering},
};

type Result<T> = core::result::Result<T, &'static str>;
static FAULTED: AtomicBool = AtomicBool::new(false);

const ST: u32 = 1;
const FRE: u32 = 1 << 4;
const FR: u32 = 1 << 14;
const CR: u32 = 1 << 15;

#[derive(Clone, Copy)]
struct Registers(u32);
impl Registers {
    fn read(self, reg: u32) -> u32 {
        unsafe { ptr::read_volatile((self.0 + reg) as *const u32) }
    }
    fn write(self, reg: u32, value: u32) {
        unsafe { ptr::write_volatile((self.0 + reg) as *mut u32, value) }
    }
}

pub struct Drive {
    pub model: [u8; 40],
    pub sectors: u64,
    pub sector_bytes: u32,
    pub read: Result<bool>,
    pub range_check: Option<Result<()>>,
}

fn wait(ticks: u32, mut ready: impl FnMut() -> bool) -> bool {
    for _ in 0..64 {
        if ready() {
            return true;
        }
        core::hint::spin_loop();
    }
    let start = timer_ticks();
    loop {
        if ready() {
            return true;
        }
        if timer_ticks().wrapping_sub(start) >= ticks {
            return false;
        }
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

fn stop(port: Registers, command: u32) -> bool {
    port.write(0x18, command & !ST);
    if !wait(50, || port.read(0x18) & CR == 0) {
        return false;
    }
    port.write(0x18, command & !(ST | FRE));
    wait(50, || port.read(0x18) & FR == 0)
}

#[derive(Clone, Copy)]
struct Range {
    lba: u64,
    count: u8,
    extended: bool,
    write: bool,
}

fn issue(port: Registers, base: u32, range: Option<Range>, opcode: u8, bytes: u32) -> Result<()> {
    if bytes > 4096 || (bytes == 0 && range.is_some()) {
        return Err("invalid DMA transfer length");
    }
    if port.read(0x38) != 0 || port.read(0x34) != 0 || port.read(0x20) & 0x88 != 0 {
        return Err("port busy before command");
    }
    let header = base as *mut u32;
    let table = (base + 0x500) as *mut u8;
    unsafe {
        ptr::write_bytes(table, 0, 0x90);
        let write = range.is_some_and(|r| r.write);
        if !write {
            ptr::write_bytes((base + 4096) as *mut u8, 0, bytes as usize);
        }
        header.write_volatile(
            5 | if bytes != 0 { 1 << 16 } else { 0 } | if write { 1 << 6 } else { 0 },
        ); // five FIS dwords, one PRD, device-to-host
        header.add(1).write_volatile(0);
        header.add(2).write_volatile(base + 0x500);
        table.write_volatile(0x27);
        table.add(1).write_volatile(0x80);
        if let Some(range) = range {
            table
                .add(2)
                .write_volatile(match (range.extended, range.write) {
                    (true, false) => 0x25,
                    (false, false) => 0xc8,
                    (true, true) => 0x35,
                    (false, true) => 0xca,
                });
            let lba = range.lba.to_le_bytes();
            for (offset, value) in lba[..3].iter().enumerate() {
                table.add(4 + offset).write_volatile(*value);
            }
            table
                .add(7)
                .write_volatile(0x40 | if range.extended { 0 } else { lba[3] & 15 });
            if range.extended {
                for (offset, value) in lba[3..6].iter().enumerate() {
                    table.add(8 + offset).write_volatile(*value);
                }
            }
            table.add(12).write_volatile(range.count);
        } else {
            table.add(2).write_volatile(opcode);
        }
        if bytes != 0 {
            table.add(0x80).cast::<u32>().write_volatile(base + 4096);
            table.add(0x8c).cast::<u32>().write_volatile(bytes - 1);
        }
    }
    compiler_fence(Ordering::Release);
    port.write(0x38, 1);
    if !wait(100, || {
        port.read(0x38) & 1 == 0 || port.read(0x10) & 0x7800_0000 != 0
    }) {
        return Err("command timed out");
    }
    if port.read(0x38) != 0 || port.read(0x10) & 0x7800_0000 != 0 || port.read(0x20) & 0x89 != 0 {
        return Err("controller reported a command error");
    }
    compiler_fence(Ordering::Acquire);
    if unsafe { header.add(1).read_volatile() } != bytes {
        return Err("incomplete transfer");
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub struct Geometry {
    pub model: [u8; 40],
    pub sectors: u64,
    pub sector_bytes: u32,
    extended: bool,
    dma: bool,
    flush: u8,
}

fn geometry(port: Registers, base: u32) -> Result<Geometry> {
    issue(port, base, None, 0xec, 512)?;
    let data = (base + 4096) as *const u16;
    let word = |index: usize| unsafe { data.add(index).read_volatile() };
    let mut model = [0; 40];
    for (index, chunk) in model.chunks_exact_mut(2).enumerate() {
        let value = word(27 + index).to_be_bytes();
        for (byte, input) in chunk.iter_mut().zip(value) {
            *byte = if (32..=126).contains(&input) {
                input
            } else {
                b' '
            };
        }
    }
    let ext = word(83) & 0xc400 == 0x4400;
    let sectors = if ext {
        (0..4).fold(0u64, |total, i| {
            total | (u64::from(word(100 + i)) << (i * 16))
        })
    } else {
        u64::from(word(60)) | (u64::from(word(61)) << 16)
    };
    let sector_bytes = if word(106) & 0xd000 == 0x5000 {
        (u32::from(word(117)) | (u32::from(word(118)) << 16)).saturating_mul(2)
    } else {
        512
    };
    let flush = if word(83) & 0xe000 == 0x6000 {
        0xea
    } else if word(83) & 0xd000 == 0x5000 {
        0xe7
    } else {
        0
    };
    Ok(Geometry {
        model,
        sectors,
        sector_bytes,
        extended: ext,
        dma: word(49) & 0x300 == 0x300,
        flush,
    })
}

fn identify(port: Registers, base: u32) -> Result<Drive> {
    let g = geometry(port, base)?;
    let Geometry {
        model,
        sectors,
        sector_bytes,
        extended: ext,
        dma: supported,
        ..
    } = g;
    let data = (base + 4096) as *const u16;
    let read = if supported {
        read_sectors(port, base, ext, sectors, sector_bytes, 0, 1).map(|()| unsafe {
            // Record only whether a boot signature is present, never sector data.
            data.add(255).read_volatile() == 0xaa55
        })
    } else {
        Err("drive does not support LBA and DMA")
    };
    let range_check = if read.is_ok() && sectors >= 5 && sector_bytes <= 1024 {
        Some(compare_reads(port, base, ext, sectors, sector_bytes))
    } else {
        None
    };
    Ok(Drive {
        model,
        sectors,
        sector_bytes,
        read,
        range_check,
    })
}

fn read_sectors(
    port: Registers,
    base: u32,
    extended: bool,
    capacity: u64,
    sector_bytes: u32,
    lba: u64,
    count: u8,
) -> Result<()> {
    let limit = if extended { 1u64 << 48 } else { 1u64 << 28 };
    let end = lba
        .checked_add(u64::from(count))
        .ok_or("invalid LBA range")?;
    let bytes = sector_bytes
        .checked_mul(u32::from(count))
        .ok_or("invalid DMA length")?;
    if count == 0 || end > capacity.min(limit) || !(512..=4096).contains(&sector_bytes) {
        return Err("unsupported drive geometry or LBA range");
    }
    issue(
        port,
        base,
        Some(Range {
            lba,
            count,
            extended,
            write: false,
        }),
        0,
        bytes,
    )
}

fn compare_reads(
    port: Registers,
    base: u32,
    extended: bool,
    capacity: u64,
    sector_bytes: u32,
) -> Result<()> {
    let buffer = (base + 4096) as *const u8;
    let size = sector_bytes as usize;
    let mut reference = [0u8; 4096];
    // Near the end of the disk, exercising upper LBA bytes on large drives.
    let start = (capacity.min(if extended { 1 << 48 } else { 1 << 28 })) - 4;
    for i in 0..4 {
        read_sectors(
            port,
            base,
            extended,
            capacity,
            sector_bytes,
            start + i as u64,
            1,
        )?;
        unsafe {
            ptr::copy_nonoverlapping(buffer, reference.as_mut_ptr().add(i * size), size);
        }
    }
    read_sectors(port, base, extended, capacity, sector_bytes, start, 4)?;
    let actual = unsafe { core::slice::from_raw_parts(buffer, size * 4) };
    if actual != &reference[..size * 4] {
        return Err("four-sector DMA read disagreed with individual reads");
    }
    Ok(())
}

fn with_port<T>(
    device: Device,
    index: usize,
    run: impl FnOnce(Registers, u32) -> Result<T>,
) -> Result<T> {
    if FAULTED.load(Ordering::Relaxed) {
        return Err("port could not be restored; reboot before retrying");
    }
    let flags: u32;
    unsafe {
        asm!("pushfd", "pop {0:e}", "sti", out(reg) flags);
    }
    let result = port_session(device, index, run);
    if flags & 0x200 == 0 {
        unsafe {
            asm!("cli", options(nostack));
        }
    }
    result
}

fn port_session<T>(
    device: Device,
    index: usize,
    run: impl FnOnce(Registers, u32) -> Result<T>,
) -> Result<T> {
    let base = crate::hardware::ahci_base(device).ok_or("AHCI register window unavailable")?;
    let (bus, slot, function) = (device.bus, device.slot, device.function);
    if pci::read(bus, slot, function, 4) & 4 == 0 {
        return Err("firmware left bus mastering disabled");
    }
    let hba = Registers(base);
    if hba.read(0x24) & 1 != 0 && hba.read(0x28) & 0x11 != 0 {
        return Err("firmware still owns the controller");
    }
    if index >= 32 || hba.read(0x0c) & (1 << index) == 0 {
        return Err("port unavailable");
    }
    let port = Registers(base + 0x100 + index as u32 * 0x80);
    if port.read(0x28) & 0x0f0f != 0x0103 || port.read(0x24) != 0x0000_0101 {
        return Err("SATA link is not active");
    }
    if port.read(0x38) != 0
        || port.read(0x34) != 0
        || port.read(0x20) & 0x88 != 0
        || port.read(0x10) & 0x7800_0000 != 0
        || port.read(0x40) & 1 != 0
    {
        return Err("port busy or in error");
    }

    let dma = Frames::new(2).ok_or("no DMA memory")?;
    let command = port.read(0x18) & 0x0fff_ffff;
    let enabled = port.read(0x14);
    let original = [port.read(0), port.read(4), port.read(8), port.read(12)];
    let previous_is = port.read(0x10);
    let previous_serr = port.read(0x30);
    if !stop(port, command) {
        port.write(0x18, command);
        return Err("firmware port did not stop");
    }
    port.write(0x14, 0);
    let dma_base = dma.base();
    port.write(0, dma_base);
    port.write(4, 0);
    port.write(8, dma_base + 0x400);
    port.write(12, 0);
    let idle = command & !(ST | FRE);
    port.write(0x18, idle | FRE);
    port.write(0x18, idle | FRE | ST);
    let result = run(port, dma_base);
    if !stop(port, idle | FRE) || port.read(0x38) != 0 || port.read(0x34) != 0 {
        // Retain DMA backing if the controller cannot be proven idle.
        FAULTED.store(true, Ordering::Relaxed);
        core::mem::forget(dma);
        return Err("port could not be restored; reboot before retrying");
    }
    for (i, value) in original.into_iter().enumerate() {
        port.write(i as u32 * 4, value);
    }
    port.write(0x10, port.read(0x10) & !previous_is);
    port.write(0x30, port.read(0x30) & !previous_serr);
    port.write(0x14, enabled);
    port.write(0x18, command & !ST);
    port.write(0x18, command);
    result
}

pub fn inspect(device: Device, index: usize) -> Result<Drive> {
    with_port(device, index, identify)
}

#[derive(Clone, Copy)]
pub struct Disk {
    device: Device,
    index: usize,
    pub geometry: Geometry,
}

impl Disk {
    pub fn probe(device: Device, index: usize) -> Option<Self> {
        let geometry = with_port(device, index, geometry).ok()?;
        let limit = if geometry.extended {
            u64::from(u32::MAX)
        } else {
            1 << 28
        };
        if geometry.sector_bytes != 512
            || !geometry.dma
            || geometry.flush == 0
            || geometry.sectors == 0
            || geometry.sectors > limit
        {
            return None;
        }
        Some(Self {
            device,
            index,
            geometry,
        })
    }

    fn transfer(&self, lba: u32, input: Option<&[u8]>, output: Option<&mut [u8]>) -> bool {
        if u64::from(lba) >= self.geometry.sectors
            || input.is_some_and(|b| b.len() < 512)
            || output.as_ref().is_some_and(|b| b.len() < 512)
        {
            return false;
        }
        let result = with_port(self.device, self.index, |port, base| {
            let data = (base + 4096) as *mut u8;
            if let Some(input) = input {
                unsafe {
                    ptr::copy_nonoverlapping(input.as_ptr(), data, 512);
                }
            }
            issue(
                port,
                base,
                Some(Range {
                    lba: u64::from(lba),
                    count: 1,
                    extended: self.geometry.extended,
                    write: input.is_some(),
                }),
                0,
                512,
            )?;
            if let Some(output) = output {
                unsafe {
                    ptr::copy_nonoverlapping(data, output.as_mut_ptr(), 512);
                }
            }
            Ok(())
        })
        .is_ok();
        if input.is_some() {
            crate::bios_disk::invalidate_cache();
        }
        crate::disk_io::checkpoint();
        result
    }
}

impl crate::blockdev::BlockDevice for Disk {
    fn read_block(&self, lba: u32, buffer: &mut [u8]) -> bool {
        self.transfer(lba, None, Some(buffer))
    }
    fn write_block(&self, lba: u32, buffer: &[u8]) -> bool {
        if crate::diskfs::live_boot() { return false; }
        self.transfer(lba, Some(buffer), None)
    }
    fn block_size(&self) -> usize {
        512
    }
    fn block_count(&self) -> u32 {
        self.geometry.sectors as u32
    }
    fn flush(&self) -> bool {
        with_port(self.device, self.index, |port, base| {
            issue(port, base, None, self.geometry.flush, 0)
        })
        .is_ok()
    }
}
