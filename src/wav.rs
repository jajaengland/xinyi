//! Bounded RIFF PCM parsing, usable with a file-range reader; no whole-file allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub channels: u16,
    pub rate: u32,
    pub bits: u16,
    pub align: u16,
    pub data: u32,
    pub bytes: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Read,
    Invalid,
    Unsupported,
    TooManyChunks,
}
fn u16le(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([b[p], b[p + 1]])
}
fn u32le(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
pub fn parse(size: u32, mut read: impl FnMut(u32, &mut [u8]) -> bool) -> Result<Format, Error> {
    let mut header = [0; 12];
    if size < 12 || !read(0, &mut header) {
        return Err(Error::Read);
    }
    if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
        return Err(Error::Invalid);
    }
    let end = u32le(&header, 4)
        .checked_add(8)
        .filter(|&n| n >= 12 && n <= size)
        .ok_or(Error::Invalid)?;
    let mut at = 12;
    let mut format = None;
    let mut data = None;
    for _ in 0..64 {
        if at == end {
            break;
        }
        if end - at < 8 {
            return Err(Error::Invalid);
        }
        let mut h = [0; 8];
        if !read(at, &mut h) {
            return Err(Error::Read);
        }
        let len = u32le(&h, 4);
        let begin = at + 8;
        let next = begin
            .checked_add(len)
            .filter(|&n| n <= end)
            .ok_or(Error::Invalid)?;
        if &h[..4] == b"fmt " {
            if format.is_some() || len < 16 {
                return Err(Error::Invalid);
            }
            let mut b = [0; 16];
            if !read(begin, &mut b) {
                return Err(Error::Read);
            }
            let channels = u16le(&b, 2);
            let rate = u32le(&b, 4);
            let bits = u16le(&b, 14);
            let align = u16le(&b, 12);
            if u16le(&b, 0) != 1
                || !matches!(channels, 1 | 2)
                || !matches!(bits, 8 | 16)
                || !(8000..=48000).contains(&rate)
            {
                return Err(Error::Unsupported);
            }
            if align != channels * (bits / 8) || u32le(&b, 8) != rate * align as u32 {
                return Err(Error::Invalid);
            }
            format = Some((channels, rate, bits, align));
        } else if &h[..4] == b"data" {
            if data.is_some() {
                return Err(Error::Invalid);
            }
            data = Some((begin, len));
        }
        at = next
            .checked_add(len & 1)
            .filter(|&n| n <= end)
            .ok_or(Error::Invalid)?;
        if at == end {
            break;
        }
    }
    if at != end {
        return Err(Error::TooManyChunks);
    }
    let (channels, rate, bits, align) = format.ok_or(Error::Invalid)?;
    let (data, bytes) = data.ok_or(Error::Invalid)?;
    if bytes == 0 || bytes % align as u32 != 0 {
        return Err(Error::Invalid);
    }
    Ok(Format {
        channels,
        rate,
        bits,
        align,
        data,
        bytes,
    })
}
pub fn sample(format: Format, frame: &[u8], channel: usize) -> i16 {
    let channel = if format.channels == 1 { 0 } else { channel };
    if format.bits == 8 {
        ((frame[channel] as i32 - 128) * 256) as i16
    } else {
        i16::from_le_bytes([frame[channel * 2], frame[channel * 2 + 1]])
    }
}
