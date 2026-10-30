//! Executable format limits, validated before physical backing is allocated.
use crate::elf;
pub const MAX_FILE: usize = 1024 * 1024;
pub const MAX_FLAT: usize = 64 * 1024;
#[derive(Debug)]
pub enum Error {
    Empty,
    TooLarge,
    FlatTooLarge,
    InvalidElf(elf::Error),
}
pub fn parse(data: &[u8]) -> Result<elf::Image, Error> {
    if data.is_empty() {
        return Err(Error::Empty);
    }
    if data.len() > MAX_FILE {
        return Err(Error::TooLarge);
    }
    if data.starts_with(b"\x7fELF") {
        return elf::parse(data).map_err(Error::InvalidElf);
    }
    if data.len() > MAX_FLAT {
        return Err(Error::FlatTooLarge);
    }
    let mut segments = [elf::Segment::default(); elf::MAX_SEGMENTS];
    segments[0] = elf::Segment {
        address: elf::BASE,
        offset: 0,
        file_size: data.len() as u32,
        memory_size: data.len() as u32,
        flags: 5,
    };
    Ok(elf::Image {
        segments,
        count: 1,
        entry: elf::BASE,
        span: data.len() as u32,
    })
}
