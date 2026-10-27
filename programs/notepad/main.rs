#![no_std]
#![no_main]
#[path = "../editor/model.rs"]
mod model;
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use model::Document;
use window::Draw;
use xinyi::{key, window};
#[derive(Clone, Copy)]
enum Action {
    Quit,
    New,
    Open,
}
#[derive(Clone, Copy)]
enum Mode {
    Edit,
    Name(bool),
    Confirm(Action),
    Overwrite,
}
struct Editor {
    doc: Document,
    name: [u8; 105],
    name_len: usize,
    prompt: [u8; 105],
    prompt_len: usize,
    mode: Mode,
    after_save: Option<Action>,
    top: usize,
    mouse_buttons: u32,
    capture: Option<usize>,
    hover: Option<usize>,
    message: &'static [u8],
    cache: [[u8; 80]; 22],
    caret: Option<usize>,
    modal: bool,
    anchor: Option<usize>,
    selecting: bool,
    marks: [(u8, u8); 22],
}
impl Editor {
    fn new() -> Self {
        Self {
            doc: Document::new(),
            name: [0; 105],
            name_len: 0,
            prompt: [0; 105],
            prompt_len: 0,
            mode: Mode::Edit,
            after_save: None,
            top: 0,
            mouse_buttons: 0,
            capture: None,
            hover: None,
            message: b"Ready - files up to 1024 bytes",
            cache: [[0; 80]; 22],
            caret: None,
            modal: false,
            anchor: None,
            selecting: false,
            marks: [(0, 0); 22],
        }
    }
    fn range(&self) -> Option<(usize, usize)> {
        self.anchor
            .filter(|&a| a != self.doc.cursor)
            .map(|a| (a.min(self.doc.cursor), a.max(self.doc.cursor)))
    }
    fn replace(&mut self, text: &[u8]) {
        let (a, b) = self.range().unwrap_or((self.doc.cursor, self.doc.cursor));
        if self.doc.replace(a, b, text) {
            self.anchor = None;
            self.selecting = false;
            self.message = b"Edited";
        } else {
            self.message = b"File full: maximum 1024 bytes; document retained";
        }
    }
    fn copy(&mut self, cut: bool) {
        let Some((a, b)) = self.range() else {
            self.message = b"Select text first (drag, Shift+arrows, or Ctrl+A)";
            return;
        };
        if xinyi::clipboard_set(&self.doc.data[a..b]) < 0 {
            self.message = b"Clipboard copy failed";
            return;
        }
        if cut {
            self.replace(b"");
            self.message = b"Cut to clipboard";
        } else {
            self.message = b"Copied to clipboard";
        }
    }
    fn paste(&mut self) {
        let mut raw = [0; 1024];
        let n = xinyi::clipboard_get(&mut raw);
        if n < 0 {
            self.message = b"Clipboard read failed";
            return;
        }
        if n == 0 {
            self.message = b"Clipboard is empty";
            return;
        }
        let mut text = [0; 1024];
        let Some(n) = model::normalize(&raw[..n as usize], &mut text) else {
            self.message = b"Clipboard text cannot fit in this document";
            return;
        };
        self.replace(&text[..n]);
    }
    fn action(&mut self, action: Action) -> bool {
        match action {
            Action::Quit => return true,
            Action::New => {
                self.doc = Document::new();
                self.anchor = None;
                self.selecting = false;
                self.name_len = 0;
                self.top = 0;
                self.message = b"New file";
                self.mode = Mode::Edit;
            }
            Action::Open => {
                self.mode = Mode::Name(false);
                self.prompt_len = 0;
                self.message = b"Open file: Enter accepts, Esc cancels";
            }
        }
        false
    }
    fn request(&mut self, action: Action) -> bool {
        if self.doc.dirty() {
            self.mode = Mode::Confirm(action);
            self.message = b"Unsaved changes: S save, D discard, C cancel";
            false
        } else {
            self.action(action)
        }
    }
    fn save(&mut self, name: [u8; 105], length: usize) -> bool {
        if length == 0 {
            self.mode = Mode::Name(true);
            self.prompt_len = 0;
            self.message = b"Save as: enter a filename (ASCII 8.3 paths)";
            return false;
        }
        let result = xinyi::save_file(&name[..length], &self.doc.data[..self.doc.length]);
        if result < 0 {
            self.message = match result {
                -13 => b"Save failed: file is read-only",
                -28 => b"Save failed: disk full",
                -19 => b"Save failed: no mounted drive",
                -22 => b"Save failed: invalid filename",
                _ => b"Save failed: disk error; document retained",
            };
            self.mode = Mode::Edit;
            self.after_save = None;
            return false;
        }
        self.name = name;
        self.name_len = length;
        self.doc.mark_saved();
        self.message = b"Saved to disk";
        self.mode = Mode::Edit;
        if let Some(action) = self.after_save.take() {
            self.action(action)
        } else {
            false
        }
    }
    fn open(&mut self, name: [u8; 105], length: usize) -> bool {
        let mut data = [0; 1024];
        let result = xinyi::read_file(&name[..length], &mut data);
        if result >= 0 {
            if !self.doc.load(&data[..result as usize]) {
                self.message = b"Cannot open: unsupported text or expanded size over 1024";
                return false;
            }
        } else if result == -2 {
            self.doc = Document::new();
            self.message = b"New file - save to create it";
        } else {
            self.message = if result == -27 {
                b"Cannot open: file exceeds 1024 bytes"
            } else {
                b"Cannot open file; current document retained"
            };
            return false;
        }
        self.name = name;
        self.name_len = length;
        self.top = 0;
        self.anchor = None;
        self.selecting = false;
        self.mode = Mode::Edit;
        if result >= 0 {
            self.message = b"Opened (CRLF normalized, tabs expanded)";
        }
        true
    }
    fn event(&mut self, event: u32) -> bool {
        let code = event as u16;
        match self.mode {
            Mode::Confirm(action) => match code {
                115 | 83 => {
                    self.after_save = Some(action);
                    return self.save(self.name, self.name_len);
                }
                100 | 68 => return self.action(action),
                99 | 67 | 27 => {
                    self.mode = Mode::Edit;
                    self.message = b"Cancelled";
                }
                _ => (),
            },
            Mode::Overwrite => match code {
                121 | 89 => return self.save(self.prompt, self.prompt_len),
                110 | 78 | 27 => {
                    self.mode = Mode::Edit;
                    self.after_save = None;
                    self.message = b"Save cancelled";
                }
                _ => (),
            },
            Mode::Name(save) => match code {
                27 => {
                    self.mode = Mode::Edit;
                    self.after_save = None;
                    self.message = b"Cancelled";
                }
                8 => self.prompt_len = self.prompt_len.saturating_sub(1),
                10 if self.prompt_len != 0 => {
                    if save {
                        let mut data = [0; 1024];
                        let exists = xinyi::read_file(&self.prompt[..self.prompt_len], &mut data);
                        if (exists >= 0 || exists == -27)
                            && (self.prompt_len != self.name_len
                                || self.prompt[..self.prompt_len] != self.name[..self.name_len])
                        {
                            self.mode = Mode::Overwrite;
                            self.message = b"Replace existing file? Y yes / N cancel";
                        } else if exists >= 0 || exists == -2 {
                            return self.save(self.prompt, self.prompt_len);
                        } else {
                            self.message = b"Cannot check destination; save cancelled";
                        }
                    } else {
                        self.open(self.prompt, self.prompt_len);
                        self.mode = Mode::Edit;
                    }
                }
                32..=126 if self.prompt_len < 105 && event & (key::CTRL | key::ALT) == 0 => {
                    self.prompt[self.prompt_len] = code as u8;
                    self.prompt_len += 1;
                }
                _ => (),
            },
            Mode::Edit => {
                let ctrl = event & key::CTRL != 0;
                let shift = event & key::SHIFT != 0;
                if event & key::ALT != 0 {
                    return false;
                }
                if ctrl {
                    match code {
                        65 | 97 => {
                            self.anchor = Some(0);
                            self.doc.cursor = self.doc.length;
                            return false;
                        }
                        67 | 99 => {
                            self.copy(false);
                            return false;
                        }
                        83 | 115 => {
                            if shift {
                                self.mode = Mode::Name(true);
                                self.prompt_len = 0;
                                self.after_save = None;
                                self.message = b"Save as: enter a filename";
                            } else {
                                return self.save(self.name, self.name_len);
                            }
                            return false;
                        }
                        88 | 120 => {
                            self.copy(true);
                            return false;
                        }
                        86 | 118 => {
                            self.paste();
                            return false;
                        }
                        81 | 113 => return self.request(Action::Quit),
                        79 | 111 => return self.request(Action::Open),
                        78 | 110 => return self.request(Action::New),
                        90 | 122 | 89 | 121 => {
                            self.doc.undo();
                            self.anchor = None;
                            self.selecting = false;
                            return false;
                        }
                        key::HOME | key::END => (),
                        _ => return false,
                    }
                }
                let navigation = matches!(
                    code,
                    key::LEFT
                        | key::RIGHT
                        | key::UP
                        | key::DOWN
                        | key::HOME
                        | key::END
                        | key::PAGE_UP
                        | key::PAGE_DOWN
                );
                if navigation {
                    let range = self.range();
                    let old = self.doc.cursor;
                    if shift {
                        if self.anchor.is_none() {
                            self.anchor = Some(old);
                        }
                    } else {
                        self.anchor = None;
                    }
                    match code {
                        key::LEFT => {
                            self.doc.cursor = if !shift {
                                range.map_or(old.saturating_sub(1), |r| r.0)
                            } else {
                                old.saturating_sub(1)
                            }
                        }
                        key::RIGHT => {
                            self.doc.cursor = if !shift {
                                range.map_or((old + 1).min(self.doc.length), |r| r.1)
                            } else {
                                (old + 1).min(self.doc.length)
                            }
                        }
                        key::UP => self.doc.vertical(-1),
                        key::DOWN => self.doc.vertical(1),
                        key::PAGE_UP => self.doc.vertical(-22),
                        key::PAGE_DOWN => self.doc.vertical(22),
                        key::HOME => {
                            let (row, _) = self.doc.position(old);
                            self.doc.cursor = if ctrl { 0 } else { self.doc.offset(row, 0) };
                        }
                        key::END => {
                            let (row, _) = self.doc.position(old);
                            self.doc.cursor = if ctrl {
                                self.doc.length
                            } else {
                                self.doc.offset(row, 80)
                            };
                        }
                        _ => (),
                    }
                } else {
                    match code {
                        27 => return self.request(Action::Quit),
                        8 | key::DELETE => {
                            if self.range().is_some() {
                                self.replace(b"");
                            } else {
                                self.doc.delete(code == 8);
                                self.anchor = None;
                            }
                        }
                        9 => self.replace(b"    "),
                        10 => self.replace(b"\n"),
                        32..=126 => self.replace(&[code as u8]),
                        _ => (),
                    }
                }
            }
        }
        false
    }
    fn pointer(&mut self, event: window::Event) -> (bool, bool) {
        let old = self.mouse_buttons;
        self.mouse_buttons = event.buttons;
        let hit = if event.y >= 4 && event.y < 28 {
            [(8, 72), (88, 72), (168, 72), (248, 88), (344, 72)]
                .iter()
                .position(|&(x, w)| event.x >= x && event.x < x + w)
        } else {
            None
        };
        let old_hover = self.hover;
        self.hover = hit;
        if !matches!(self.mode, Mode::Edit) {
            self.capture = None;
            return (false, false);
        }
        let mut redraw = false;
        if event.buttons & 1 != 0 && old & 1 == 0 {
            self.capture = hit;
            self.selecting = false;
            redraw = true;
            if hit.is_none() && event.x >= 0 && event.x < 640 && event.y >= 32 && event.y < 384 {
                self.doc.cursor = self.doc.offset(
                    self.top + (event.y as usize - 32) / 16,
                    event.x as usize / 8,
                );
                self.anchor = Some(self.doc.cursor);
                self.selecting = true;
            }
        }
        if self.selecting && (old & 1 != 0 || event.buttons & 1 != 0) {
            if event.y < 32 {
                self.top = self.top.saturating_sub(1);
            } else if event.y >= 384 {
                self.top = (self.top + 1).min(self.doc.position(self.doc.length).0);
            }
            self.doc.cursor = self.doc.offset(
                self.top + ((event.y.clamp(32, 383) - 32) / 16) as usize,
                (event.x.clamp(0, 639) / 8) as usize,
            );
            redraw = true;
        }
        if old & 1 != 0 && event.buttons & 1 == 0 {
            self.selecting = false;
            redraw = true;
            if let Some(button) = self.capture.take() {
                if Some(button) == hit {
                    match button {
                        0 => return (self.save(self.name, self.name_len), true),
                        1 => return (self.request(Action::Open), true),
                        2 => return (self.request(Action::New), true),
                        3 => {
                            self.mode = Mode::Name(true);
                            self.prompt_len = 0;
                            self.after_save = None;
                            self.message = b"Save as: enter a filename";
                        }
                        _ => return (self.request(Action::Quit), true),
                    }
                }
            }
        }
        (
            false,
            redraw || (self.capture.is_some() && hit != old_hover),
        )
    }
    fn draw(&mut self, handle: u32) -> bool {
        let (row, column) = self.doc.position(self.doc.cursor);
        if row < self.top {
            self.top = row;
        }
        if row >= self.top + 22 {
            self.top = row - 21;
        }
        if window::draw(handle, &[Draw::rect(0, 0, 640, 32, 0xc4c4c4)]) < 0 {
            return false;
        }
        for (i, (x, w, label)) in [
            (8, 72, b"Save" as &[u8]),
            (88, 72, b"Open"),
            (168, 72, b"New"),
            (248, 88, b"Save as"),
            (344, 72, b"Quit"),
        ]
        .iter()
        .enumerate()
        {
            let pressed = self.capture == Some(i) && self.hover == Some(i);
            let (light, dark, d) = if pressed {
                (0x404040, 0xffffff, 1)
            } else {
                (0xffffff, 0x404040, 0)
            };
            if window::draw(
                handle,
                &[
                    Draw::rect(*x, 4, *w, 24, 0xc4c4c4),
                    Draw::rect(*x, 4, *w, 1, light),
                    Draw::rect(*x, 4, 1, 24, light),
                    Draw::rect(*x, 27, *w, 1, dark),
                    Draw::rect(x + w - 1, 4, 1, 24, dark),
                    Draw::text(x + 8 + d, 8 + d, 0, label),
                ],
            ) < 0
            {
                return false;
            }
        }
        let mut name = [b' '; 26];
        name[0] = if self.doc.dirty() { b'*' } else { b' ' };
        let text = if self.name_len == 0 {
            b"Untitled" as &[u8]
        } else {
            &self.name[..self.name_len]
        };
        let length = text.len().min(24);
        name[2..2 + length].copy_from_slice(&text[..length]);
        if window::draw(handle, &[Draw::text(424, 8, 0, &name)]) < 0 {
            return false;
        }
        let range = self.range();
        let caret = if matches!(self.mode, Mode::Edit) && range.is_none() {
            Some(row - self.top)
        } else {
            None
        };
        for y in 0..22 {
            let line = self.doc.screen_row(self.top + y);
            let mark = range.map_or((0, 0), |(a, b)| self.doc.selection_span(self.top + y, a, b));
            if self.cache[y] != line
                || self.marks[y] != mark
                || self.caret == Some(y)
                || caret == Some(y)
                || (self.modal && y == 21)
            {
                if window::row(handle, 0, 32 + y as u32 * 16, 0xffffff, 0, &line) < 0 {
                    return false;
                }
                if mark.1 > mark.0 {
                    if window::row(
                        handle,
                        mark.0 as u32 * 8,
                        32 + y as u32 * 16,
                        0x153e8e,
                        0xffffff,
                        &line[mark.0 as usize..mark.1 as usize],
                    ) < 0
                    {
                        return false;
                    }
                }
                self.cache[y] = line;
                self.marks[y] = mark;
            }
        }
        self.caret = caret;
        self.modal = matches!(self.mode, Mode::Name(_) | Mode::Overwrite);
        let mut status = [b' '; 80];
        let n = self.message.len().min(80);
        status[..n].copy_from_slice(&self.message[..n]);
        if window::row(handle, 0, 384, 0xc4c4c4, 0, &status) < 0 {
            return false;
        }
        if matches!(self.mode, Mode::Name(_) | Mode::Overwrite) {
            let mut prompt = [b' '; 80];
            let start = self.prompt_len.saturating_sub(78);
            let shown = self.prompt_len - start;
            prompt[..shown].copy_from_slice(&self.prompt[start..self.prompt_len]);
            window::row(handle, 0, 368, 0x153e8e, 0xffffff, &prompt);
            window::draw(
                handle,
                &[Draw::rect(shown as u32 * 8, 368, 1, 16, 0xffffff)],
            );
        } else if matches!(self.mode, Mode::Edit) && range.is_none() {
            window::draw(
                handle,
                &[Draw::rect(
                    column as u32 * 8,
                    32 + (row - self.top) as u32 * 16,
                    1,
                    16,
                    0,
                )],
            );
        }
        window::present(handle) >= 0
    }
}
fn main() -> u32 {
    let mut editor = Editor::new();
    let mut args = [0; 128];
    let length = xinyi::arguments(&mut args);
    if length < 0 || length > 105 {
        xinyi::print("Invalid notepad filename.\n");
        return 1;
    }
    if length > 0 {
        let mut name = [0; 105];
        name[..length as usize].copy_from_slice(&args[..length as usize]);
        if !editor.open(name, length as usize) {
            xinyi::write(editor.message);
            xinyi::print("\n");
            return 1;
        }
    }
    let handle = window::open(b"Notepad");
    if handle < 0 {
        if handle == -12 {
            xinyi::print("Cannot open graphical window: not enough memory.\n");
        } else {
            xinyi::print(
                "Cannot open graphical window. Boot the BIOS image or use QEMU std VGA.\n",
            );
        }
        return 1;
    }
    if !editor.draw(handle as u32) {
        return 1;
    }
    loop {
        let mut event = window::Event::default();
        if window::event(handle as u32, &mut event, true) < 0 {
            return 1;
        }
        let (quit, redraw) = match event.kind {
            window::KEY => (editor.event(event.code), true),
            window::POINTER => editor.pointer(event),
            window::CLOSE => (editor.request(Action::Quit), true),
            window::RESET => {
                editor.capture = None;
                editor.selecting = false;
                editor.mouse_buttons = event.buttons;
                (false, true)
            }
            _ => (false, false),
        };
        if quit {
            return 0;
        }
        if redraw && !editor.draw(handle as u32) {
            return 1;
        }
    }
}
