// Disk filesystem integration
use crate::{ata::AtaDisk, blockdev::BlockDevice, println};

static mut DISK: Option<AtaDisk> = None;

pub unsafe fn get_disk() -> Option<&'static mut AtaDisk> {
    DISK.as_mut()
}

pub fn init() {
    let disk = AtaDisk::new(0);
    if disk.init() {
        unsafe {
            DISK = Some(disk);
        }
        println!("ATA disk initialized");
    } else {
        println!("No ATA disk found");
    }
}

pub fn test() {
    unsafe {
        if let Some(ref disk) = DISK {
            let mut buf = [0u8; 512];
            if disk.read_block(0, &mut buf) {
                println!("Read sector 0 successfully");
                if buf[510] == 0x55 && buf[511] == 0xAA {
                    println!("Valid boot signature");
                }
            } else {
                println!("Failed to read sector 0");
            }
        } else {
            println!("No disk available");
        }
    }
}
