use core::cell::RefCell;
pub trait BlockDevice {
    fn read_block(&self, block: u32, buffer: &mut [u8]) -> bool;
    fn write_block(&self, block: u32, buffer: &[u8]) -> bool;
    fn block_size(&self) -> usize;
    fn block_count(&self) -> u32;
    // Full device size, not just the driver's addressable prefix. None means
    // IDENTIFY did not establish it reliably; destructive whole-disk work fails.
    fn physical_block_count(&self) -> Option<u64> {
        Some(self.block_count() as u64)
    }
    fn flush(&self) -> bool {
        true
    }
}
// Interior mutability implements the shared-reference block-device contract.
pub struct RamDisk {
    data: RefCell<&'static mut [u8]>,
    block_size: usize,
}
impl RamDisk {
    pub const fn new(data: &'static mut [u8], block_size: usize) -> Self {
        Self {
            data: RefCell::new(data),
            block_size,
        }
    }
}
impl BlockDevice for RamDisk {
    fn read_block(&self, block: u32, buffer: &mut [u8]) -> bool {
        if block >= self.block_count() || buffer.len() < self.block_size {
            return false;
        }
        let offset = block as usize * self.block_size;
        buffer[..self.block_size]
            .copy_from_slice(&self.data.borrow()[offset..offset + self.block_size]);
        true
    }
    fn write_block(&self, block: u32, buffer: &[u8]) -> bool {
        if block >= self.block_count() || buffer.len() < self.block_size {
            return false;
        }
        let offset = block as usize * self.block_size;
        self.data.borrow_mut()[offset..offset + self.block_size]
            .copy_from_slice(&buffer[..self.block_size]);
        true
    }
    fn block_size(&self) -> usize {
        self.block_size
    }
    fn block_count(&self) -> u32 {
        if self.block_size == 0 {
            0
        } else {
            (self.data.borrow().len() / self.block_size) as u32
        }
    }
}
