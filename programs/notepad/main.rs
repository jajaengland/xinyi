#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
use alloc::vec::Vec;
#[path = "document.rs"]
mod model;
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use model::Document;
use window::Draw;
use xinyi::{
    file_dialog, gui, key,
    widgets::{Button, Rect, Scrollbar},
    window,
};
#[global_allocator]
static ALLOCATOR: xinyi::heap::Allocator = xinyi::heap::Allocator::new();
#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! {
    xinyi::print("Notepad: out of memory.\n");
    xinyi::exit(12)
}
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
    dialog: Option<file_dialog::Dialog>,
    prompt_volume: u32,
    modal_buttons: [Button; 3],
    width: u32,
    height: u32,
    doc: Document,
    doc_volume: u32,
    bar: Scrollbar,
    follow: bool,
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
    cache: [[u8; 96]; 30],
    caret: Option<usize>,
    modal: bool,
    anchor: Option<usize>,
    selecting: bool,
    marks: [(u8, u8); 30],
}
impl Editor {
    fn rows(&self) -> usize {
        ((self.height - 48) / 16) as usize
    }
    fn resize(&mut self, w: u32, e: window::Event) {
        if window::resize(w, e) == 0 {
            self.width = e.x as u32;
            self.height = e.y as u32;
            self.doc.set_width((self.width as usize - 16) / 8);
            self.bar.rect = Rect::new(self.width as i32 - 16, 32, 16, self.height as i32 - 48);
            self.bar.reset(0);
            self.follow = true;
            self.cache = [[0; 96]; 30];
            self.marks = [(0, 0); 30];
            self.caret = None;
            self.selecting = false;
            self.capture = None;
            self.mouse_buttons = 0;
        }
    }
    fn new() -> Self {
        Self {
            dialog: None,
            prompt_volume: 0,
            modal_buttons: [
                Button::new(Rect::new(128, 220, 112, 28)),
                Button::new(Rect::new(264, 220, 112, 28)),
                Button::new(Rect::new(400, 220, 112, 28)),
            ],
            width: 640,
            height: 400,
            doc: Document::new(),
            doc_volume: xinyi::directory::volume(),
            bar: Scrollbar::new(Rect::new(624, 32, 16, 352)),
            follow: true,
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
            message: b"Ready - files up to 64 KiB",
            cache: [[0; 96]; 30],
            caret: None,
            modal: false,
            anchor: None,
            selecting: false,
            marks: [(0, 0); 30],
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
            self.message = b"Edit failed: 64 KiB limit or insufficient memory";
        }
    }
    fn copy(&mut self, cut: bool) {
        let Some((a, b)) = self.range() else {
            self.message = b"Select text first (drag, Shift+arrows, or Ctrl+A)";
            return;
        };
        if xinyi::clipboard_set(&self.doc.data[a..b]) < 0 {
            self.message = b"Clipboard limit is 1024 bytes; selection retained";
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
        let Some(text) = model::normalize(&raw[..n as usize]) else {
            self.message = b"Clipboard text cannot fit in this document";
            return;
        };
        self.replace(&text);
    }
    fn action(&mut self, action: Action) -> bool {
        match action {
            Action::Quit => return true,
            Action::New => {
                self.doc.clear();
                self.doc_volume = xinyi::directory::volume();
                self.follow = true;
                self.anchor = None;
                self.selecting = false;
                self.name_len = 0;
                self.top = 0;
                self.message = b"New file";
                self.mode = Mode::Edit;
            }
            Action::Open => {
                self.begin_name(false);
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
            self.begin_name(true);
            self.prompt_len = 0;
            self.message = b"Save as: enter a filename (ASCII 8.3 paths)";
            return false;
        }
        let volume = if matches!(self.mode, Mode::Name(_) | Mode::Overwrite) {
            self.prompt_volume
        } else {
            self.doc_volume
        };
        let result = xinyi::files::save_on(&name[..length], &self.doc.data, volume);
        if result < 0 {
            self.message = match result {
                -13 => b"Save failed: file is read-only",
                -28 => b"Save failed: disk full",
                -19 => b"Save failed: no mounted drive",
                -22 => b"Save failed: invalid filename",
                -116 => b"Drive changed. Use Save As to choose the destination.",
                -12 => b"Save failed: not enough memory; document retained",
                _ => b"Save failed: disk error; document retained",
            };
            self.mode = Mode::Edit;
            self.after_save = None;
            return false;
        }
        self.name = name;
        self.name_len = length;
        self.doc_volume = volume;
        if !self.doc.mark_saved() {
            self.message = b"Saved; insufficient memory to update saved-state marker";
            self.mode = Mode::Edit;
            self.after_save = None;
            return false;
        }
        self.message = b"Saved to disk";
        self.mode = Mode::Edit;
        if let Some(action) = self.after_save.take() {
            self.action(action)
        } else {
            false
        }
    }
    fn open(&mut self, name: [u8; 105], length: usize) -> bool {
        let mut data = Vec::new();
        if data.try_reserve_exact(model::LIMIT).is_err() {
            self.message = b"Cannot open: not enough memory; document retained";
            return false;
        }
        data.resize(model::LIMIT, 0);
        let volume = xinyi::directory::volume();
        let result = xinyi::files::read_on(&name[..length], &mut data, volume);
        if result >= 0 {
            if !self.doc.load(&data[..result as usize]) {
                self.message =
                    b"Cannot open: unsupported text, expanded size over 64 KiB or low memory";
                return false;
            }
        } else if result == -2 {
            self.doc.clear();
            self.doc_volume = xinyi::directory::volume();
            self.follow = true;
            self.message = b"New file - save to create it";
        } else {
            self.message = if result == -27 {
                b"Cannot open: file exceeds 64 KiB"
            } else {
                b"Cannot open file; current document retained"
            };
            return false;
        }
        self.name = name;
        self.name_len = length;
        self.doc_volume = volume;
        self.follow = true;
        self.top = 0;
        self.anchor = None;
        self.selecting = false;
        self.mode = Mode::Edit;
        if result >= 0 {
            self.message = b"Opened (CRLF normalized, tabs expanded)";
        }
        true
    }
    fn begin_name(&mut self, save: bool) {
        self.mode = Mode::Name(save);
        let mut dialog = file_dialog::Dialog::new(save);
        dialog.reset(self.mouse_buttons);
        self.dialog = Some(dialog);
        self.prompt_len = 0;
    }
    fn dialog_event(&mut self, event: window::Event) -> bool {
        let result = if let Some(d) = self.dialog.as_mut() {
            d.event(event)
        } else {
            return false;
        };
        match result {
            file_dialog::Result::Pending => false,
            file_dialog::Result::Cancel => {
                self.dialog = None;
                self.mode = Mode::Edit;
                self.after_save = None;
                self.message = b"Cancelled";
                false
            }
            file_dialog::Result::Accept { path, len, volume } => {
                self.prompt = path;
                self.prompt_len = len;
                self.prompt_volume = volume;
                if let Mode::Name(save) = self.mode {
                    if volume != xinyi::directory::volume() {
                        self.message = b"Drive changed; reopen the dialog";
                        return false;
                    }
                    if save {
                        let exists = xinyi::files::size_on(&path[..len], volume);
                        if (exists >= 0 || exists == -27)
                            && (len != self.name_len || path[..len] != self.name[..self.name_len])
                        {
                            self.dialog = None;
                            self.mode = Mode::Overwrite;
                            self.message = b"Replace existing file? Y yes / N cancel";
                        } else if exists >= 0 || exists == -2 {
                            self.dialog = None;
                            return self.save(path, len);
                        } else {
                            self.message = b"Cannot check destination; document retained";
                        }
                    } else {
                        self.dialog = None;
                        self.open(path, len);
                        self.mode = Mode::Edit;
                    }
                }
                false
            }
        }
    }
    fn event(&mut self, event: u32) -> bool {
        let code = event as u16;
        if matches!(self.mode, Mode::Edit)
            && ((event & key::CTRL == 0 && matches!(code,8|10|32..=126|0x100..=0x108))
                || (event & key::CTRL != 0
                    && matches!(code, 65 | 97 | 86 | 118 | 88 | 120 | 90 | 122 | 89 | 121 | key::HOME | key::END)))
        {
            self.follow = true;
        }

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
                121 | 89 => {
                    if self.prompt_volume != xinyi::directory::volume() {
                        self.mode = Mode::Edit;
                        self.after_save = None;
                        self.message = b"Drive changed; save cancelled";
                    } else {
                        return self.save(self.prompt, self.prompt_len);
                    }
                }
                110 | 78 | 27 => {
                    self.mode = Mode::Edit;
                    self.after_save = None;
                    self.message = b"Save cancelled";
                }
                _ => (),
            },
            Mode::Name(_) => {
                return self.dialog_event(window::Event {
                    kind: window::KEY,
                    code: event,
                    ..window::Event::default()
                })
            }
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
                                self.begin_name(true);
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
                            if matches!(code, 89 | 121) {
                                self.doc.redo();
                            } else {
                                self.doc.undo();
                            }
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
                        key::PAGE_UP => self.doc.vertical(-(self.rows() as i32)),
                        key::PAGE_DOWN => self.doc.vertical(self.rows() as i32),
                        key::HOME => {
                            let (row, _) = self.doc.position(old);
                            self.doc.cursor = if ctrl { 0 } else { self.doc.offset(row, 0) };
                        }
                        key::END => {
                            let (row, _) = self.doc.position(old);
                            self.doc.cursor = if ctrl {
                                self.doc.length
                            } else {
                                self.doc.offset(row, self.doc.width())
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
        if matches!(self.mode, Mode::Name(_)) {
            let redraw = (event.buttons | self.mouse_buttons) & 1 != 0;
            self.mouse_buttons = event.buttons;
            return (self.dialog_event(event), redraw);
        }
        if matches!(self.mode, Mode::Confirm(_) | Mode::Overwrite) {
            let redraw = (event.buttons | self.mouse_buttons) & 1 != 0;
            self.mouse_buttons = event.buttons;
            let overwrite = matches!(self.mode, Mode::Overwrite);
            let mut action = None;
            for (i, b) in self.modal_buttons.iter_mut().enumerate() {
                if overwrite && i == 2 {
                    continue;
                }
                if b.pointer(event.x, event.y, event.buttons) {
                    action = Some(i);
                }
            }
            if let Some(i) = action {
                let code = if matches!(self.mode, Mode::Overwrite) {
                    [b'y', b'n', 27][i]
                } else {
                    [b's', b'd', b'c'][i]
                };
                return (self.event(code as u32), true);
            }
            return (false, redraw);
        }

        self.bar.range(
            self.doc.line_count().saturating_sub(self.rows()) as u32,
            self.rows() as u32,
        );
        self.bar.value = self.top as u32;
        let on_bar =
            !self.selecting && (self.bar.rect.contains(event.x, event.y) || self.bar.dragging());
        let scrolled = self.bar.pointer(event.x, event.y, event.buttons);
        if on_bar {
            self.mouse_buttons = event.buttons;
            self.capture = None;
            self.selecting = false;
            if scrolled {
                self.top = self.bar.value as usize;
                self.follow = false;
            }
            return (false, scrolled);
        }
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
            self.follow = true;
            self.capture = hit;
            self.selecting = false;
            redraw = true;
            if hit.is_none()
                && event.x >= 0
                && event.x < self.width as i32 - 16
                && event.y >= 32
                && event.y < self.height as i32 - 16
            {
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
            } else if event.y >= self.height as i32 - 16 {
                self.top = (self.top + 1).min(self.doc.position(self.doc.length).0);
            }
            self.doc.cursor = self.doc.offset(
                self.top + ((event.y.clamp(32, self.height as i32 - 17) - 32) / 16) as usize,
                (event.x.clamp(0, self.width as i32 - 17) / 8) as usize,
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
                            self.begin_name(true);
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
        if self.modal {
            self.cache = [[0; 96]; 30];
            self.marks = [(0, 0); 30];
        }
        if !matches!(self.mode, Mode::Name(_)) {
            self.dialog = None;
        }

        self.doc.set_width((self.width as usize - 16) / 8);
        self.bar.rect = Rect::new(self.width as i32 - 16, 32, 16, self.height as i32 - 48);
        let (row, column) = self.doc.position(self.doc.cursor);
        if self.follow && row < self.top {
            self.top = row;
        }
        if self.follow && row >= self.top + self.rows() {
            self.top = row + 1 - self.rows();
        }
        if window::draw(handle, &[Draw::rect(0, 0, self.width, 32, 0xc4c4c4)]) < 0 {
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
            let mut button = Button::new(Rect::new(*x as i32, 4, *w as i32, 24));
            button.pressed = self.capture == Some(i);
            button.hover = self.hover == Some(i);
            gui::button(handle, &button, label, false);
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
        self.top = self
            .top
            .min(self.doc.line_count().saturating_sub(self.rows()));
        let range = self.range();
        let caret = if matches!(self.mode, Mode::Edit)
            && range.is_none()
            && row >= self.top
            && row < self.top + self.rows()
        {
            Some(row - self.top)
        } else {
            None
        };
        for y in 0..self.rows() {
            let line = self.doc.screen_row_n::<96>(self.top + y);
            let mark = range.map_or((0, 0), |(a, b)| self.doc.selection_span(self.top + y, a, b));
            if self.cache[y] != line
                || self.marks[y] != mark
                || self.caret == Some(y)
                || caret == Some(y)
                || (self.modal && y == 21)
            {
                if window::row(
                    handle,
                    0,
                    32 + y as u32 * 16,
                    0xffffff,
                    0,
                    &line[..self.doc.width()],
                ) < 0
                {
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
        self.modal = !matches!(self.mode, Mode::Edit);
        let mut status = [b' '; 96];
        let n = self.message.len().min(80);
        status[..n].copy_from_slice(&self.message[..n]);
        if window::row(
            handle,
            0,
            self.height - 16,
            0xc4c4c4,
            0,
            &status[..self.width as usize / 8],
        ) < 0
        {
            return false;
        }
        self.bar.range(
            self.doc.line_count().saturating_sub(self.rows()) as u32,
            self.rows() as u32,
        );
        self.bar.value = self.top as u32;
        gui::scrollbar(handle, &self.bar);
        if let Some(d) = self.dialog.as_ref() {
            d.paint(handle);
        } else if matches!(self.mode, Mode::Confirm(_) | Mode::Overwrite) {
            gui::dialog(
                handle,
                if matches!(self.mode, Mode::Overwrite) {
                    b"Replace file?"
                } else {
                    b"Unsaved changes"
                },
                Rect::new(104, 132, 432, 136),
            );
            window::draw(
                handle,
                &[Draw::text(
                    120,
                    176,
                    0,
                    if matches!(self.mode, Mode::Overwrite) {
                        b"Replace the existing file with this document?"
                    } else {
                        b"Save your changes before continuing?"
                    },
                )],
            );
            let labels = if matches!(self.mode, Mode::Overwrite) {
                [b"Replace" as &[u8], b"Cancel", b"Cancel"]
            } else {
                [b"Save" as &[u8], b"Discard", b"Cancel"]
            };
            for (i, button) in self.modal_buttons.iter().enumerate() {
                if !matches!(self.mode, Mode::Overwrite) || i < 2 {
                    gui::button(handle, button, labels[i], false);
                }
            }
        } else if let Some(caret_row) = caret {
            window::draw(
                handle,
                &[Draw::rect(
                    column as u32 * 8,
                    32 + caret_row as u32 * 16,
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
    window::resizable(handle as u32, 640, 400, 768, 528);
    if !editor.draw(handle as u32) {
        return 1;
    }
    loop {
        let mut event = window::Event::default();
        if window::event(handle as u32, &mut event, true) < 0 {
            return 1;
        }
        let (quit, redraw) = match event.kind {
            window::RESIZE => {
                editor.resize(handle as u32, event);
                (false, true)
            }
            window::KEY => (editor.event(event.code), true),
            window::POINTER => editor.pointer(event),
            window::CLOSE => (editor.request(Action::Quit), true),
            window::RESET => {
                if let Some(d) = editor.dialog.as_mut() {
                    d.reset(event.buttons);
                }
                for b in &mut editor.modal_buttons {
                    b.reset(event.buttons);
                }
                editor.bar.reset(event.buttons);
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
