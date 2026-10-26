// PCI-routed IDE LBA28/LBA48 PIO. Called from shell context, never from IRQs.
use crate::{
    ata_poll::Budget,
    blockdev::BlockDevice,
    pci::{self, IdeChannel},
};
use core::sync::atomic::{AtomicU32, Ordering};
fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!("in al, dx",out("al") value,in("dx") port,options(nomem,nostack,preserves_flags));
    }
    value
}
fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al",in("dx") port,in("al") value,options(nomem,nostack,preserves_flags));
    }
}
#[derive(Clone, Copy)]
#[repr(u32)]
enum Failure {
    None,
    Routing,
    Absent,
    Timeout,
    Device,
    Packet,
    NoLba,
    Capacity,
    SectorSize,
}
pub struct AtaDisk {
    drive: AtomicU32,
    sectors: AtomicU32,
    physical_low: AtomicU32,
    physical_high: AtomicU32,
    lba48: AtomicU32,
    ports: AtomicU32,
    failure: AtomicU32,
    registers: AtomicU32,
    model: [AtomicU32; 10],
}
impl AtaDisk {
    pub const fn new(drive: u8) -> Self {
        Self {
            drive: AtomicU32::new(drive as u32),
            sectors: AtomicU32::new(0),
            physical_low: AtomicU32::new(0),
            physical_high: AtomicU32::new(0),
            lba48: AtomicU32::new(0),
            ports: AtomicU32::new(0),
            failure: AtomicU32::new(0),
            registers: AtomicU32::new(0),
            model: [const { AtomicU32::new(0) }; 10],
        }
    }
    pub fn at(channel: IdeChannel, drive: u8) -> Self {
        let disk = Self::new(drive);
        disk.ports.store(
            channel.io as u32 | (channel.control as u32) << 16,
            Ordering::Relaxed,
        );
        disk
    }
    pub fn configure(&self, channel: IdeChannel, drive: u8) {
        self.lba48.store(0, Ordering::Relaxed);
        self.sectors.store(0, Ordering::Relaxed);
        self.physical_low.store(0, Ordering::Relaxed);
        self.physical_high.store(0, Ordering::Relaxed);
        self.drive.store(drive as u32, Ordering::Relaxed);
        self.ports.store(
            channel.io as u32 | (channel.control as u32) << 16,
            Ordering::Relaxed,
        );
    }
    fn drive(&self) -> u8 {
        self.drive.load(Ordering::Relaxed) as u8
    }
    pub fn adopt(&self, other: &Self) {
        self.lba48
            .store(other.lba48.load(Ordering::Relaxed), Ordering::Relaxed);
        self.physical_low.store(
            other.physical_low.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.physical_high.store(
            other.physical_high.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.drive
            .store(other.drive.load(Ordering::Relaxed), Ordering::Relaxed);
        self.ports
            .store(other.ports.load(Ordering::Relaxed), Ordering::Relaxed);
        self.failure
            .store(other.failure.load(Ordering::Relaxed), Ordering::Relaxed);
        self.registers
            .store(other.registers.load(Ordering::Relaxed), Ordering::Relaxed);
        for i in 0..10 {
            self.model[i].store(other.model[i].load(Ordering::Relaxed), Ordering::Relaxed);
        }
        self.sectors.store(other.block_count(), Ordering::Relaxed);
    }
    pub fn unavailable(&self) {
        self.lba48.store(0, Ordering::Relaxed);
        self.physical_low.store(0, Ordering::Relaxed);
        self.physical_high.store(0, Ordering::Relaxed);
        self.ports.store(0, Ordering::Relaxed);
        self.sectors.store(0, Ordering::Relaxed);
        self.failure
            .store(Failure::Routing as u32, Ordering::Relaxed);
        self.registers.store(0, Ordering::Relaxed);
    }
    fn channel(&self) -> IdeChannel {
        let value = self.ports.load(Ordering::Relaxed);
        IdeChannel {
            io: value as u16,
            control: (value >> 16) as u16,
        }
    }
    fn delay(&self) {
        for _ in 0..4 {
            inb(self.channel().control);
        }
    }
    fn fail(&self, failure: Failure) -> bool {
        self.failure.store(failure as u32, Ordering::Relaxed);
        if self.ports.load(Ordering::Relaxed) != 0 {
            let io = self.channel().io;
            let status = inb(io + 7);
            let error = if status & 1 != 0 { inb(io + 1) } else { 0 };
            self.registers.store(
                status as u32
                    | (error as u32) << 8
                    | (inb(io + 4) as u32) << 16
                    | (inb(io + 5) as u32) << 24,
                Ordering::Relaxed,
            );
        }
        false
    }
    fn wait(&self, data: bool, check_error: bool, ticks: u32) -> bool {
        let mut budget = Budget::new(crate::interrupts::timer_ticks(), ticks);
        loop {
            let status = inb(self.channel().io + 7);
            if status == 0 || status == 0xff {
                return self.fail(Failure::Absent);
            }
            if status & 0x80 == 0 {
                if check_error && status & 0x21 != 0 {
                    return self.fail(Failure::Device);
                }
                if (status & 8 != 0) == data {
                    return true;
                }
            }
            if budget.expired(crate::interrupts::timer_ticks()) {
                return self.fail(Failure::Timeout);
            }
            core::hint::spin_loop();
        }
    }
    pub fn init(&self) -> bool {
        self.lba48.store(0, Ordering::Relaxed);
        self.sectors.store(0, Ordering::Relaxed);
        self.physical_low.store(0, Ordering::Relaxed);
        self.physical_high.store(0, Ordering::Relaxed);
        self.failure.store(0, Ordering::Relaxed);
        self.registers.store(0, Ordering::Relaxed);
        if self.drive() > 1 {
            return self.fail(Failure::Routing);
        }
        if self.ports.load(Ordering::Relaxed) == 0 {
            let Some(channel) = pci::primary_channel() else {
                return self.fail(Failure::Routing);
            };
            self.ports.store(
                channel.io as u32 | (channel.control as u32) << 16,
                Ordering::Relaxed,
            );
        }
        let channel = self.channel();
        outb(channel.control, 2); // Polling: disable ATA IRQs, no reset or mode change.
        outb(channel.io + 6, 0xa0 | (self.drive() << 4));
        self.delay();
        if !self.wait(false, false, 500) {
            return false;
        }
        for register in 2..=5 {
            outb(channel.io + register, 0);
        }
        outb(channel.io + 7, 0xec); // IDENTIFY DEVICE, does not write media.
        self.delay();
        let ready = self.wait(true, true, 500);
        let signature = (inb(channel.io + 4), inb(channel.io + 5));
        if signature == (0x14, 0xeb) || signature == (0x69, 0x96) {
            return self.fail(Failure::Packet);
        }
        if !ready {
            return false;
        }
        if signature != (0, 0) {
            return self.fail(Failure::Device);
        }
        let mut ident = [0u16; 256];
        for word in &mut ident {
            unsafe {
                core::arch::asm!("in ax, dx",out("ax") *word,in("dx") channel.io,options(nomem,nostack,preserves_flags));
            }
        }
        self.delay();
        if !self.wait(false, true, 200) {
            return false;
        }
        if ident[49] & (1 << 9) == 0 {
            return self.fail(Failure::NoLba);
        }
        if ident[106] & 0xd000 == 0x5000 && (ident[117] != 256 || ident[118] != 0) {
            return self.fail(Failure::SectorSize);
        }
        let Some(capacity) = crate::ata_capacity::decode(&ident) else {
            return self.fail(Failure::Capacity);
        };
        let mut text = [b' '; 40];
        for i in 0..20 {
            text[i * 2] = (ident[27 + i] >> 8) as u8;
            text[i * 2 + 1] = ident[27 + i] as u8;
        }
        for i in 0..10 {
            self.model[i].store(
                u32::from_le_bytes([
                    text[i * 4],
                    text[i * 4 + 1],
                    text[i * 4 + 2],
                    text[i * 4 + 3],
                ]),
                Ordering::Relaxed,
            );
        }
        self.sectors.store(capacity.accessible, Ordering::Relaxed);
        self.lba48.store(capacity.lba48 as u32, Ordering::Relaxed);
        let physical = capacity.physical.unwrap_or(0);
        self.physical_low.store(physical as u32, Ordering::Relaxed);
        self.physical_high
            .store((physical >> 32) as u32, Ordering::Relaxed);
        self.failure.store(Failure::None as u32, Ordering::Relaxed);
        true
    }
    fn select(&self) -> bool {
        if self.block_count() == 0 {
            return false;
        }
        outb(self.channel().io + 6, 0xe0 | (self.drive() << 4));
        self.delay();
        self.wait(false, false, 200)
    }
    fn command(&self, block: u32, write: bool) -> bool {
        if block >= self.block_count() || !self.select() {
            return false;
        }
        let io = self.channel().io;
        let extended = self.lba48.load(Ordering::Relaxed) != 0;
        outb(
            io + 6,
            0xe0 | (self.drive() << 4)
                | if extended {
                    0
                } else {
                    (block >> 24) as u8 & 15
                },
        );
        self.delay();
        if extended {
            // ATA task-file FIFOs: high-order bytes first, then low-order.
            // The public block number is u32; bits 32..47 are always zero.
            outb(io + 1, 0);
            outb(io + 2, 0);
            outb(io + 3, (block >> 24) as u8);
            outb(io + 4, 0);
            outb(io + 5, 0);
            outb(io + 1, 0);
        }
        outb(io + 2, 1);
        outb(io + 3, block as u8);
        outb(io + 4, (block >> 8) as u8);
        outb(io + 5, (block >> 16) as u8);
        outb(
            io + 7,
            match (extended, write) {
                (false, false) => 0x20,
                (false, true) => 0x30,
                (true, false) => 0x24,
                (true, true) => 0x34,
            },
        );
        self.delay();
        self.wait(true, true, 200)
    }
    pub fn model_name(&self)->[u8;40] {
        let mut name=[0;40];
        for i in 0..10 {name[i*4..i*4+4].copy_from_slice(&self.model[i].load(Ordering::Relaxed).to_le_bytes());}
        name
    }
    pub fn report(&self) {
        let channel = self.channel();
        let value = self.failure.load(Ordering::Relaxed);
        let reason = match value {
            0 => "identified",
            1 => "PCI IDE routing unavailable/disabled",
            2 => "no response (00/FF)",
            3 => "poll timeout/timer stalled",
            4 => "device ERR/DF or unexpected signature",
            5 => "ATAPI packet device (not a hard disk)",
            6 => "LBA addressing unsupported",
            7 => "invalid ATA capacity",
            8 => "non-512-byte logical sectors unsupported",
            _ => "unknown",
        };
        crate::println!(
            "  I/O={:#06x} ctrl={:#06x} {}: {}",
            channel.io,
            channel.control,
            if self.drive() == 0 { "master" } else { "slave" },
            reason
        );
        if self.block_count() == 0 {
            let r = self.registers.load(Ordering::Relaxed);
            crate::println!(
                "  status={:#04x} error={:#04x} signature={:02x}/{:02x}",
                r & 255,
                (r >> 8) & 255,
                (r >> 16) & 255,
                r >> 24
            );
        } else {
            let mut text = [0u8; 40];
            for i in 0..10 {
                text[i * 4..i * 4 + 4]
                    .copy_from_slice(&self.model[i].load(Ordering::Relaxed).to_le_bytes());
            }
            crate::println!(
                "  {} ({} MiB accessible)",
                core::str::from_utf8(&text)
                    .unwrap_or("(non-ASCII model)")
                    .trim(),
                self.block_count() / 2048
            );
            crate::println!(
                "  Addressing: {} PIO",
                if self.lba48.load(Ordering::Relaxed) != 0 {
                    "LBA48"
                } else {
                    "LBA28"
                }
            );
            match self.physical_block_count() {
                Some(full) if full != self.block_count() as u64 => crate::println!(
                    "  Full device: {} MiB. Exceeds block-device address range.",
                    full / 2048
                ),
                None => {
                    crate::println!("  Full capacity unknown. Whole-disk formatting is disabled.")
                }
                _ => (),
            }
        }
    }
}
impl BlockDevice for AtaDisk {
    fn read_block(&self, block: u32, buffer: &mut [u8]) -> bool {
        if buffer.len() < 512 || !self.command(block, false) {
            return false;
        }
        let io = self.channel().io;
        for chunk in buffer[..512].chunks_exact_mut(2) {
            let word: u16;
            unsafe {
                core::arch::asm!("in ax, dx",out("ax") word,in("dx") io,options(nomem,nostack,preserves_flags));
            }
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        self.delay();
        self.wait(false, true, 200)
    }
    fn write_block(&self, block: u32, buffer: &[u8]) -> bool {
        if buffer.len() < 512 || !self.command(block, true) {
            return false;
        }
        let io = self.channel().io;
        for chunk in buffer[..512].chunks_exact(2) {
            let word = u16::from_le_bytes([chunk[0], chunk[1]]);
            unsafe {
                core::arch::asm!("out dx, ax",in("ax") word,in("dx") io,options(nomem,nostack,preserves_flags));
            }
        }
        self.delay();
        self.wait(false, true, 200)
    }
    fn block_size(&self) -> usize {
        512
    }
    fn block_count(&self) -> u32 {
        self.sectors.load(Ordering::Relaxed)
    }
    fn physical_block_count(&self) -> Option<u64> {
        let value = self.physical_low.load(Ordering::Relaxed) as u64
            | (self.physical_high.load(Ordering::Relaxed) as u64) << 32;
        if value == 0 {
            None
        } else {
            Some(value)
        }
    }
    fn flush(&self) -> bool {
        if !self.select() {
            return false;
        }
        outb(
            self.channel().io + 7,
            if self.lba48.load(Ordering::Relaxed) != 0 {
                0xea
            } else {
                0xe7
            },
        );
        self.delay();
        self.wait(false, true, 1000)
    }
}
