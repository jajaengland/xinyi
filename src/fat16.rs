// FAT16 filesystem implementation
use crate::blockdev::BlockDevice;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct BootSector {
    jmp: [u8; 3],
    oem: [u8; 8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    root_entries: u16,
    total_sectors_16: u16,
    media_type: u8,
    sectors_per_fat: u16,
    sectors_per_track: u16,
    num_heads: u16,
    hidden_sectors: u32,
    total_sectors_32: u32,
    drive_number: u8,
    reserved: u8,
    boot_sig: u8,
    volume_id: u32,
    volume_label: [u8; 11],
    fs_type: [u8; 8],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct DirEntry {
    name: [u8; 8],
    ext: [u8; 3],
    attr: u8,
    reserved: [u8; 10],
    time: u16,
    date: u16,
    cluster: u16,
    size: u32,
}

pub struct Fat16<'a> {
    device: &'a dyn BlockDevice,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    sectors_per_fat: u16,
    root_entries: u16,
    root_start: u32,
    data_start: u32,
}

impl<'a> Fat16<'a> {
    pub fn new(device: &'a dyn BlockDevice) -> Option<Self> {
        let mut boot_sector = [0u8; 512];
        if !device.read_block(0, &mut boot_sector) {
            return None;
        }

        let bytes_per_sector = u16::from_le_bytes([boot_sector[11], boot_sector[12]]);
        let sectors_per_cluster = boot_sector[13];
        let reserved_sectors = u16::from_le_bytes([boot_sector[14], boot_sector[15]]);
        let num_fats = boot_sector[16];
        let root_entries = u16::from_le_bytes([boot_sector[17], boot_sector[18]]);
        let sectors_per_fat = u16::from_le_bytes([boot_sector[22], boot_sector[23]]);

        if bytes_per_sector != 512 {
            return None;
        }

        let root_start = reserved_sectors as u32 + (num_fats as u32) * (sectors_per_fat as u32);
        let data_start = root_start + ((root_entries as u32) * 32 + bytes_per_sector as u32 - 1) / (bytes_per_sector as u32);

        Some(Fat16 {
            device,
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            sectors_per_fat,
            root_entries,
            root_start,
            data_start,
        })
    }

    pub fn list_root(&self) {
        crate::println!("Root directory:");
        let mut sector = [0u8; 512];
        for i in 0..((self.root_entries as u32) * 32 / 512) {
            if self.device.read_block(self.root_start + i, &mut sector) {
                for j in 0..16 {
                    let offset = j * 32;
                    let name = &sector[offset..offset + 8];
                    if name[0] == 0x00 || name[0] == 0xE5 {
                        continue;
                    }
                    if name[0] == 0x05 {
                        // Deleted entry marker
                        continue;
                    }
                    let attr = sector[offset + 11];
                    if attr == 0x0F {
                        // Long filename entry
                        continue;
                    }
                    crate::print!("  ");
                    for &b in name {
                        if b == 0x20 {
                            break;
                        }
                        crate::print!("{}", b as char);
                    }
                    if sector[offset + 8] != 0x20 {
                        crate::print!(".");
                        for &b in &sector[offset + 8..offset + 11] {
                            if b == 0x20 {
                                break;
                            }
                            crate::print!("{}", b as char);
                        }
                    }
                    crate::println!();
                }
            }
        }
    }
}
