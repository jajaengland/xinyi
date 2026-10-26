// Owned physical backing for private user code/stack mappings.
// Flat images run synchronously; ELF parsing and writable globals are pending.
use crate::phys;

pub const FRAME_BYTES: usize = 4096;
pub const STACK_PAGES: usize = 4;
pub const MAX_BINARY_BYTES: usize = 64 * 1024;
pub const DEMO_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/demo.bin"));

pub struct Frames {
    base: u32,
    pages: usize,
}
impl Frames {
    pub fn new(pages: usize) -> Option<Self> {
        let base = phys::alloc_frames(pages)?;
        let end = base + (pages as u64) * FRAME_BYTES as u64;
        if end > u32::MAX as u64 {
            for page in 0..pages {
                phys::free_frame(base + (page * FRAME_BYTES) as u64);
            }
            return None;
        }
        let frames = Self {
            base: base as u32,
            pages,
        };
        unsafe {
            core::ptr::write_bytes(frames.base as *mut u8, 0, pages * FRAME_BYTES);
        }
        Some(frames)
    }
    pub fn base(&self) -> u32 {
        self.base
    }
    pub fn top(&self) -> u32 {
        self.base + (self.pages * FRAME_BYTES) as u32
    }
}
impl Drop for Frames {
    fn drop(&mut self) {
        for page in 0..self.pages {
            phys::free_frame(self.base as u64 + (page * FRAME_BYTES) as u64);
        }
    }
}
pub struct LoadedProgram {
    pub image: Frames,
    pub stack: Frames,
    pub size: u32,
    pub arguments: [u8; 128],
    pub argument_length: usize,
}
pub fn load_binary(data: &[u8]) -> Option<LoadedProgram> {
    if data.is_empty() || data.len() > MAX_BINARY_BYTES {
        return None;
    }
    let image = Frames::new(data.len().div_ceil(FRAME_BYTES))?;
    let stack = Frames::new(STACK_PAGES)?;
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), image.base() as *mut u8, data.len());
    }
    Some(LoadedProgram {
        image,
        stack,
        size: data.len() as u32,
        arguments: [0;128],
        argument_length: 0,
    })
}
