// Ring-3 window ABI v1: fixed 640x400 client, 8x16 text cells, RGB24 colours.
pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 400;
pub const MAX_COMMANDS: usize = 16;
pub const KEY: u32 = 1;
pub const POINTER: u32 = 2;
pub const CLOSE: u32 = 3;
pub const RESET: u32 = 4;
pub const RESIZE: u32 = 5;
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: u32,
    pub code: u32,
    pub x: i32,
    pub y: i32,
    pub buttons: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Draw {
    pub kind: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub color: u32,
    pub pointer: u32,
    pub length: u32,
}
#[allow(dead_code)]
impl Draw {
    pub fn rect(x: u32, y: u32, width: u32, height: u32, color: u32) -> Self {
        Self {
            kind: 0,
            x,
            y,
            width,
            height,
            color,
            pointer: 0,
            length: 0,
        }
    }
    pub fn text(x: u32, y: u32, color: u32, text: &[u8]) -> Self {
        Self {
            kind: 1,
            x,
            y,
            color,
            pointer: text.as_ptr() as u32,
            length: text.len() as u32,
            ..Self::default()
        }
    }
    pub fn valid(&self) -> bool {
        self.valid_for(WIDTH, HEIGHT)
    }
    pub fn valid_for(&self, width: u32, height: u32) -> bool {
        if self.color > 0xffffff {
            return false;
        }
        let (w, h) = match self.kind {
            0 if self.pointer == 0 && self.length == 0 => (self.width, self.height),
            1 if self.width == 0 && self.height == 0 && self.length <= 96 => (self.length * 8, 16),
            _ => return false,
        };
        self.x.checked_add(w).is_some_and(|end| end <= width)
            && self.y.checked_add(h).is_some_and(|end| end <= height)
    }
}
const _: () = assert!(core::mem::size_of::<Event>() == 20 && core::mem::size_of::<Draw>() == 32);

pub fn valid_size(width: u32, height: u32) -> bool {
    (160..=768).contains(&width)
        && (80..=528).contains(&height)
        && width % 8 == 0
        && height % 16 == 0
}

pub fn resize_limits(min: u32, max: u32, width: u32, height: u32) -> Option<(i32, i32, i32, i32)> {
    let (a, b, c, d) = (min & 65535, min >> 16, max & 65535, max >> 16);
    if !valid_size(a, b) || !valid_size(c, d) || a > width || b > height || c < width || d < height
    {
        return None;
    }
    Some((a as i32, b as i32, c as i32, d as i32))
}
