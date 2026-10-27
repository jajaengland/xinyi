#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: u8,
    pub color: u8,
}
const BLANK: Cell = Cell {
    ch: b' ',
    color: 0x0a,
};
pub struct Console {
    pub cells: [Cell; 2000],
    history: [[Cell; 80]; 64],
    pub history_len: usize,
    head: usize,
    pub x: usize,
    pub y: usize,
    pub color: u8,
    pub visible: bool,
    pub scrolls: usize,
    pub generation: u32,
    escape: u8,
    params: [u16; 8],
    at: usize,
    saved: (usize, usize),
}
impl Console {
    pub const fn new() -> Self {
        Self {
            cells: [BLANK; 2000],
            history: [[BLANK; 80]; 64],
            history_len: 0,
            head: 0,
            x: 0,
            y: 0,
            color: 0x0a,
            visible: true,
            scrolls: 0,
            generation: 1,
            escape: 0,
            params: [0; 8],
            at: 0,
            saved: (0, 0),
        }
    }
    pub fn touch(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn clear(&mut self) {
        self.cells.fill(Cell {
            ch: b' ',
            color: self.color,
        });
        self.x = 0;
        self.y = 0;
        self.scrolls = 0;
        self.touch();
    }
    fn newline(&mut self) {
        self.x = 0;
        if self.y < 24 {
            self.y += 1;
        } else {
            self.history[self.head].copy_from_slice(&self.cells[..80]);
            self.head = (self.head + 1) % 64;
            self.history_len = (self.history_len + 1).min(64);
            self.cells.copy_within(80..2000, 0);
            self.cells[1920..].fill(Cell {
                ch: b' ',
                color: self.color,
            });
            self.scrolls += 1;
        }
        self.touch();
    }
    pub fn position(&mut self, x: usize, y: usize) {
        self.x = x.min(79);
        self.y = y.min(24);
        self.touch();
    }
    pub fn draw(&mut self, x: usize, y: usize, color: u8, text: &[u8]) {
        for (i, &ch) in text.iter().enumerate() {
            self.cells[y * 80 + x + i] = Cell {
                ch: if ch < 32 || ch == 127 { b' ' } else { ch },
                color,
            };
        }
        self.touch();
    }
    pub fn write(&mut self, text: &[u8]) {
        for &byte in text {
            self.byte(byte);
        }
    }
    fn byte(&mut self, b: u8) {
        if self.escape == 1 {
            self.escape = 0;
            match b {
                b'[' => {
                    self.escape = 2;
                    self.params = [0; 8];
                    self.at = 0;
                }
                b'7' => self.saved = (self.x, self.y),
                b'8' => self.position(self.saved.0, self.saved.1),
                _ => (),
            }
            return;
        }
        if self.escape == 2 {
            if b.is_ascii_digit() {
                self.params[self.at] = self.params[self.at]
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u16)
                    .min(9999);
                return;
            }
            if b == b';' {
                self.at = (self.at + 1).min(7);
                return;
            }
            self.escape = 0;
            let p = self.params[0] as usize;
            let n = p.max(1);
            match b {
                b'H' | b'f' => self.position((self.params[1] as usize).max(1) - 1, n - 1),
                b'A' => self.position(self.x, self.y.saturating_sub(n)),
                b'B' => self.position(self.x, self.y + n),
                b'C' => self.position(self.x + n, self.y),
                b'D' => self.position(self.x.saturating_sub(n), self.y),
                b'G' => self.position(n - 1, self.y),
                b'J' => {
                    let cursor = self.y * 80 + self.x;
                    let (a, b) = match p {
                        1 => (0, cursor + 1),
                        2 => (0, 2000),
                        _ => (cursor, 2000),
                    };
                    self.cells[a..b].fill(Cell {
                        ch: b' ',
                        color: self.color,
                    });
                    self.touch();
                }
                b'K' => {
                    let (a, b) = match p {
                        1 => (0, self.x + 1),
                        2 => (0, 80),
                        _ => (self.x, 80),
                    };
                    self.cells[self.y * 80 + a..self.y * 80 + b].fill(Cell {
                        ch: b' ',
                        color: self.color,
                    });
                    self.touch();
                }
                b'm' => {
                    for i in 0..=self.at {
                        let p = self.params[i];
                        let map = [0, 4, 2, 6, 1, 5, 3, 7];
                        match p {
                            0 => self.color = 7,
                            1 => self.color |= 8,
                            22 => self.color &= !8,
                            30..=37 => self.color = (self.color & 0xf0) | map[(p - 30) as usize],
                            40..=47 => {
                                self.color = (self.color & 15) | (map[(p - 40) as usize] << 4)
                            }
                            90..=97 => {
                                self.color = (self.color & 0xf0) | map[(p - 90) as usize] | 8
                            }
                            100..=107 => {
                                self.color =
                                    (self.color & 15) | ((map[(p - 100) as usize] | 8) << 4)
                            }
                            _ => (),
                        }
                    }
                }
                b's' => self.saved = (self.x, self.y),
                b'u' => self.position(self.saved.0, self.saved.1),
                _ => (),
            }
            return;
        }
        match b {
            27 => self.escape = 1,
            b'\n' => self.newline(),
            b'\r' => {
                self.x = 0;
                self.touch();
            }
            8 => {
                if self.x != 0 {
                    self.x -= 1;
                }
                self.touch();
            }
            9 => {
                for _ in 0..4 {
                    self.byte(b' ');
                }
            }
            _ => {
                self.cells[self.y * 80 + self.x] = Cell {
                    ch: b,
                    color: self.color,
                };
                self.x += 1;
                if self.x == 80 {
                    self.newline();
                }
                self.touch();
            }
        }
    }
    pub fn snapshot(&self, scroll: usize, out: &mut [Cell; 2000]) {
        let scroll = scroll.min(self.history_len);
        let start = self.history_len - scroll;
        for row in 0..25 {
            let virtual_row = start + row;
            if virtual_row < self.history_len {
                let i = (self.head + 64 - self.history_len + virtual_row) % 64;
                out[row * 80..row * 80 + 80].copy_from_slice(&self.history[i]);
            } else {
                let r = virtual_row - self.history_len;
                out[row * 80..row * 80 + 80].copy_from_slice(&self.cells[r * 80..r * 80 + 80]);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ansi_and_console_calls_share_cells() {
        let mut c = Console::new();
        c.write(b"\x1b[2J\x1b[2;3H\x1b[31mX");
        assert_eq!(c.cells[82].ch, b'X');
        assert_eq!(c.cells[82].color & 15, 4);
        c.position(0, 0);
        c.draw(0, 0, 0x1f, b"hello");
        assert_eq!(c.cells[0].ch, b'h');
    }
    #[test]
    fn bounded_history_keeps_latest_screen() {
        let mut c = Console::new();
        for _ in 0..100 {
            c.write(b"line\n");
        }
        assert_eq!(c.history_len, 64);
        let mut out = [BLANK; 2000];
        c.snapshot(64, &mut out);
        assert_eq!(out[0].ch, b'l');
        assert!(c.y < 25 && c.x < 80);
    }
}
