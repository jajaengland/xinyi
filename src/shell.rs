// The physical console and pseudo-terminals share this line editor and dispatcher.
use crate::{keyboard, vga};
pub struct Shell {
    pub input: [u8; 128],
    pub length: usize,
    cursor: usize,
    history: [[u8; 128]; 8],
    lengths: [usize; 8],
    count: usize,
    at: usize,
    column: usize,
    row: usize,
    scroll: usize,
}
impl Shell {
    pub const fn new() -> Self {
        Self {
            input: [0; 128],
            length: 0,
            cursor: 0,
            history: [[0; 128]; 8],
            lengths: [0; 8],
            count: 0,
            at: 0,
            column: 0,
            row: 0,
            scroll: 0,
        }
    }
    pub fn prompt(&mut self) {
        crate::print_prompt();
        let (x, y) = vga::cursor_position();
        self.column = x;
        self.row = y;
        self.scroll = vga::scroll_count();
    }
    fn redraw(&mut self, old: usize) {
        crate::redraw_input(
            &self.input,
            self.length,
            self.cursor,
            old,
            self.column,
            &mut self.row,
            &mut self.scroll,
        );
    }
    pub fn cancel(&mut self) {
        self.length = 0;
        self.cursor = 0;
        self.at = self.count;
        crate::println!("^C");
        self.prompt();
    }
    pub fn key(&mut self, event: u32) -> Option<([u8; 128], usize)> {
        let code = event as u16;
        let ctrl = event & (1 << 17) != 0;
        if ctrl {
            match code {
                67 | 99 => {
                    self.cancel();
                    return None;
                }
                65 | 97 => {
                    self.cursor = 0;
                    self.redraw(self.length);
                    return None;
                }
                69 | 101 => {
                    self.cursor = self.length;
                    self.redraw(self.length);
                    return None;
                }
                _ => (),
            }
        }
        match code {
            10 => {
                crate::sync_prompt_row(&mut self.row, &mut self.scroll);
                vga::move_cursor(
                    (self.column + self.length) % 80,
                    self.row + (self.column + self.length) / 80,
                );
                crate::println!();
                let line = self.input;
                let length = self.length;
                if length != 0
                    && (self.count == 0
                        || self.lengths[self.count - 1] != length
                        || self.history[self.count - 1][..length] != line[..length])
                {
                    if self.count == 8 {
                        self.history.copy_within(1..8, 0);
                        self.lengths.copy_within(1..8, 0);
                        self.count -= 1;
                    }
                    self.history[self.count] = line;
                    self.lengths[self.count] = length;
                    self.count += 1;
                }
                self.length = 0;
                self.cursor = 0;
                self.input.fill(0);
                self.at = self.count;
                return Some((line, length));
            }
            8 => {
                if self.cursor != 0 {
                    let old = self.length;
                    self.input
                        .copy_within(self.cursor..self.length, self.cursor - 1);
                    self.cursor -= 1;
                    self.length -= 1;
                    self.redraw(old);
                }
            }
            0x106 => {
                if self.cursor < self.length {
                    let old = self.length;
                    self.input
                        .copy_within(self.cursor + 1..self.length, self.cursor);
                    self.length -= 1;
                    self.redraw(old);
                }
            }
            0x102 => {
                self.cursor = self.cursor.saturating_sub(1);
                self.redraw(self.length);
            }
            0x103 => {
                self.cursor = (self.cursor + 1).min(self.length);
                self.redraw(self.length);
            }
            0x104 => {
                self.cursor = 0;
                self.redraw(self.length);
            }
            0x105 => {
                self.cursor = self.length;
                self.redraw(self.length);
            }
            0x100 | 0x101 => {
                let old = self.length;
                if code == 0x100 {
                    self.at = self.at.saturating_sub(1);
                } else {
                    self.at = (self.at + 1).min(self.count);
                }
                self.length = if self.at < self.count {
                    self.lengths[self.at]
                } else {
                    0
                };
                if self.length != 0 {
                    self.input = self.history[self.at];
                }
                self.cursor = self.length;
                self.redraw(old);
            }
            9 => {
                if self.cursor == self.length {
                    let current = core::str::from_utf8(&self.input[..self.length]).unwrap_or("");
                    let mut matches = crate::COMMANDS
                        .iter()
                        .map(|c| c.0)
                        .filter(|c| c.starts_with(current));
                    if let Some(name) = matches.next() {
                        if matches.next().is_none() {
                            let old = self.length;
                            self.input[..name.len()].copy_from_slice(name.as_bytes());
                            self.length = name.len();
                            self.cursor = self.length;
                            self.redraw(old);
                        }
                    }
                }
            }
            32..=126 if event & ((1 << 17) | (1 << 18)) == 0 => {
                if self.length < 128 {
                    let old = self.length;
                    self.input
                        .copy_within(self.cursor..self.length, self.cursor + 1);
                    self.input[self.cursor] = code as u8;
                    self.cursor += 1;
                    self.length += 1;
                    self.redraw(old);
                }
            }
            _ => (),
        }
        None
    }
    pub fn physical() -> ! {
        let mut shell = Self::new();
        shell.prompt();
        loop {
            if let Some(event) = keyboard::poll_event() {
                if let Some((line, n)) = shell.key(event) {
                    crate::run_command(core::str::from_utf8(&line[..n]).unwrap_or(""));
                    crate::println!();
                    shell.prompt();
                }
            } else {
                crate::idle::wait_for_input();
            }
        }
    }
}
