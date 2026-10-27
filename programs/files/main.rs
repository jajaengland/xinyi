#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::{
    directory::{self, Listing},
    key,
    window::{self, Draw},
};
struct Files {
    list: Listing,
    offset: u16,
    selected: usize,
    path: [u8; 105],
    length: usize,
    status: &'static [u8],
    mode: u8,
    input: [u8; 12],
    used: usize,
    last: usize,
    clicked: u32,
}
fn error(n: i32) -> &'static [u8] {
    match n {
        -2 => b"File no longer exists. Refresh the folder.",
        -13 => b"Read-only file.",
        -17 => b"That name already exists.",
        -20 => b"Not a folder.",
        -21 => b"That is a folder.",
        -22 => b"Use an ASCII 8.3 name, for example NOTE.TXT.",
        -24 => b"Close another window before opening this file.",
        -27 => b"File is too large for Notepad.",
        -28 => b"The disk is full.",
        -39 => b"Folder is not empty. Remove its contents first.",
        -16 => b"Folder is in use, or another launch is pending.",
        _ => b"Disk operation failed.",
    }
}
impl Files {
    fn refresh(&mut self) {
        self.last = usize::MAX;
        let n = directory::list(b".", self.offset, &mut self.list);
        if n < 0 {
            self.list = Listing::new();
            self.status = error(n);
        } else {
            if self.offset as u32 >= self.list.total && self.offset > 0 {
                self.offset = 0;
                self.refresh();
                return;
            }
            self.selected = self.selected.min(self.rows().saturating_sub(1));
        }
        let n = xinyi::working_directory(&mut self.path);
        self.length = n.max(0) as usize;
    }
    fn rows(&self) -> usize {
        (self.list.count as usize).min(18)
    }
    fn cd(&mut self, path: &[u8]) {
        let n = directory::cd(path);
        if n < 0 {
            self.status = error(n);
        } else {
            self.offset = 0;
            self.selected = 0;
            self.status = b"Folder opened.";
            self.refresh();
        }
    }
    fn launch(&mut self, prefix: &[u8], name: &[u8]) {
        let mut command = [0; 128];
        let n = prefix.len() + name.len();
        command[..prefix.len()].copy_from_slice(prefix);
        command[prefix.len()..n].copy_from_slice(name);
        let result = xinyi::launch(&command[..n]);
        self.status = if result < 0 {
            error(result)
        } else {
            b"Opened. Use the taskbar to return to Files; F5 refreshes."
        };
    }
    fn open(&mut self) {
        if self.selected >= self.rows() {
            return;
        }
        let e = self.list.entries[self.selected];
        let name = &e.name[..e.length as usize];
        if e.flags & 1 != 0 {
            self.cd(name);
        } else if name.ends_with(b".BIN") {
            self.launch(b"terminal run ", name);
        } else {
            self.launch(b"notepad ", name);
        }
    }
    fn action(&mut self, action: usize) -> bool {
        self.status = b"";
        match action {
            0 => self.cd(b".."),
            1 => self.open(),
            2 | 3 => {
                self.mode = if action == 2 { 1 } else { 2 };
                self.used = 0;
            }
            4 => {
                if self.rows() > 0 {
                    self.mode = 3;
                }
            }
            5 => self.refresh(),
            6 => {
                self.offset = self.offset.saturating_sub(18);
                self.selected = 0;
                self.refresh();
            }
            7 => {
                if self.offset as u32 + 18 < self.list.total {
                    self.offset += 18;
                    self.selected = 0;
                    self.refresh();
                }
            }
            8 => return true,
            _ => (),
        }
        false
    }
    fn key(&mut self, event: u32) -> bool {
        let code = event as u16;
        let ctrl = event & key::CTRL != 0;
        if self.mode != 0 {
            if code == 27 {
                self.mode = 0;
                self.status = b"Cancelled.";
                return false;
            }
            if self.mode == 3 {
                if matches!(code, 89 | 121) {
                    let e = self.list.entries[self.selected];
                    let n = directory::action(
                        &e.name[..e.length as usize],
                        if e.flags & 1 != 0 { 2 } else { 1 },
                    );
                    self.mode = 0;
                    self.status = if n < 0 { error(n) } else { b"Deleted." };
                    self.refresh();
                } else if matches!(code, 78 | 110) {
                    self.mode = 0;
                    self.status = b"Cancelled.";
                }
                return false;
            }
            match code {
                8 => self.used = self.used.saturating_sub(1),
                10 if self.used > 0 => {
                    let name = self.input;
                    let used = self.used;
                    let mode = self.mode;
                    self.mode = 0;
                    if mode == 2 {
                        let n = directory::action(&name[..used], 0);
                        self.status = if n < 0 { error(n) } else { b"Folder created." };
                        self.refresh();
                    } else {
                        self.launch(b"notepad ", &name[..used]);
                    }
                }
                32..=126 if !ctrl => {
                    if self.used < 12 {
                        self.input[self.used] = code as u8;
                        self.used += 1;
                    }
                }
                _ => (),
            }
            return false;
        }
        if ctrl && matches!(code, 81 | 113) {
            return true;
        }
        if ctrl && matches!(code, 78 | 110) {
            return self.action(2);
        }
        match code {
            key::UP => self.selected = self.selected.saturating_sub(1),
            key::DOWN => self.selected = (self.selected + 1).min(self.rows().saturating_sub(1)),
            key::HOME => self.selected = 0,
            key::END => self.selected = self.rows().saturating_sub(1),
            10 => self.open(),
            8 => self.cd(b".."),
            key::DELETE => {
                self.action(4);
            }
            key::PAGE_UP => {
                self.action(6);
            }
            key::PAGE_DOWN => {
                self.action(7);
            }
            key::F5 => self.refresh(),
            _ => (),
        }
        false
    }
    fn paint(&self, w: u32) {
        window::draw(w, &[Draw::rect(0, 0, 640, 400, 0xf4f4f4)]);
        let labels: [&[u8]; 9] = [
            b"Up", b"Open", b"New", b"Folder", b"Delete", b"Refresh", b"<", b">", b"Quit",
        ];
        for (i, label) in labels.iter().enumerate() {
            let x = i as u32 * 70;
            window::draw(
                w,
                &[
                    Draw::rect(x + 2, 2, 66, 26, 0xc4c4c4),
                    Draw::text(x + 6, 8, 0, label),
                ],
            );
        }
        let start = self.length.saturating_sub(78);
        window::draw(
            w,
            &[
                Draw::text(8, 40, 0x153e8e, &self.path[start..self.length]),
                Draw::text(8, 64, 0, b"Name                 Type       Bytes"),
            ],
        );
        for i in 0..self.rows() {
            let e = self.list.entries[i];
            let y = 80 + i as u32 * 16;
            let bg = if self.selected == i {
                0x153e8e
            } else {
                0xffffff
            };
            let fg = if self.selected == i { 0xffffff } else { 0 };
            let mut digits = [b' '; 10];
            let mut value = e.size;
            for digit in digits.iter_mut().rev() {
                *digit = b'0' + (value % 10) as u8;
                value /= 10;
            }
            window::draw(
                w,
                &[
                    Draw::rect(8, y, 624, 16, bg),
                    Draw::text(8, y, fg, &e.name[..e.length as usize]),
                    Draw::text(
                        176,
                        y,
                        fg,
                        if e.flags & 1 != 0 { b"Folder" } else { b"File" },
                    ),
                    Draw::text(264, y, fg, &digits),
                ],
            );
        }
        let text = match self.mode {
            1 => b"New file (8.3): " as &[u8],
            2 => b"New folder (8.3): ",
            3 => b"Delete selected item? Y confirms / N cancels",
            _ => b"Enter opens | Backspace up | F5 refresh | PgUp/PgDn pages",
        };
        window::draw(
            w,
            &[
                Draw::text(8, 368, 0, text),
                Draw::text(8, 384, 0, &self.status[..self.status.len().min(78)]),
            ],
        );
        if self.mode == 1 || self.mode == 2 {
            window::draw(
                w,
                &[Draw::text(160, 368, 0x153e8e, &self.input[..self.used])],
            );
        }
        window::present(w);
    }
}
fn main() -> u32 {
    let w = window::open(b"Files");
    if w < 0 {
        return 1;
    }
    let w = w as u32;
    let mut app = Files {
        list: Listing::new(),
        offset: 0,
        selected: 0,
        path: [0; 105],
        length: 0,
        status: b"Select a file or folder, then Open (or double-click).",
        mode: 0,
        input: [0; 12],
        used: 0,
        last: usize::MAX,
        clicked: 0,
    };
    app.refresh();
    app.paint(w);
    let mut buttons = 0;
    loop {
        let mut event = window::Event::default();
        let n = window::event(w, &mut event, true);
        if n < 0 {
            return 1;
        }
        if n == 0 {
            continue;
        }
        let mut repaint = false;
        match event.kind {
            window::CLOSE => return 0,
            window::KEY => {
                if app.key(event.code) {
                    return 0;
                }
                repaint = true;
            }
            window::RESET => buttons = event.buttons,
            window::POINTER => {
                if event.buttons & 1 != 0 && buttons & 1 == 0 {
                    if app.mode == 0 {
                        if event.y < 30 && event.y >= 0 && event.x >= 0 {
                            if app.action(event.x as usize / 70) {
                                return 0;
                            }
                        } else if (80..368).contains(&event.y) {
                            let row = (event.y - 80) as usize / 16;
                            if row < app.rows() {
                                app.selected = row;
                                let now = xinyi::ticks();
                                if app.last == row && now.wrapping_sub(app.clicked) < 30 {
                                    app.open();
                                    app.last = usize::MAX;
                                } else {
                                    app.last = row;
                                    app.clicked = now;
                                }
                            }
                        }
                    }
                    repaint = true;
                }
                buttons = event.buttons;
            }
            _ => (),
        }
        if repaint {
            app.paint(w);
        }
    }
}
