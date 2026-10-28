//! Allocation-free GUI interaction models. Coordinates are client-relative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}
#[derive(Clone, Copy)]
pub struct Button {
    pub rect: Rect,
    pub pressed: bool,
    pub hover: bool,
    buttons: u32,
}
impl Button {
    pub const fn new(rect: Rect) -> Self {
        Self {
            rect,
            pressed: false,
            hover: false,
            buttons: 0,
        }
    }
    pub fn reset(&mut self, buttons: u32) {
        self.pressed = false;
        self.buttons = buttons;
        self.hover = false;
    }
    pub fn pointer(&mut self, x: i32, y: i32, buttons: u32) -> bool {
        self.hover = self.rect.contains(x, y);
        let down = buttons & 1 != 0;
        let old = self.buttons & 1 != 0;
        let clicked = !down && old && self.pressed && self.hover;
        if down && !old {
            self.pressed = self.hover;
        }
        if !down {
            self.pressed = false;
        }
        self.buttons = buttons;
        clicked
    }
}
pub struct TextInput<const N: usize> {
    pub bytes: [u8; N],
    pub len: usize,
    pub cursor: usize,
    pub anchor: Option<usize>,
}
impl<const N: usize> TextInput<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
            cursor: 0,
            anchor: None,
        }
    }
    pub fn set(&mut self, text: &[u8]) -> bool {
        if text.len() > N || !text.iter().all(|b| (32..=126).contains(b)) {
            return false;
        }
        self.bytes[..text.len()].copy_from_slice(text);
        self.len = text.len();
        self.cursor = self.len;
        self.anchor = None;
        true
    }
    pub fn range(&self) -> (usize, usize) {
        self.anchor.map_or((self.cursor, self.cursor), |a| {
            (a.min(self.cursor), a.max(self.cursor))
        })
    }
    pub fn insert(&mut self, text: &[u8]) -> bool {
        let (a, b) = self.range();
        if text.len() > N - (self.len - (b - a)) || !text.iter().all(|b| (32..=126).contains(b)) {
            return false;
        }
        self.bytes.copy_within(b..self.len, a + text.len());
        self.bytes[a..a + text.len()].copy_from_slice(text);
        self.len = self.len - (b - a) + text.len();
        self.cursor = a + text.len();
        self.anchor = None;
        true
    }
    pub fn key(&mut self, event: u32) -> bool {
        let code = event as u16;
        let ctrl = event & (1 << 17) != 0;
        let shift = event & (1 << 16) != 0;
        if ctrl && matches!(code, 65 | 97) {
            self.anchor = Some(0);
            self.cursor = self.len;
            return true;
        }
        if matches!(code, 0x102..=0x105) {
            let old = self.cursor;
            let (a, b) = self.range();
            self.cursor = match code {
                0x102 if !shift && a != b => a,
                0x103 if !shift && a != b => b,
                0x102 => old.saturating_sub(1),
                0x103 => (old + 1).min(self.len),
                0x104 => 0,
                _ => self.len,
            };
            if shift {
                if self.anchor.is_none() {
                    self.anchor = Some(old);
                }
            } else {
                self.anchor = None;
            }
            return true;
        }
        if matches!(code, 8 | 0x106) {
            let (a, b) = self.range();
            if a == b {
                if code == 8 && self.cursor > 0 {
                    self.anchor = Some(self.cursor - 1);
                } else if code == 0x106 && self.cursor < self.len {
                    self.anchor = Some(self.cursor + 1);
                } else {
                    return false;
                }
            }
            return self.insert(b"");
        }
        if (32..=126).contains(&code) && event & ((1 << 17) | (1 << 18)) == 0 {
            return self.insert(&[code as u8]);
        }
        false
    }
}
pub struct Scrollbar {
    pub rect: Rect,
    pub value: u32,
    pub max: u32,
    pub page: u32,
    drag: Option<i32>,
    buttons: u32,
}
impl Scrollbar {
    pub const fn new(rect: Rect) -> Self {
        Self {
            rect,
            value: 0,
            max: 0,
            page: 1,
            drag: None,
            buttons: 0,
        }
    }
    pub fn reset(&mut self, buttons: u32) {
        self.drag = None;
        self.buttons = buttons;
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn range(&mut self, max: u32, page: u32) {
        self.max = max;
        self.page = page.max(1);
        self.value = self.value.min(max);
    }
    pub fn thumb(&self) -> Rect {
        let track = (self.rect.h - 32).max(16);
        let h = ((track as u64 * self.page as u64 / (self.page as u64 + self.max as u64)) as i32)
            .clamp(16, track);
        let travel = track - h;
        let dy = if self.max == 0 {
            0
        } else {
            (travel as u64 * self.value as u64 / self.max as u64) as i32
        };
        Rect::new(self.rect.x, self.rect.y + 16 + dy, self.rect.w, h)
    }
    pub fn pointer(&mut self, x: i32, y: i32, buttons: u32) -> bool {
        let before = self.value;
        let down = buttons & 1 != 0;
        let old = self.buttons & 1 != 0;
        let thumb = self.thumb();
        if down && !old && self.rect.contains(x, y) {
            if y < self.rect.y + 16 {
                self.value = self.value.saturating_sub(1);
            } else if y >= self.rect.y + self.rect.h - 16 {
                self.value = self.value.saturating_add(1).min(self.max);
            } else if thumb.contains(x, y) {
                self.drag = Some(y - thumb.y);
            } else if y < thumb.y {
                self.value = self.value.saturating_sub(self.page);
            } else {
                self.value = self.value.saturating_add(self.page).min(self.max);
            }
        }
        if down {
            if let Some(offset) = self.drag {
                let travel = self.rect.h - 32 - thumb.h;
                if travel > 0 {
                    let pos = (y - self.rect.y - 16 - offset).clamp(0, travel);
                    self.value =
                        ((pos as u64 * self.max as u64 + travel as u64 / 2) / travel as u64) as u32;
                }
            }
        } else {
            self.drag = None;
        }
        self.buttons = buttons;
        self.value != before
    }
}
