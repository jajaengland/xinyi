// Block device interface for storage
pub trait BlockDevice {
    fn read_block(&self, block: u32, buffer: &mut [u8]) -> bool;
    fn write_block(&self, block: u32, buffer: &[u8]) -> bool;
    fn block_size(&self) -> usize;
}

// Simple RAM disk for testing
pub struct RamDisk {
    data: &'static mut [u8],
    block_size: usize,
}

impl RamDisk {
    pub const fn new(data: &'static mut [u8], block_size: usize) -> Self {
        RamDisk { data, block_size }
    }
}

impl BlockDevice for RamDisk {
    fn read_block(&self, block: u32, buffer: &mut [u8]) -> bool {
        let offset = (block as usize) * self.block_size;
        if offset + self.block_size > self.data.len() || buffer.len() < self.block_size {
            return false;
        }
        for i in 0..self.block_size {
            buffer[i] = self.data[offset + i];
        }
        true
    }

    fn write_block(&self, block: u32, buffer: &[u8]) -> bool {
        let offset = (block as usize) * self.block_size;
        if offset + self.block_size > self.data.len() || buffer.len() < self.block_size {
            return false;
        }
        // We need mutable access - simplified for now
        true
    }

    fn block_size(&self) -> usize {
        self.block_size
    }
}
