// Owned physical backing for private user code/stack mappings.
// Foreground and scheduled flat/static ELF images with private writable data.
use crate::phys;

pub const FRAME_BYTES: usize = 4096;
pub const STACK_PAGES: usize = 4;
pub const KERNEL_STACK_PAGES: usize = 16;
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
    pub layout: crate::elf::Image,
    pub image: Frames,
    pub stack: Frames,
    pub size: u32,
    pub arguments: [u8; 128],
    pub argument_length: usize,
}
pub fn load_binary(data: &[u8]) -> Option<LoadedProgram> {
    load(data).ok()
}
#[derive(Debug)]
pub enum LoadError {
    Empty,
    TooLarge,
    InvalidElf(crate::elf::Error),
    Memory,
}
impl LoadError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "Program is empty.",
            Self::TooLarge => "Program file exceeds 64 KiB.",
            Self::InvalidElf(_) => "Invalid or unsupported ELF executable.",
            Self::Memory => "Not enough memory for program code/stack.",
        }
    }
}
pub fn load(data: &[u8]) -> Result<LoadedProgram, LoadError> {
    if data.is_empty() {
        return Err(LoadError::Empty);
    }
    if data.len() > MAX_BINARY_BYTES {
        return Err(LoadError::TooLarge);
    }
    let layout = if data.starts_with(b"\x7fELF") {
        crate::elf::parse(data).map_err(LoadError::InvalidElf)?
    } else {
        let mut segments = [crate::elf::Segment::default(); crate::elf::MAX_SEGMENTS];
        segments[0] = crate::elf::Segment {
            address: crate::elf::BASE,
            offset: 0,
            file_size: data.len() as u32,
            memory_size: data.len() as u32,
            flags: 5,
        };
        crate::elf::Image {
            segments,
            count: 1,
            entry: crate::elf::BASE,
            span: data.len() as u32,
        }
    };
    let image =
        Frames::new((layout.span as usize).div_ceil(FRAME_BYTES)).ok_or(LoadError::Memory)?;
    let stack = Frames::new(STACK_PAGES).ok_or(LoadError::Memory)?;
    for s in &layout.segments[..layout.count] {
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr().add(s.offset as usize),
                (image.base() + s.address - crate::elf::BASE) as *mut u8,
                s.file_size as usize,
            );
        }
    }
    Ok(LoadedProgram {
        layout,
        image,
        stack,
        size: layout.span,
        arguments: [0; 128],
        argument_length: 0,
    })
}
