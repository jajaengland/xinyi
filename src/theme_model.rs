#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub desktop: u32,
    pub title: u32,
}
impl Theme {
    pub const DEFAULT: Self = Self {
        desktop: 0x20364f,
        title: 0x215c83,
    };
    pub fn valid(self) -> bool {
        self.desktop <= 0xffffff && self.title <= 0xffffff
    }
    pub fn encode(self) -> [u8; 14] {
        let mut out = [0; 14];
        for (start, value) in [(0, self.desktop), (7, self.title)] {
            for n in 0..6 {
                out[start + n] = b"0123456789ABCDEF"[((value >> (4 * (5 - n))) & 15) as usize];
            }
        }
        out[6] = b' ';
        out[13] = 10;
        out
    }
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != 14 || data[6] != b' ' || data[13] != 10 {
            return None;
        }
        Some(Self {
            desktop: hex(&data[..6])?,
            title: hex(&data[7..13])?,
        })
    }
}
pub fn hex(bytes: &[u8]) -> Option<u32> {
    if bytes.len() != 6 {
        return None;
    }
    let mut value = 0;
    for &b in bytes {
        let n = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return None,
        };
        value = (value << 4) | n as u32;
    }
    Some(value)
}
pub fn ink(color: u32) -> u32 {
    if ((color >> 16 & 255) * 299 + (color >> 8 & 255) * 587 + (color & 255) * 114) >= 140000 {
        0
    } else {
        0xffffff
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(Theme::parse(&Theme::DEFAULT.encode()), Some(Theme::DEFAULT));
        assert!(Theme::parse(b"BAD").is_none());
        assert!(!Theme {
            desktop: 0x1000000,
            title: 0
        }
        .valid());
        assert_eq!(hex(b"aBcDeF"), Some(0xabcdef));
        assert_eq!(hex(b"ZZZZZZ"), None);
    }
    #[test]
    fn readable_ink() {
        assert_eq!(ink(0xffffff), 0);
        assert_eq!(ink(0), 0xffffff);
    }
}
