// Owned physical backing for private user code/stack mappings.
// Foreground and scheduled flat/static ELF images with private writable data.
use crate::phys;

pub const FRAME_BYTES: usize = 4096;
pub const STACK_PAGES: usize = 4;
pub const KERNEL_STACK_PAGES: usize = 16;
pub const MAX_BINARY_BYTES: usize = crate::program_image::MAX_FILE;
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
    pub profile: crate::permissions::Profile,
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
    FlatTooLarge,
    InvalidElf(crate::elf::Error),
    Memory,
}
impl LoadError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "Program is empty.",
            Self::TooLarge => "Program file exceeds 1 MiB.",
            Self::FlatTooLarge => "Flat program exceeds 64 KiB; build larger applications as ELF.",
            Self::InvalidElf(_) => "Invalid or unsupported ELF executable.",
            Self::Memory => "Not enough memory for program code/stack.",
        }
    }
}
pub fn load(data: &[u8]) -> Result<LoadedProgram, LoadError> {
    let layout=crate::program_image::parse(data).map_err(|e|match e {
        crate::program_image::Error::Empty=>LoadError::Empty,
        crate::program_image::Error::TooLarge=>LoadError::TooLarge,
        crate::program_image::Error::FlatTooLarge=>LoadError::FlatTooLarge,
        crate::program_image::Error::InvalidElf(e)=>LoadError::InvalidElf(e),
    })?;
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
        profile: crate::permissions::Profile::Trusted,
        layout,
        image,
        stack,
        size: layout.span,
        arguments: [0; 128],
        argument_length: 0,
    })
}

/// Serialized filesystem reader, used synchronously at boot and on the desktop worker.
pub fn from_disk(path:&str)->Result<LoadedProgram,&'static str> {
    let staging=Frames::new(MAX_BINARY_BYTES/FRAME_BYTES).ok_or("Not enough memory for the program loader.")?;
    let data=unsafe {core::slice::from_raw_parts_mut(staging.base() as *mut u8,MAX_BINARY_BYTES)};
    let mut resolved=[0;105];
    let(size,n)=crate::diskfs::read_executable_resolved(path,data,&mut resolved).map_err(|e|e.message())?;
    let resolved=core::str::from_utf8(&resolved[..n]).unwrap();
    let metadata=crate::app_metadata::load(resolved)?;
    let mut program=load(&data[..size]).map_err(|e|e.message())?;
    program.profile=crate::permissions::Profile::for_name(resolved);
    if let Some(metadata)=metadata {program.profile=program.profile.restrict(crate::app_metadata::profile(metadata.access));crate::debug::write_fmt(format_args!("App manifest: path={} access={}\n",resolved,program.profile.label()));}
    Ok(program)
}
