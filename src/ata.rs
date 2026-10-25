// ATA/IDE disk driver for QEMU
// Primary IDE controller at 0x1F0-0x1F7

use crate::blockdev::BlockDevice;

const ATA_PRIMARY_IO: u16 = 0x1F0;
const ATA_PRIMARY_CTRL: u16 = 0x3F6;

const ATA_REG_DATA: u16 = 0x00;
const ATA_REG_ERROR: u16 = 0x01;
const ATA_REG_SECCOUNT0: u16 = 0x02;
const ATA_REG_LBA0: u16 = 0x03;
const ATA_REG_LBA1: u16 = 0x04;
const ATA_REG_LBA2: u16 = 0x05;
const ATA_REG_HDDEVSEL: u16 = 0x06;
const ATA_REG_COMMAND: u16 = 0x07;
const ATA_REG_STATUS: u16 = 0x07;

const ATA_CMD_READ_PIO: u8 = 0x20;
const ATA_CMD_WRITE_PIO: u8 = 0x30;
const ATA_CMD_IDENTIFY: u8 = 0xEC;

const ATA_SR_BSY: u8 = 0x80;
const ATA_SR_DRDY: u8 = 0x40;
const ATA_SR_DF: u8 = 0x20;
const ATA_SR_DSC: u8 = 0x10;
const ATA_SR_DRQ: u8 = 0x08;
const ATA_SR_CORR: u8 = 0x04;
const ATA_SR_IDX: u8 = 0x02;
const ATA_SR_ERR: u8 = 0x01;

fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

fn insw(port: u16, buffer: &mut [u16]) {
    unsafe {
        for word in buffer.iter_mut() {
            core::arch::asm!(
                "in ax, dx",
                out("ax") *word,
                in("dx") port,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

fn outsw(port: u16, buffer: &[u16]) {
    unsafe {
        for &word in buffer.iter() {
            core::arch::asm!(
                "out dx, ax",
                in("dx") port,
                in("ax") word,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

pub struct AtaDisk {
    drive: u8, // 0 = master, 1 = slave
}

impl AtaDisk {
    pub fn new(drive: u8) -> Self {
        AtaDisk { drive }
    }

    pub fn init(&self) -> bool {
        // Select drive
        outb(ATA_PRIMARY_IO + ATA_REG_HDDEVSEL, 0xA0 | (self.drive << 4));
        // Small delay
        for _ in 0..4 {
            inb(ATA_PRIMARY_CTRL);
        }
        
        outb(ATA_PRIMARY_IO + ATA_REG_SECCOUNT0, 0);
        outb(ATA_PRIMARY_IO + ATA_REG_LBA0, 0);
        outb(ATA_PRIMARY_IO + ATA_REG_LBA1, 0);
        outb(ATA_PRIMARY_IO + ATA_REG_LBA2, 0);
        outb(ATA_PRIMARY_IO + ATA_REG_COMMAND, ATA_CMD_IDENTIFY);
        
        // Check if drive exists
        let status = inb(ATA_PRIMARY_IO + ATA_REG_STATUS);
        if status == 0 {
            return false;
        }
        
        // Wait for BSY to clear
        loop {
            let status = inb(ATA_PRIMARY_IO + ATA_REG_STATUS);
            if status & ATA_SR_BSY == 0 {
                break;
            }
        }
        
        // Check if it's ATA
        let lba1 = inb(ATA_PRIMARY_IO + ATA_REG_LBA1);
        let lba2 = inb(ATA_PRIMARY_IO + ATA_REG_LBA2);
        if lba1 != 0 || lba2 != 0 {
            return false;
        }
        
        // Wait for DRQ
        loop {
            let status = inb(ATA_PRIMARY_IO + ATA_REG_STATUS);
            if status & ATA_SR_ERR != 0 {
                return false;
            }
            if status & ATA_SR_DRQ != 0 {
                break;
            }
        }
        
        // Read identification space
        let mut ident = [0u16; 256];
        insw(ATA_PRIMARY_IO + ATA_REG_DATA, &mut ident);
        
        true
    }
}

impl BlockDevice for AtaDisk {
    fn read_block(&self, block: u32, buffer: &mut [u8]) -> bool {
        if buffer.len() < 512 {
            return false;
        }
        
        // Wait for drive to be ready
        loop {
            let status = inb(ATA_PRIMARY_IO + ATA_REG_STATUS);
            if status & ATA_SR_BSY == 0 && status & ATA_SR_DRDY != 0 {
                break;
            }
        }
        
        outb(ATA_PRIMARY_IO + ATA_REG_HDDEVSEL, 0xE0 | (self.drive << 4) | ((block >> 24) & 0x0F) as u8);
        outb(ATA_PRIMARY_IO + ATA_REG_SECCOUNT0, 1);
        outb(ATA_PRIMARY_IO + ATA_REG_LBA0, block as u8);
        outb(ATA_PRIMARY_IO + ATA_REG_LBA1, (block >> 8) as u8);
        outb(ATA_PRIMARY_IO + ATA_REG_LBA2, (block >> 16) as u8);
        outb(ATA_PRIMARY_IO + ATA_REG_COMMAND, ATA_CMD_READ_PIO);
        
        // Wait for BSY to clear and DRQ to set
        loop {
            let status = inb(ATA_PRIMARY_IO + ATA_REG_STATUS);
            if status & ATA_SR_BSY == 0 {
                if status & ATA_SR_ERR != 0 {
                    return false;
                }
                if status & ATA_SR_DRQ != 0 {
                    break;
                }
            }
        }
        
        // Read 256 words (512 bytes)
        let mut words = [0u16; 256];
        insw(ATA_PRIMARY_IO + ATA_REG_DATA, &mut words);
        
        for (i, &word) in words.iter().enumerate() {
            buffer[i * 2] = word as u8;
            buffer[i * 2 + 1] = (word >> 8) as u8;
        }
        
        true
    }

    fn write_block(&self, block: u32, buffer: &[u8]) -> bool {
        if buffer.len() < 512 {
            return false;
        }
        
        loop {
            let status = inb(ATA_PRIMARY_IO + ATA_REG_STATUS);
            if status & ATA_SR_BSY == 0 && status & ATA_SR_DRDY != 0 {
                break;
            }
        }
        
        outb(ATA_PRIMARY_IO + ATA_REG_HDDEVSEL, 0xE0 | (self.drive << 4) | ((block >> 24) & 0x0F) as u8);
        outb(ATA_PRIMARY_IO + ATA_REG_SECCOUNT0, 1);
        outb(ATA_PRIMARY_IO + ATA_REG_LBA0, block as u8);
        outb(ATA_PRIMARY_IO + ATA_REG_LBA1, (block >> 8) as u8);
        outb(ATA_PRIMARY_IO + ATA_REG_LBA2, (block >> 16) as u8);
        outb(ATA_PRIMARY_IO + ATA_REG_COMMAND, ATA_CMD_WRITE_PIO);
        
        loop {
            let status = inb(ATA_PRIMARY_IO + ATA_REG_STATUS);
            if status & ATA_SR_BSY == 0 {
                if status & ATA_SR_ERR != 0 {
                    return false;
                }
                if status & ATA_SR_DRQ != 0 {
                    break;
                }
            }
        }
        
        let mut words = [0u16; 256];
        for (i, chunk) in buffer.chunks(2).enumerate().take(256) {
            words[i] = chunk[0] as u16 | ((chunk[1] as u16) << 8);
        }
        
        outsw(ATA_PRIMARY_IO + ATA_REG_DATA, &words);
        
        true
    }

    fn block_size(&self) -> usize {
        512
    }
}
