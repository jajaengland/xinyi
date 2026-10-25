// Install functionality
use crate::{ata::AtaDisk, blockdev::BlockDevice, println, print};

extern "C" {
    static _kernel_start: u8;
    static _kernel_end: u8;
}

pub fn install_to_disk(drive: u8) {
    println!("Installing Xinyi to disk...");
    
    let disk = AtaDisk::new(drive);
    if !disk.init() {
        println!("Failed to initialize disk");
        return;
    }
    
    println!("Disk initialized, writing kernel...");
    
    // Get kernel size
    let kernel_start = unsafe { &_kernel_start as *const u8 as u32 };
    let kernel_end = unsafe { &_kernel_end as *const u8 as u32 };
    let kernel_size = kernel_end - kernel_start;
    
    println!("Kernel size: {} bytes", kernel_size);
    
    // Write kernel starting at sector 64 (after bootloader area)
    let mut sector = [0u8; 512];
    let kernel_ptr = kernel_start as *const u8;
    
    let mut bytes_written: u32 = 0;
    let mut sector_num: u32 = 64;
    
    while bytes_written < kernel_size {
        // Clear sector
        for b in sector.iter_mut() {
            *b = 0;
        }
        
        // Copy up to 512 bytes
        let mut copied = 0;
        while copied < 512 && bytes_written < kernel_size {
            let byte = unsafe { *kernel_ptr.add(bytes_written as usize) };
            sector[copied] = byte;
            copied += 1;
            bytes_written += 1;
        }
        
        if !disk.write_block(sector_num, &sector) {
            println!("Failed to write sector {}", sector_num);
            return;
        }
        
        sector_num += 1;
    }
    
    println!("Installation complete!");
    println!("Written {} sectors starting at sector 64", sector_num - 64);
}
