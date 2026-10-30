//! Allocation-free text metrics and layout shared by the kernel and application SDK.
//! Native 16-pixel bitmap faces; printable ASCII, with '?' as the fallback glyph.
#[path = "ui_font.rs"]
mod bitmap;
pub use bitmap::FONT;
pub const LINE_HEIGHT: u32 = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    Mono,
    Ui,
}
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub byte: u8,
    pub left: u8,
    pub width: u8,
    pub advance: u8,
}
const fn metrics() -> [[u8; 3]; 95] {
    let mut out = [[0; 3]; 95];
    let mut i = 0;
    while i < 95 {
        let mut bits = 0;
        let mut row = 0;
        while row < 16 {
            bits |= FONT[(i + 32) * 16 + row];
            row += 1;
        }
        let mut left = 0;
        while left < 8 && bits & (0x80 >> left) == 0 {
            left += 1;
        }
        let mut right = 8;
        while right > left && bits & (0x80 >> (right - 1)) == 0 {
            right -= 1;
        }
        out[i] = if bits == 0 {
            [0, 0, 4]
        } else {
            [left, right - left, right - left + 1]
        };
        // Tabular digits avoid jitter in counters, sizes and progress labels.
        if i >= 16 && i <= 25 {
            out[i][2] = 7;
        }
        i += 1;
    }
    out
}
const METRICS: [[u8; 3]; 95] = metrics();
pub fn glyph(byte: u8, font: Font) -> Glyph {
    let byte = if (32..=126).contains(&byte) {
        byte
    } else {
        b'?'
    };
    if font == Font::Mono {
        return Glyph {
            byte,
            left: 0,
            width: 8,
            advance: 8,
        };
    }
    let m = METRICS[(byte - 32) as usize];
    Glyph {
        byte,
        left: m[0],
        width: m[1],
        advance: m[2],
    }
}
pub fn width(bytes: &[u8], font: Font) -> u32 {
    let (mut line, mut maximum) = (0u32, 0u32);
    for &b in bytes {
        if b == b'\n' {
            maximum = maximum.max(line);
            line = 0;
        } else {
            line = line.saturating_add(glyph(b, font).advance as u32);
        }
    }
    maximum.max(line)
}
pub fn fit(bytes: &[u8], limit: u32, font: Font) -> usize {
    let mut used = 0u32;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            return i;
        }
        let next = used.saturating_add(glyph(b, font).advance as u32);
        if next > limit {
            return i;
        }
        used = next;
    }
    bytes.len()
}
/// A fitting prefix and zero to three trailing dots. Never exceeds the pixel budget.
pub fn ellipsis(bytes: &[u8], limit: u32, font: Font) -> (usize, usize) {
    let n = fit(bytes, limit, font);
    if n == bytes.len() {
        return (n, 0);
    }
    let dot = glyph(b'.', font).advance as u32;
    let dots = (limit / dot).min(3) as usize;
    (fit(bytes, limit - dots as u32 * dot, font), dots)
}
pub fn hit_test(bytes: &[u8], x: u32, font: Font) -> usize {
    let mut at = 0u32;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            return i;
        }
        let advance = glyph(b, font).advance as u32;
        if x < at.saturating_add(advance.div_ceil(2)) {
            return i;
        }
        at = at.saturating_add(advance);
    }
    bytes.len()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub start: usize,
    pub end: usize,
    pub width: u32,
}
pub struct Lines<'a> {
    bytes: &'a [u8],
    limit: u32,
    font: Font,
    position: usize,
    trailing: bool,
}
impl<'a> Lines<'a> {
    pub fn new(bytes: &'a [u8], limit: u32, font: Font) -> Self {
        Self {
            bytes,
            limit,
            font,
            position: 0,
            trailing: false,
        }
    }
}
impl Iterator for Lines<'_> {
    type Item = Line;
    fn next(&mut self) -> Option<Line> {
        if self.position >= self.bytes.len() {
            if self.trailing {
                self.trailing = false;
                return Some(Line {
                    start: self.position,
                    end: self.position,
                    width: 0,
                });
            }
            return None;
        }
        let start = self.position;
        let mut at = start;
        let mut used = 0u32;
        let mut space = None;
        while at < self.bytes.len() {
            let b = self.bytes[at];
            if b == b'\n' {
                self.position = at + 1;
                self.trailing = self.position == self.bytes.len();
                return Some(Line {
                    start,
                    end: at,
                    width: used,
                });
            }
            let next = used.saturating_add(glyph(b, self.font).advance as u32);
            if next > self.limit {
                if b == b' ' {
                    self.position = at + 1;
                    while self.bytes.get(self.position) == Some(&b' ') {
                        self.position += 1;
                    }
                    return Some(Line {
                        start,
                        end: at,
                        width: used,
                    });
                }
                if let Some((end, w)) = space.filter(|(end, _)| *end > start) {
                    self.position = end;
                    while self.bytes.get(self.position) == Some(&b' ') {
                        self.position += 1;
                    }
                    return Some(Line {
                        start,
                        end,
                        width: w,
                    });
                }
                // Even a zero-width box consumes a byte; long words always make progress.
                self.position = if at == start { at + 1 } else { at };
                return Some(Line {
                    start,
                    end: self.position,
                    width: if at == start { next } else { used },
                });
            }
            if b == b' ' && (at == start || self.bytes[at - 1] != b' ') {
                space = Some((at, used));
            }
            used = next;
            at += 1;
        }
        self.position = at;
        Some(Line {
            start,
            end: at,
            width: used,
        })
    }
}
pub fn height(bytes: &[u8], width: u32, font: Font) -> u32 {
    (Lines::new(bytes, width, font).count() as u32).saturating_mul(LINE_HEIGHT)
}
