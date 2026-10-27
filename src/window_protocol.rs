// Ring-3 window ABI v1: fixed 640x400 client, 8x16 text cells, RGB24 colours.
pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 400;
pub const MAX_COMMANDS: usize = 16;
pub const KEY: u32 = 1;
pub const POINTER: u32 = 2;
pub const CLOSE: u32 = 3;
pub const RESET: u32 = 4;
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
        if self.color > 0xffffff {
            return false;
        }
        let (w, h) = match self.kind {
            0 if self.pointer == 0 && self.length == 0 => (self.width, self.height),
            1 if self.width == 0 && self.height == 0 && self.length <= 80 => (self.length * 8, 16),
            _ => return false,
        };
        self.x.checked_add(w).is_some_and(|end| end <= WIDTH)
            && self.y.checked_add(h).is_some_and(|end| end <= HEIGHT)
    }
}
const _: () = assert!(core::mem::size_of::<Event>() == 20 && core::mem::size_of::<Draw>() == 32);
