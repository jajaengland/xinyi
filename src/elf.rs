//! Strict static ELF32/i386 program headers, independent of allocation and hardware.
// ABI reference: https://refspecs.linuxfoundation.org/elf/elfspec.pdf
pub const BASE: u32 = 0x40000000;
pub const IMAGE_END: u32 = 0x40100000;
pub const HEAP_BASE: u32 = IMAGE_END;
pub const HEAP_END: u32 = 0x40200000;
pub const MAX_SEGMENTS: usize = 16;
#[derive(Clone, Copy, Default, Debug)]
pub struct Segment {
    pub address: u32,
    pub offset: u32,
    pub file_size: u32,
    pub memory_size: u32,
    pub flags: u32,
}
#[derive(Clone, Copy)]
pub struct Image {
    pub segments: [Segment; MAX_SEGMENTS],
    pub count: usize,
    pub entry: u32,
    pub span: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Unsupported,
    TooLarge,
    Overlap,
    Entry,
}
fn word(b: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4).ok_or(Error::Invalid)?.try_into().unwrap(),
    ))
}
fn half(b: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2).ok_or(Error::Invalid)?.try_into().unwrap(),
    ))
}
pub fn parse(b: &[u8]) -> Result<Image, Error> {
    if b.len() < 52 || b.get(..7) != Some(&[127, 69, 76, 70, 1, 1, 1]) {
        return Err(Error::Invalid);
    }
    if b[7] != 0 || b[8] != 0 || word(b, 36)? != 0 {
        return Err(Error::Unsupported);
    }
    if half(b, 16)? != 2
        || half(b, 18)? != 3
        || word(b, 20)? != 1
        || half(b, 40)? != 52
        || half(b, 42)? != 32
    {
        return Err(Error::Unsupported);
    }
    let count = half(b, 44)? as usize;
    let ph = word(b, 28)? as usize;
    if count == 0
        || count > MAX_SEGMENTS
        || ph < 52
        || ph.checked_add(count * 32).is_none_or(|e| e > b.len())
    {
        return Err(Error::Invalid);
    }
    let mut image = Image {
        segments: [Segment::default(); MAX_SEGMENTS],
        count: 0,
        entry: word(b, 24)?,
        span: 0,
    };
    for i in 0..count {
        let p = ph + i * 32;
        let kind = word(b, p)?;
        if matches!(kind, 2 | 3 | 5 | 7) {
            return Err(Error::Unsupported);
        }
        if kind == 0x6474e551 && word(b, p + 24)? & 1 != 0 {
            return Err(Error::Unsupported);
        }
        if kind != 1 {
            continue;
        }
        let s = Segment {
            offset: word(b, p + 4)?,
            address: word(b, p + 8)?,
            file_size: word(b, p + 16)?,
            memory_size: word(b, p + 20)?,
            flags: word(b, p + 24)?,
        };
        let align = word(b, p + 28)?;
        if s.file_size > s.memory_size
            || s.offset
                .checked_add(s.file_size)
                .is_none_or(|n| n as usize > b.len())
            || s.flags & 4 == 0
            || s.flags & !7 != 0
            || s.flags & 3 == 3
        {
            return Err(Error::Invalid);
        }
        if align > 1 && (!align.is_power_of_two() || s.address % align != s.offset % align) {
            return Err(Error::Invalid);
        }
        if s.address % 4096 != s.offset % 4096 {
            return Err(Error::Invalid);
        }
        if s.memory_size == 0 {
            continue;
        }
        let end = s
            .address
            .checked_add(s.memory_size)
            .ok_or(Error::TooLarge)?;
        if s.address < BASE || end > IMAGE_END {
            return Err(Error::TooLarge);
        }
        let first = s.address & !4095;
        let last = (end + 4095) & !4095;
        for old in &image.segments[..image.count] {
            let a = old.address & !4095;
            let z = (old.address + old.memory_size + 4095) & !4095;
            if first < z && a < last {
                return Err(Error::Overlap);
            }
        }
        image.span = image.span.max(end - BASE);
        image.segments[image.count] = s;
        image.count += 1;
    }
    if image.count == 0
        || !image.segments[..image.count].iter().any(|s| {
            s.flags & 1 != 0 && image.entry >= s.address && image.entry < s.address + s.file_size
        })
    {
        return Err(Error::Entry);
    }
    Ok(image)
}

impl Image {
    pub fn permits(&self, pointer: u32, length: u32, write: bool, heap_end: u32) -> bool {
        if length == 0 {
            return true;
        }
        let Some(end) = pointer.checked_add(length) else {
            return false;
        };
        let mut at = pointer;
        while at < end {
            let mut region_end = 0;
            if (0x403fc000..0x40400000).contains(&at) {
                region_end = 0x40400000;
            }
            if heap_end <= HEAP_END && at >= HEAP_BASE && at < heap_end {
                region_end = heap_end;
            }
            for s in &self.segments[..self.count] {
                if at >= s.address && at < s.address + s.memory_size && (!write || s.flags & 2 != 0)
                {
                    region_end = region_end.max(s.address + s.memory_size);
                }
            }
            if region_end <= at {
                return false;
            }
            at = end.min(region_end);
        }
        true
    }
}
