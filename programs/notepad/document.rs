//! Heap-backed ASCII document with bounded history and indexed visual rows.
extern crate alloc;
use alloc::vec::Vec;
pub const LIMIT: usize = 65536;
const HISTORY: usize = 4;
struct Snapshot {
    data: Vec<u8>,
    cursor: usize,
}
pub struct Document {
    pub data: Vec<u8>,
    pub length: usize,
    pub cursor: usize,
    width: usize,
    saved: Vec<u8>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    rows: Vec<u32>,
}
fn copy(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len()).ok()?;
    out.extend_from_slice(bytes);
    Some(out)
}
pub fn normalize(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    out.try_reserve_exact(input.len().saturating_mul(4).min(LIMIT))
        .ok()?;
    let mut cr = false;
    for &b in input {
        if b == 10 && cr {
            cr = false;
            continue;
        }
        cr = b == 13;
        let (value, count) = match b {
            10 | 13 => (10, 1),
            9 => (32, 4),
            32..=126 => (b, 1),
            _ => return None,
        };
        if out.len() + count > LIMIT {
            return None;
        }
        for _ in 0..count {
            out.push(value);
        }
    }
    Some(out)
}
impl Document {
    pub fn new() -> Self {
        let mut d = Self {
            data: Vec::new(),
            length: 0,
            cursor: 0,
            width: 78,
            saved: Vec::new(),
            undo: Vec::with_capacity(HISTORY),
            redo: Vec::with_capacity(HISTORY),
            rows: Vec::with_capacity(LIMIT + 1),
        };
        d.index();
        d
    }
    fn index(&mut self) {
        self.rows.clear();
        self.rows.push(0);
        let mut c = 0;
        for (i, &b) in self.data.iter().enumerate() {
            if b == 10 {
                c = 0;
                self.rows.push((i + 1) as u32);
            } else {
                c += 1;
                if c == self.width {
                    c = 0;
                    self.rows.push((i + 1) as u32);
                }
            }
        }
        self.length = self.data.len();
    }
    pub fn set_width(&mut self, width: usize) {
        let width = width.clamp(1, 96);
        if self.width != width {
            self.width = width;
            self.index();
        }
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn line_count(&self) -> usize {
        self.rows.len()
    }
    pub fn clear(&mut self) {
        self.data.clear();
        self.saved.clear();
        self.undo.clear();
        self.redo.clear();
        self.cursor = 0;
        self.index();
    }
    pub fn dirty(&self) -> bool {
        self.data != self.saved
    }
    pub fn mark_saved(&mut self) -> bool {
        if let Some(saved) = copy(&self.data) {
            self.saved = saved;
            true
        } else {
            false
        }
    }
    pub fn load(&mut self, input: &[u8]) -> bool {
        let Some(data) = normalize(input) else {
            return false;
        };
        let Some(saved) = copy(&data) else {
            return false;
        };
        self.data = data;
        self.saved = saved;
        self.undo.clear();
        self.redo.clear();
        self.cursor = 0;
        self.index();
        true
    }
    fn checkpoint(&mut self) -> bool {
        let Some(data) = copy(&self.data) else {
            return false;
        };
        if self.undo.len() == HISTORY {
            self.undo.remove(0);
        }
        self.undo.push(Snapshot {
            data,
            cursor: self.cursor,
        });
        self.redo.clear();
        true
    }
    pub fn replace(&mut self, a: usize, b: usize, text: &[u8]) -> bool {
        if a > b || b > self.length || text.len() > LIMIT - (self.length - (b - a)) {
            return false;
        }
        if a == b && text.is_empty() {
            return true;
        }
        let n = self.length - (b - a) + text.len();
        if n > self.length && self.data.try_reserve_exact(n - self.length).is_err() {
            return false;
        }
        if !self.checkpoint() {
            return false;
        }
        if n > self.data.len() {
            self.data.resize(n, 0);
        }
        self.data.copy_within(b..self.length, a + text.len());
        self.data[a..a + text.len()].copy_from_slice(text);
        self.data.truncate(n);
        self.cursor = a + text.len();
        self.index();
        true
    }
    pub fn delete(&mut self, back: bool) {
        if back && self.cursor > 0 {
            self.replace(self.cursor - 1, self.cursor, b"");
        } else if !back && self.cursor < self.length {
            self.replace(self.cursor, self.cursor + 1, b"");
        }
    }
    pub fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            let current = core::mem::replace(&mut self.data, s.data);
            self.redo.push(Snapshot {
                data: current,
                cursor: self.cursor,
            });
            self.cursor = s.cursor;
            self.index();
        }
    }
    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            let current = core::mem::replace(&mut self.data, s.data);
            self.undo.push(Snapshot {
                data: current,
                cursor: self.cursor,
            });
            self.cursor = s.cursor;
            self.index();
        }
    }
    pub fn position(&self, index: usize) -> (usize, usize) {
        let i = index.min(self.length);
        let row = self
            .rows
            .partition_point(|&n| n as usize <= i)
            .saturating_sub(1);
        (row, i - self.rows[row] as usize)
    }
    pub fn offset(&self, row: usize, column: usize) -> usize {
        let Some(&start) = self.rows.get(row) else {
            return self.length;
        };
        let start = start as usize;
        let mut end = self
            .rows
            .get(row + 1)
            .copied()
            .map_or(self.length, |n| n as usize);
        if end > start && self.data[end - 1] == 10 {
            end -= 1;
        }
        (start + column).min(end)
    }
    pub fn vertical(&mut self, delta: i32) {
        let (row, col) = self.position(self.cursor);
        self.cursor = self.offset((row as i32 + delta).max(0) as usize, col);
    }
    pub fn screen_row_n<const N: usize>(&self, row: usize) -> [u8; N] {
        let mut out = [32; N];
        if let Some(&start) = self.rows.get(row) {
            let start = start as usize;
            let end = self
                .rows
                .get(row + 1)
                .copied()
                .map_or(self.length, |n| n as usize);
            for (i, &b) in self.data[start..end]
                .iter()
                .take(self.width.min(N))
                .enumerate()
            {
                if b == 10 {
                    break;
                }
                out[i] = b;
            }
        }
        out
    }
    pub fn selection_span(&self, row: usize, a: usize, b: usize) -> (u8, u8) {
        let Some(&start) = self.rows.get(row) else {
            return (0, 0);
        };
        let start = start as usize;
        let end = self
            .rows
            .get(row + 1)
            .copied()
            .map_or(self.length, |n| n as usize);
        let left = a.max(start);
        let right = b.min(end);
        if left >= right {
            (0, 0)
        } else {
            (
                (left - start).min(self.width) as u8,
                (right - start).min(self.width) as u8,
            )
        }
    }
}
