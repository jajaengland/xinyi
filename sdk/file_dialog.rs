//! Reusable file picker. Browsing never changes the application's working directory.
use super::{
    directory::{self, Listing},
    gui, key,
    widgets::{Button, Rect, Scrollbar, TextInput},
    window::{self, Draw},
};
pub enum Result {
    Pending,
    Cancel,
    Accept {
        path: [u8; 105],
        len: usize,
        volume: u32,
    },
}
pub struct Dialog {
    pub save: bool,
    path: [u8; 105],
    len: usize,
    volume: u32,
    list: Listing,
    offset: u16,
    selected: Option<usize>,
    pub input: TextInput<105>,
    bar: Scrollbar,
    buttons: [Button; 3],
    focus: u8,
    last: Option<usize>,
    clicked: u32,
    mouse: u32,
    message: &'static [u8],
}
const FIELD: Rect = Rect::new(120, 288, 440, 28);
impl Dialog {
    pub fn new(save: bool) -> Self {
        let mut path = [0; 105];
        let n = super::working_directory(&mut path);
        let len = if n > 0 {
            n as usize
        } else {
            path[0] = b'/';
            1
        };
        let mut d = Self {
            save,
            path,
            len,
            volume: directory::volume(),
            list: Listing::new(),
            offset: 0,
            selected: None,
            input: TextInput::new(),
            bar: Scrollbar::new(Rect::new(560, 108, 16, 160)),
            buttons: [
                Button::new(Rect::new(64, 76, 48, 24)),
                Button::new(Rect::new(352, 324, 96, 24)),
                Button::new(Rect::new(464, 324, 96, 24)),
            ],
            focus: 0,
            last: None,
            clicked: 0,
            mouse: 0,
            message: b"Enter a name or choose a file. Tab changes focus.",
        };
        d.refresh();
        d
    }
    fn refresh(&mut self) {
        if self.volume != directory::volume() {
            self.list = Listing::new();
            self.offset = 0;
            self.bar.range(0, 10);
            self.bar.value = 0;
            self.message = b"Drive changed. Cancel and reopen this dialog.";
            return;
        }
        let n = directory::list(&self.path[..self.len], self.offset, &mut self.list);
        if n < 0 {
            self.list = Listing::new();
            self.message = b"Cannot read this folder.";
        }
        let max = self.list.total.saturating_sub(10);
        if self.offset as u32 > max {
            self.offset = max as u16;
            self.refresh();
            return;
        }
        self.bar.range(self.list.total.saturating_sub(10), 10);
        self.bar.value = self.offset as u32;
        self.selected = None;
        self.last = None;
    }
    fn change_folder(&mut self, name: &[u8]) {
        let (mut p, mut n) = (self.path, self.len);
        if name == b".." {
            n = p[..n].iter().rposition(|&b| b == b'/').unwrap_or(0).max(1);
        } else {
            if n > 1 {
                if n == 105 {
                    return;
                }
                p[n] = b'/';
                n += 1;
            }
            if name.len() > 105 - n {
                self.message = b"Path is too deep.";
                return;
            }
            p[n..n + name.len()].copy_from_slice(name);
            n += name.len();
        }
        let mut listing = Listing::new();
        if directory::list(&p[..n], 0, &mut listing) < 0 {
            self.message = b"Cannot open this folder.";
            return;
        }
        self.path = p;
        self.len = n;
        self.offset = 0;
        if !self.save {
            self.input.set(b"");
        }
        self.refresh();
    }
    fn choose(&mut self, row: usize) {
        let e = self.list.entries[row];
        self.selected = Some(row);
        if e.flags & 1 == 0 {
            self.input.set(&e.name[..e.length as usize]);
        }
    }
    fn activate(&mut self) -> Result {
        if self.volume != directory::volume() || (self.volume as i32) < 0 {
            self.message = b"Drive changed or unavailable. Cancel and reopen.";
            return Result::Pending;
        }
        if self.input.len == 0 {
            if let Some(row) = self.selected {
                let e = self.list.entries[row];
                if e.flags & 1 != 0 {
                    self.change_folder(&e.name[..e.length as usize]);
                    return Result::Pending;
                }
            }
            self.message = b"Enter a filename first.";
            return Result::Pending;
        }
        let mut path = [0; 105];
        let text = &self.input.bytes[..self.input.len];
        let prefix = if matches!(text[0], b'/' | b'\\') {
            0
        } else {
            self.len + usize::from(self.len > 1)
        };
        if prefix + text.len() > 105 {
            self.message = b"Path is too long.";
            return Result::Pending;
        }
        if prefix != 0 {
            path[..self.len].copy_from_slice(&self.path[..self.len]);
            if self.len > 1 {
                path[self.len] = b'/';
            }
        }
        path[prefix..prefix + text.len()].copy_from_slice(text);
        let len = prefix + text.len();
        let mut listing = Listing::new();
        if directory::list(&path[..len], 0, &mut listing) == 0 {
            self.path = path;
            self.len = len;
            self.offset = 0;
            self.input.set(b"");
            self.refresh();
            return Result::Pending;
        }
        Result::Accept {
            path,
            len,
            volume: self.volume,
        }
    }
    pub fn reset(&mut self, buttons: u32) {
        self.mouse = buttons;
        self.bar.reset(buttons);
        for b in &mut self.buttons {
            b.reset(buttons);
        }
        self.last = None;
    }
    pub fn event(&mut self, e: window::Event) -> Result {
        match e.kind {
            window::RESET => self.reset(e.buttons),
            window::KEY => {
                let code = e.code as u16;
                match code {
                    27 => return Result::Cancel,
                    9 => {
                        self.focus = if e.code & key::SHIFT != 0 {
                            (self.focus + 4) % 5
                        } else {
                            (self.focus + 1) % 5
                        };
                    }
                    10 => {
                        if self.focus == 3 {
                            return Result::Cancel;
                        }
                        if self.focus == 4 {
                            self.change_folder(b"..");
                        } else if self.focus == 1 {
                            if let Some(row) = self.selected {
                                let entry = self.list.entries[row];
                                if entry.flags & 1 != 0 {
                                    self.change_folder(&entry.name[..entry.length as usize]);
                                } else {
                                    return self.activate();
                                }
                            }
                        } else {
                            return self.activate();
                        }
                    }
                    key::UP if e.code & key::ALT != 0 => self.change_folder(b".."),
                    key::PAGE_UP | key::PAGE_DOWN => {
                        self.offset = if code == key::PAGE_UP {
                            self.offset.saturating_sub(10)
                        } else {
                            (self.offset as u32 + 10).min(self.bar.max) as u16
                        };
                        self.refresh();
                    }
                    key::UP | key::DOWN if self.focus == 1 => {
                        let total = self.list.count.min(10) as usize;
                        if total > 0 {
                            let i = self.selected.unwrap_or(0);
                            self.choose(if code == key::UP {
                                i.saturating_sub(1)
                            } else {
                                (i + 1).min(total - 1)
                            });
                        }
                    }
                    _ if self.focus == 0 => {
                        gui::edit_key(&mut self.input, e.code);
                    }
                    _ => (),
                }
            }
            window::POINTER => {
                let down = e.buttons & 1 != 0 && self.mouse & 1 == 0;
                let dragging = self.bar.dragging();
                if self.bar.pointer(e.x, e.y, e.buttons) {
                    self.offset = self.bar.value as u16;
                    self.refresh();
                }
                let mut clicked = None;
                for (i, b) in self.buttons.iter_mut().enumerate() {
                    if b.pointer(e.x, e.y, e.buttons) {
                        clicked = Some(i);
                    }
                }
                if let Some(i) = clicked {
                    match i {
                        0 => self.change_folder(b".."),
                        1 => return self.activate(),
                        _ => return Result::Cancel,
                    }
                }
                if down && !dragging {
                    if FIELD.contains(e.x, e.y) {
                        self.focus = 0;
                        let cols = ((FIELD.w - 12) / 8) as usize;
                        let start = self.input.cursor.saturating_sub(cols - 1);
                        self.input.cursor =
                            (start + ((e.x - FIELD.x - 6).max(0) / 8) as usize).min(self.input.len);
                        self.input.anchor = None;
                    } else if Rect::new(64, 108, 496, 160).contains(e.x, e.y) {
                        self.focus = 1;
                        let row = ((e.y - 108) / 16) as usize;
                        if row < (self.list.count as usize).min(10) {
                            self.choose(row);
                            let now = super::ticks();
                            if self.last == Some(row) && now.wrapping_sub(self.clicked) < 30 {
                                let entry = self.list.entries[row];
                                self.last = None;
                                if entry.flags & 1 != 0 {
                                    self.change_folder(&entry.name[..entry.length as usize]);
                                } else {
                                    return self.activate();
                                }
                            } else {
                                self.last = Some(row);
                                self.clicked = now;
                            }
                        } else {
                            self.selected = None;
                            self.last = None;
                        }
                    } else {
                        for (i, b) in self.buttons.iter().enumerate() {
                            if b.rect.contains(e.x, e.y) {
                                self.focus = [4, 2, 3][i];
                            }
                        }
                    }
                }
                self.mouse = e.buttons;
            }
            _ => (),
        }
        Result::Pending
    }
    pub fn paint(&self, w: u32) {
        gui::dialog(
            w,
            if self.save { b"Save as" } else { b"Open file" },
            Rect::new(48, 40, 544, 316),
        );
        gui::button(w, &self.buttons[0], b"Up", self.focus == 4);
        let start = self.len.saturating_sub(54);
        window::draw(
            w,
            &[
                Draw::text(120, 80, 0, &self.path[start..self.len]),
                Draw::rect(64, 108, 496, 160, 0xffffff),
            ],
        );
        for (i, e) in self.list.entries[..(self.list.count as usize).min(10)]
            .iter()
            .enumerate()
        {
            let y = 108 + i as u32 * 16;
            let selected = self.selected == Some(i);
            if selected {
                window::draw(w, &[Draw::rect(64, y, 496, 16, 0x153e8e)]);
            }
            window::draw(
                w,
                &[
                    Draw::text(
                        72,
                        y,
                        if selected { 0xffffff } else { 0 },
                        &e.name[..e.length as usize],
                    ),
                    Draw::text(
                        480,
                        y,
                        if selected { 0xffffff } else { 0 },
                        if e.flags & 1 != 0 { b"Folder" } else { b"File" },
                    ),
                ],
            );
        }
        gui::scrollbar(w, &self.bar);
        window::draw(
            w,
            &[
                Draw::text(64, 272, 0, &self.message[..self.message.len().min(64)]),
                Draw::text(64, 294, 0, b"Name:"),
            ],
        );
        gui::text_input(w, FIELD, &self.input, self.focus == 0);
        gui::button(
            w,
            &self.buttons[1],
            if self.save { b"Save" } else { b"Open" },
            self.focus == 2,
        );
        gui::button(w, &self.buttons[2], b"Cancel", self.focus == 3);
    }
}
