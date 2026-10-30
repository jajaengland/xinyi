#![no_std]
#![no_main]
mod selection;
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use selection::Selection;
use xinyi::{
    directory::{self, Listing},
    file_clipboard::{self, Selection as FileClip},
    key,
    window::{self, Draw},
};
struct Files {
    width: u32,
    height: u32,
    list: Listing,
    offset: u16,
    selection: Selection,
    path: [u8; 105],
    length: usize,
    volume: u32,
    status: &'static [u8],
    mode: u8,
    input: [u8; 12],
    used: usize,
    last: usize,
    clicked: u32,
    modifiers: u32,
    menu: Option<(i32, i32)>,
    hover: Option<usize>,
    can_paste: bool,
    pending: FileClip,
    quit: bool,
    press: Option<(i32, i32, usize, u32, bool)>,
    dragging: bool,
    open_on_release: bool,
    drop_batch: bool,
    destination: [u8; 105],
    destination_len: usize,
    drop_row: Option<usize>,
}
fn error(n: i32) -> &'static [u8] {
    xinyi::sounds::play(xinyi::sounds::Event::Error);
    match n {
        -2 => b"Item no longer exists. Refresh the folder.",
        -8 => b"Invalid app folder. Check its executable and matching .APP manifest.",
        -13 => b"Read-only item or folder.",
        -17 => b"That name already exists.",
        -20 => b"Not a folder.",
        -21 => b"Expected a regular file.",
        -22 => b"Invalid 8.3 path, or destination is inside the source.",
        -24 => b"Close another window before opening this item.",
        -27 => b"File/tree exceeds the supported size or depth.",
        -28 => b"The disk is full.",
        -39 => b"Folder is not empty.",
        -16 => b"Folder is in use. Close the app or leave its folder first.",
        -116 => b"Drive or file selection changed. Refresh and select again.",
        -12 => b"Not enough memory.",
        _ => b"Disk operation failed. Check remaining items.",
    }
}
impl Files {
    fn toolbar_cell(&self) -> u32 {
        self.width / 9
    }
    fn page(&self) -> u16 {
        ((self.height - 112) / 16) as u16
    }
    fn resize(&mut self, w: u32, e: window::Event) {
        if window::resize(w, e) == 0 {
            self.width = e.x as u32;
            self.height = e.y as u32;
            self.menu = None;
            self.hover = None;
            self.last = usize::MAX;
            self.refresh();
        }
    }
    fn rows(&self) -> usize {
        (self.list.count as usize).min(self.page() as usize)
    }
    fn refresh(&mut self) {
        self.last = usize::MAX;
        let volume = directory::volume();
        let n = directory::list(b".", self.offset, &mut self.list);
        if n < 0 {
            self.list = Listing::new();
            self.status = error(n);
        } else if self.offset as u32 >= self.list.total && self.offset > 0 {
            self.offset = 0;
            self.refresh();
            return;
        }
        let n = xinyi::working_directory(&mut self.path);
        self.length = n.max(0) as usize;
        if volume != directory::volume() {
            self.volume = 0;
            self.list = Listing::new();
            self.selection.clear();
            self.status = error(-116);
        } else {
            self.volume = volume;
        }
    }
    fn navigate(&mut self, name: &[u8]) {
        let n = directory::cd(name);
        if n < 0 {
            self.status = error(n);
        } else {
            self.offset = 0;
            self.selection.clear();
            self.status = b"Folder opened.";
            self.refresh();
            if self.list.total == 0 {
                xinyi::sounds::play(xinyi::sounds::Event::Empty);
            }
        }
    }
    fn launch(&mut self, prefix: &[u8], name: &[u8]) {
        let mut line = [0; 128];
        let n = prefix.len() + name.len();
        line[..prefix.len()].copy_from_slice(prefix);
        line[prefix.len()..n].copy_from_slice(name);
        let n = xinyi::launch(&line[..n]);
        self.status = if n < 0 {
            error(n)
        } else {
            b"Opened. Use the taskbar to return to Files; F5 refreshes."
        };
    }
    fn open(&mut self) {
        if self.selection.count != 1 {
            self.status = b"Select one item to open.";
            return;
        }
        if self.volume != directory::volume() {
            self.status = error(-116);
            return;
        }
        let name = self.selection.names[0];
        let n = self.selection.lengths[0] as usize;
        if self.selection.flags[0] & 1 != 0 {
            self.navigate(&name[..n]);
        } else if (name[..n].ends_with(b".BIN") || name[..n].ends_with(b".ELF")) {
            self.launch(b"terminal run ", &name[..n]);
        } else {
            let result = xinyi::associations::open(&name[..n], self.volume);
            self.status = match result {
                0 => b"Opened with its associated application.",
                xinyi::associations::NO_HANDLER => {
                    b"No installed application handles this file type."
                }
                xinyi::associations::AMBIGUOUS => {
                    b"More than one app handles this type. Open the app explicitly."
                }
                _ => error(result),
            };
        }
    }
    fn select(&mut self, row: usize, ctrl: bool, shift: bool) {
        let e = self.list.entries[row];
        let index = self.offset + row as u16;
        if shift {
            let anchor = self.selection.anchor.unwrap_or(index);
            let a = anchor.min(index);
            let b = anchor.max(index);
            if b - a >= 32 {
                self.status = b"Select at most 32 items at a time.";
                return;
            }
            let mut page = Listing::new();
            let n = directory::list(b".", a, &mut page);
            if n < 0 {
                self.status = error(n);
                return;
            }
            if page.count < (b - a + 1) as u32 {
                self.status = b"Folder changed. Refresh before selecting.";
                return;
            }
            self.selection.clear();
            for e in &page.entries[..(b - a + 1) as usize] {
                self.selection
                    .add(&e.name[..e.length as usize], e.flags as u8);
            }
            self.selection.anchor = Some(anchor);
            self.selection.focus = Some(index);
        } else if ctrl {
            if !self
                .selection
                .toggle(&e.name[..e.length as usize], e.flags as u8, index)
            {
                self.status = b"Select at most 32 items at a time.";
            }
        } else {
            self.selection
                .plain(&e.name[..e.length as usize], e.flags as u8, index);
        }
    }
    fn payload(&mut self, cut: bool) -> bool {
        if self.selection.count == 0 {
            self.status = b"Select files or folders first.";
            return false;
        }
        let r = &mut self.pending;
        *r = FileClip::new();
        r.count = self.selection.count as u16;
        r.cut = cut as u16;
        r.volume = self.volume;
        for i in 0..self.selection.count {
            let extra = usize::from(self.length > 1);
            let n = self.length + extra + self.selection.lengths[i] as usize;
            if n > 105 {
                self.status = error(-22);
                return false;
            }
            r.paths[i][..self.length].copy_from_slice(&self.path[..self.length]);
            if extra != 0 {
                r.paths[i][self.length] = b'/';
            }
            r.paths[i][self.length + extra..n]
                .copy_from_slice(&self.selection.names[i][..self.selection.lengths[i] as usize]);
            r.lengths[i] = n as u16;
            r.flags[i] = self.selection.flags[i];
        }
        true
    }
    fn stage(&mut self, cut: bool) {
        if !self.payload(cut) {
            return;
        }
        let r = &mut self.pending;
        let n = file_clipboard::set(r);
        self.status = if n < 0 {
            error(n)
        } else if cut {
            b"Move selected. Open the destination, in any Files window, then Paste."
        } else {
            b"Copy selected. Open the destination, in any Files window, then Paste."
        };
    }
    fn paste(&mut self) {
        self.drop_batch = false;
        self.destination_len = 0;
        let n = file_clipboard::get(&mut self.pending);
        if n < 0 {
            self.status = error(n);
            return;
        }
        if self.pending.count == 0 {
            self.status = b"Copy or Move files first.";
            return;
        }
        self.selection.clear();
        self.paste_next(None);
    }
    fn paste_next(&mut self, renamed: Option<&[u8]>) {
        let mut override_name = renamed;
        while self.pending.count > 0 {
            let path = self.pending.paths[0];
            let n = self.pending.lengths[0] as usize;
            let start = path[..n]
                .iter()
                .rposition(|&b| b == b'/')
                .map_or(0, |i| i + 1);
            let name = override_name.take().unwrap_or(&path[start..n]);
            let mut destination = [0; 105];
            let extra = usize::from(self.destination_len > 1);
            let len = self.destination_len + extra + name.len();
            let dest = if self.destination_len == 0 {
                name
            } else {
                if len > 105 {
                    self.status = error(-22);
                    self.finish_drop();
                    return;
                }
                destination[..self.destination_len]
                    .copy_from_slice(&self.destination[..self.destination_len]);
                if extra != 0 {
                    destination[self.destination_len] = b'/';
                }
                destination[self.destination_len + extra..len].copy_from_slice(name);
                &destination[..len]
            };
            let result = directory::transfer_on(
                &path[..n],
                dest,
                if self.pending.cut != 0 { 2 } else { 1 },
                self.pending.volume,
            );
            if result == -17 {
                self.mode = 5;
                self.used = 0;
                self.status = b"Name exists. Paste as another name, or Escape to stop this batch.";
                xinyi::sounds::play(xinyi::sounds::Event::Warning);
                self.refresh();
                return;
            }
            if result < 0 {
                self.finish_drop();
                self.status = error(result);
                self.refresh();
                return;
            }
            self.selection.add(name, self.pending.flags[0]);
            for i in 1..self.pending.count as usize {
                self.pending.paths[i - 1] = self.pending.paths[i];
                self.pending.lengths[i - 1] = self.pending.lengths[i];
                self.pending.flags[i - 1] = self.pending.flags[i];
            }
            self.pending.count -= 1;
            if self.pending.cut != 0 && !self.drop_batch {
                let n = file_clipboard::update(&self.pending);
                if n < 0 {
                    self.status =
                        b"Moved an item; shared selection changed. Remaining moves stopped.";
                    self.refresh();
                    return;
                }
                self.pending.version = n as u32;
            }
        }
        if self.destination_len > 0
            && !self.destination[..self.destination_len]
                .eq_ignore_ascii_case(&self.path[..self.length])
        {
            self.selection.clear();
        }
        self.finish_drop();
        self.status = b"Paste complete.";
        xinyi::sounds::play(xinyi::sounds::Event::Complete);
        self.refresh();
    }
    fn finish_drop(&mut self) {
        if self.drop_batch {
            xinyi::drag_drop::finish();
            self.drop_batch = false;
        }
        self.destination_len = 0;
        self.drop_row = None;
    }
    fn receive_drop(&mut self, e: window::Event) {
        if self.mode != 0 {
            xinyi::drag_drop::finish();
            self.status = b"Drop cancelled. Finish the current dialog first.";
            return;
        }
        self.pending = FileClip::new();
        self.pending.version = e.code;
        let n = xinyi::drag_drop::take(&mut self.pending);
        if n < 0 {
            self.status = error(n);
            xinyi::drag_drop::finish();
            return;
        }
        self.drop_batch = true;
        self.dragging = false;
        self.press = None;
        self.drop_row = None;
        if self.mode != 0 || self.volume != directory::volume() {
            self.status = b"Drop cancelled: folder or dialog changed.";
            self.finish_drop();
            return;
        }
        self.destination = self.path;
        self.destination_len = self.length;
        if e.y >= 80 {
            let row = (e.y - 80) as usize / 16;
            if row < self.rows() && self.list.entries[row].flags & 1 != 0 {
                let item = self.list.entries[row];
                let n = item.length as usize;
                let extra = usize::from(self.destination_len > 1);
                if self.destination_len + extra + n > 105 {
                    self.status = error(-22);
                    self.finish_drop();
                    return;
                }
                if extra != 0 {
                    self.destination[self.destination_len] = b'/';
                    self.destination_len += 1;
                }
                self.destination[self.destination_len..self.destination_len + n]
                    .copy_from_slice(&item.name[..n]);
                self.destination_len += n;
            }
        }
        // Dropping into the source folder is a no-op, never a rename prompt.
        if (0..self.pending.count as usize).all(|i| {
            let p = &self.pending.paths[i][..self.pending.lengths[i] as usize];
            p.iter().rposition(|&b| b == b'/').is_some_and(|n| {
                let parent = if n == 0 { &p[..1] } else { &p[..n] };
                parent.eq_ignore_ascii_case(&self.destination[..self.destination_len])
            })
        }) {
            self.status = b"Already in this folder.";
            self.finish_drop();
            return;
        }
        self.selection.clear();
        self.paste_next(None);
    }
    fn delete(&mut self) {
        let s = self.selection;
        for i in 0..s.count {
            let n = directory::action_on(
                &s.names[i][..s.lengths[i] as usize],
                if s.flags[i] & 1 != 0 { 2 } else { 1 },
                self.volume,
            );
            if n < 0 {
                self.status = error(n);
                self.selection.clear();
                self.refresh();
                return;
            }
        }
        self.status = b"Deleted selected items.";
        xinyi::sounds::play(xinyi::sounds::Event::Complete);
        self.selection.clear();
        self.refresh();
    }
    fn manage_app(&mut self, remove: bool) {
        if self.selection.count != 1 || self.selection.flags[0] & 1 == 0 {
            self.status = b"Select one application folder.";
            return;
        }
        let name = self.selection.names[0];
        let n = self.selection.lengths[0] as usize;
        let result = directory::app_action(&name[..n], remove as u16, self.volume);
        self.status = if result < 0 {
            error(result)
        } else {
            xinyi::sounds::play(xinyi::sounds::Event::Complete);
            if remove {
                b"App removed. Your documents are unchanged."
            } else {
                b"App installed. Open Apps to launch it."
            }
        };
        if result >= 0 {
            self.selection.clear();
        }
        self.refresh();
    }
    fn action(&mut self, n: usize) -> bool {
        self.menu = None;
        self.hover = None;
        if matches!(n, 4 | 10 | 11) && self.selection.count == 0 {
            self.status = b"Select files or folders first.";
            return false;
        }
        match n {
            0 => self.navigate(b".."),
            1 => self.open(),
            2 | 3 => {
                self.mode = if n == 2 { 1 } else { 2 };
                self.used = 0;
            }
            4 => {
                self.mode = 3;
                xinyi::sounds::play(xinyi::sounds::Event::Warning);
            }
            5 => {
                self.selection.clear();
                self.refresh();
            }
            6 => {
                self.offset = self.offset.saturating_sub(self.page());
                self.refresh();
            }
            7 => {
                if self.offset as u32 + (self.page() as u32) < self.list.total {
                    self.offset += self.page();
                    self.refresh();
                }
            }
            8 => {
                self.quit = true;
                return true;
            }
            9 => {
                if self.selection.count != 1 {
                    self.status = b"Select one item to rename.";
                } else {
                    self.mode = 4;
                    self.used = 0;
                }
            }
            10 => self.stage(false),
            11 => self.stage(true),
            12 => self.paste(),
            13 | 14 => {
                if self.selection.count != 1 || self.selection.flags[0] & 1 == 0 {
                    self.status = b"Select one application folder.";
                } else if n == 14 && !self.path[..self.length].eq_ignore_ascii_case(b"/APPS") {
                    self.status = b"Open /APPS and select an installed app to remove.";
                } else {
                    self.mode = if n == 13 { 6 } else { 7 };
                    xinyi::sounds::play(xinyi::sounds::Event::Warning);
                }
            }
            _ => (),
        }
        false
    }
    fn key(&mut self, event: u32) -> bool {
        self.modifiers = event & (key::CTRL | key::SHIFT);
        let code = event as u16;
        let ctrl = event & key::CTRL != 0;
        if self.menu.is_some() {
            if code == 27 {
                self.menu = None;
                return false;
            }
            if code == 10 {
                if let Some(i) = self.hover {
                    return self.action([1, 9, 10, 11, 12, 4, 5, 13, 14][i]);
                }
            }
            if matches!(code, key::UP | key::DOWN) {
                self.hover = Some(match (self.hover, code) {
                    (Some(i), key::UP) => i.saturating_sub(1),
                    (Some(i), _) => (i + 1).min(8),
                    _ => 0,
                });
            }
            return false;
        }
        if self.mode != 0 {
            if code == 27 {
                self.finish_drop();
                self.mode = 0;
                self.pending.count = 0;
                self.status = b"Cancelled. Earlier batch items may have completed.";
                return false;
            }
            if matches!(self.mode, 3 | 6 | 7) {
                if matches!(code, 89 | 121) {
                    let mode = self.mode;
                    self.mode = 0;
                    if mode == 3 {
                        self.delete();
                    } else {
                        self.manage_app(mode == 7);
                    }
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
                    match mode {
                        1 => self.launch(b"notepad ", &name[..used]),
                        2 => {
                            let n = directory::action_on(&name[..used], 0, self.volume);
                            self.status = if n < 0 { error(n) } else { b"Folder created." };
                            self.refresh();
                        }
                        4 => {
                            let s = self.selection;
                            let n = directory::transfer_on(
                                &s.names[0][..s.lengths[0] as usize],
                                &name[..used],
                                0,
                                self.volume,
                            );
                            self.status = if n < 0 { error(n) } else { b"Renamed." };
                            if n == 0 {
                                self.selection.clear();
                                self.selection.add(&name[..used], s.flags[0]);
                            }
                            self.refresh();
                        }
                        5 => self.paste_next(Some(&name[..used])),
                        _ => (),
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
        if ctrl {
            match code {
                81 | 113 => return true,
                78 | 110 => return self.action(2),
                67 | 99 => return self.action(10),
                88 | 120 => return self.action(11),
                86 | 118 => return self.action(12),
                65 | 97 => {
                    if self.list.total > 32 {
                        self.status = b"Select at most 32 items at a time.";
                    } else {
                        let mut page = Listing::new();
                        if directory::list(b".", 0, &mut page) == 0 {
                            self.selection.clear();
                            for e in &page.entries[..page.count as usize] {
                                self.selection
                                    .add(&e.name[..e.length as usize], e.flags as u8);
                            }
                        }
                    }
                    return false;
                }
                _ => (),
            }
        }
        match code {
            0x121 => {
                self.action(9);
            }
            key::UP | key::DOWN | key::HOME | key::END => {
                if self.rows() > 0 {
                    let old = self.selection.focus.unwrap_or(self.offset);
                    let index = match code {
                        key::UP => old.saturating_sub(1),
                        key::DOWN => {
                            if self.selection.focus.is_none() {
                                self.offset
                            } else {
                                old.saturating_add(1)
                                    .min(self.list.total.saturating_sub(1) as u16)
                            }
                        }
                        key::HOME => 0,
                        _ => self.list.total.saturating_sub(1) as u16,
                    };
                    if index < self.offset || index >= self.offset + self.page() {
                        self.offset = index / self.page() * self.page();
                        self.refresh();
                    }
                    let row = (index - self.offset) as usize;
                    if row < self.rows() {
                        self.select(row, false, event & key::SHIFT != 0);
                    }
                }
            }
            10 => self.open(),
            8 => self.navigate(b".."),
            key::DELETE => {
                self.action(4);
            }
            key::PAGE_UP => {
                self.action(6);
            }
            key::PAGE_DOWN => {
                self.action(7);
            }
            key::F5 => {
                self.selection.clear();
                self.refresh();
            }
            _ => (),
        }
        false
    }
    fn enabled(&self, i: usize) -> bool {
        match i {
            0 | 1 => self.selection.count == 1,
            2 | 3 | 5 => self.selection.count > 0,
            4 => self.can_paste,
            7 | 8 => {
                self.selection.count == 1
                    && self.selection.flags[0] & 1 != 0
                    && (i == 7 || self.path[..self.length].eq_ignore_ascii_case(b"/APPS"))
            }
            _ => true,
        }
    }
    fn menu_hit(&self, x: i32, y: i32) -> Option<usize> {
        let (a, b) = self.menu?;
        if (a + 4..a + 164).contains(&x) && (b + 4..b + 202).contains(&y) {
            Some(((y - b - 4) / 22) as usize)
        } else {
            None
        }
    }
    fn pointer(&mut self, event: window::Event, previous: u32) -> bool {
        if event.buttons & 1 == 0 && previous & 1 != 0 {
            if let Some((_, _, row, mods, selected)) = self.press.take() {
                if !self.dragging && mods & key::SHIFT == 0 && row < self.rows() {
                    if mods & key::CTRL == 0 {
                        self.select(row, false, false);
                    } else if selected {
                        self.select(row, true, false);
                    }
                }
            }
            if !self.dragging && self.open_on_release {
                self.open();
            }
            self.open_on_release = false;
            self.dragging = false;
            return true;
        }
        if event.buttons & 1 != 0 && !self.dragging {
            if let Some((x, y, _, _, _)) = self.press {
                if (event.x - x).abs().max((event.y - y).abs()) >= 5 {
                    self.press = None;
                    self.open_on_release = false;
                    self.last = usize::MAX;
                    if self.payload(event.code & key::SHIFT != 0) {
                        let n = xinyi::drag_drop::begin(&self.pending);
                        if n == 0 {
                            self.dragging = true;
                            self.status=b"Drag to Files. Copy by default; hold Shift to move. Escape cancels.";
                        } else {
                            self.status = error(n);
                        }
                    }
                    return true;
                }
            }
        }
        let left = event.buttons & 1 != 0 && previous & 1 == 0;
        let right = event.buttons & 2 != 0 && previous & 2 == 0;
        if self.mode != 0 {
            return false;
        }
        if self.menu.is_some() {
            if left {
                let hit = self.menu_hit(event.x, event.y);
                self.menu = None;
                if let Some(i) = hit {
                    if self.enabled(i) {
                        self.action([1, 9, 10, 11, 12, 4, 5, 13, 14][i]);
                    }
                }
                return true;
            }
            if !right {
                let hover = self.menu_hit(event.x, event.y);
                let changed = hover != self.hover;
                self.hover = hover;
                return changed;
            }
        }
        let row = if (8..self.width as i32 - 8).contains(&event.x)
            && (80..self.height as i32 - 32).contains(&event.y)
        {
            let row = (event.y - 80) as usize / 16;
            if row < self.rows() {
                Some(row)
            } else {
                None
            }
        } else {
            None
        };
        if right {
            if let Some(row) = row {
                let e = self.list.entries[row];
                if !self.selection.contains(&e.name[..e.length as usize]) {
                    self.select(row, false, false);
                }
            } else {
                self.selection.clear();
            }
            self.can_paste = file_clipboard::get(&mut self.pending) == 0 && self.pending.count > 0;
            self.menu = Some((
                event.x.clamp(0, self.width as i32 - 168),
                event.y.clamp(0, self.height as i32 - 206),
            ));
            self.hover = None;
            self.last = usize::MAX;
            return true;
        }
        if !left {
            return false;
        }
        if (0..30).contains(&event.y) && (0..self.width as i32).contains(&event.x) {
            return {
                self.action((event.x as u32 / self.toolbar_cell()).min(8) as usize);
                true
            };
        }
        if (34..60).contains(&event.y) && (0..(self.toolbar_cell() * 6) as i32).contains(&event.x) {
            self.action(9 + event.x as usize / self.toolbar_cell() as usize);
            return true;
        }
        if (80..self.height as i32 - 32).contains(&event.y) {
            if let Some(row) = row {
                let modifiers = event.code;
                let ctrl = modifiers & key::CTRL != 0;
                let shift = modifiers & key::SHIFT != 0;
                let e = self.list.entries[row];
                let selected = self.selection.contains(&e.name[..e.length as usize]);
                if shift || !selected {
                    self.select(row, ctrl, shift);
                }
                self.press = Some((event.x, event.y, row, modifiers, selected));
                let now = xinyi::ticks();
                if !ctrl && !shift && self.last == row && now.wrapping_sub(self.clicked) < 30 {
                    self.open_on_release = true;
                    self.last = usize::MAX;
                } else {
                    self.last = if ctrl || shift { usize::MAX } else { row };
                    self.clicked = now;
                }
            } else {
                self.selection.clear();
                self.last = usize::MAX;
                self.clicked = 0;
            }
            return true;
        }
        false
    }
    fn paint(&self, w: u32) {
        window::draw(w, &[Draw::rect(0, 0, self.width, self.height, 0xf4f4f4)]);
        let cell = self.toolbar_cell();
        for (i, label) in [
            b"Up" as &[u8],
            b"Open",
            b"New",
            b"Folder",
            b"Delete",
            b"Refresh",
            b"<",
            b">",
            b"Quit",
        ]
        .iter()
        .enumerate()
        {
            let x = i as u32 * cell;
            let end = if i == 8 { self.width } else { x + cell };
            window::draw(
                w,
                &[
                    Draw::rect(x + 2, 2, end - x - 4, 26, 0xc4c4c4),
                    Draw::text(x + (end - x - label.len() as u32 * 8) / 2, 8, 0, label),
                ],
            );
        }
        for (i, label) in [
            b"Rename" as &[u8],
            b"Copy",
            b"Move",
            b"Paste",
            b"Install",
            b"Remove",
        ]
        .iter()
        .enumerate()
        {
            let x = i as u32 * cell;
            window::draw(
                w,
                &[
                    Draw::rect(x + 2, 34, cell - 4, 26, 0xc4c4c4),
                    Draw::text(x + (cell - label.len() as u32 * 8) / 2, 40, 0, label),
                ],
            );
        }
        window::draw(
            w,
            &[
                Draw::text(
                    cell * 6 + 8,
                    40,
                    0x153e8e,
                    &self.path[self
                        .length
                        .saturating_sub(((self.width - cell * 4 - 24) / 8) as usize)
                        ..self.length],
                ),
                Draw::text(8, 64, 0, b"Name"),
                Draw::text(self.width / 2, 64, 0, b"Type"),
                Draw::text(self.width - 96, 64, 0, b"Bytes"),
                Draw::rect(8, 80, self.width - 16, self.height - 112, 0xffffff),
            ],
        );
        for i in 0..self.rows() {
            let e = self.list.entries[i];
            let name = &e.name[..e.length as usize];
            let chosen = self.selection.contains(name);
            let (bg, fg) = if chosen {
                (0x153e8e, 0xffffff)
            } else {
                (0xffffff, 0)
            };
            let y = 80 + i as u32 * 16;
            let mut digits = [b' '; 10];
            let mut value = e.size;
            for n in (0..10).rev() {
                digits[n] = b'0' + (value % 10) as u8;
                value /= 10;
            }
            let start = digits.iter().position(|&b| b != b'0').unwrap_or(9);
            digits[..start].fill(b' ');
            window::draw(
                w,
                &[
                    Draw::rect(8, y, self.width - 16, 16, bg),
                    Draw::text(8, y, fg, name),
                    Draw::text(
                        self.width / 2,
                        y,
                        fg,
                        if e.flags & 1 != 0 { b"Folder" } else { b"File" },
                    ),
                    Draw::text(self.width - 96, y, fg, &digits),
                ],
            );
        }
        if let Some(row) = self.drop_row {
            let y = 80 + row as u32 * 16;
            window::draw(
                w,
                &[
                    Draw::rect(8, y, self.width - 16, 2, 0x238a60),
                    Draw::rect(8, y + 14, self.width - 16, 2, 0x238a60),
                ],
            );
        }
        let hint = match self.mode {
            1 => b"New file (8.3): " as &[u8],
            2 => b"New folder (8.3): ",
            3 => b"Delete selected items? Y confirms / N cancels",
            4 => b"Rename to (8.3): ",
            5 => b"Paste as (8.3): ",
            6 => b"Install selected app folder? Y confirms / N cancels",
            7 => b"Remove app and its bundled files? Y confirms / N cancels",
            _ => b"Ctrl/Shift select | Ctrl+C/X/V | F2 rename | Right-click actions",
        };
        window::draw(
            w,
            &[
                Draw::text(8, self.height - 32, 0, hint),
                Draw::text(
                    8,
                    self.height - 16,
                    0,
                    &self.status[..self
                        .status
                        .len()
                        .min(((self.width as usize - 16) / 8).min(96))],
                ),
            ],
        );
        if matches!(self.mode, 1 | 2 | 4 | 5) {
            window::draw(
                w,
                &[Draw::text(
                    160,
                    self.height - 32,
                    0x153e8e,
                    &self.input[..self.used],
                )],
            );
        }
        if let Some((x, y)) = self.menu {
            window::draw(
                w,
                &[
                    Draw::rect((x + 4) as u32, (y + 4) as u32, 164, 202, 0x404040),
                    Draw::rect(x as u32, y as u32, 168, 206, 0xc4c4c4),
                ],
            );
            for (i, label) in [
                b"Open" as &[u8],
                b"Rename",
                b"Copy",
                b"Move",
                b"Paste",
                b"Delete",
                b"Refresh",
                b"Install app",
                b"Remove app",
            ]
            .iter()
            .enumerate()
            {
                let yy = y + 4 + i as i32 * 22;
                let hot = self.hover == Some(i) && self.enabled(i);
                window::draw(
                    w,
                    &[
                        Draw::rect(
                            (x + 4) as u32,
                            yy as u32,
                            160,
                            22,
                            if hot { 0x153e8e } else { 0xc4c4c4 },
                        ),
                        Draw::text(
                            (x + 10) as u32,
                            (yy + 3) as u32,
                            if !self.enabled(i) {
                                0x707070
                            } else if hot {
                                0xffffff
                            } else {
                                0
                            },
                            label,
                        ),
                    ],
                );
            }
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
        width: 640,
        height: 400,
        list: Listing::new(),
        offset: 0,
        selection: Selection::new(),
        path: [0; 105],
        length: 0,
        volume: 0,
        status: b"Ctrl/Shift selects multiple items. Right-click for actions.",
        mode: 0,
        input: [0; 12],
        used: 0,
        last: usize::MAX,
        clicked: 0,
        modifiers: 0,
        menu: None,
        hover: None,
        can_paste: false,
        pending: FileClip::new(),
        quit: false,
        press: None,
        dragging: false,
        open_on_release: false,
        drop_batch: false,
        destination: [0; 105],
        destination_len: 0,
        drop_row: None,
    };
    window::resizable(w, 640, 400, 792, 544);
    let mut args=[0;128];let argc=xinyi::arguments(&mut args);
    if argc>0 {
        let raw=&args[..argc as usize];let folder=raw.starts_with(b"--folder ");let path=if folder {&raw[9..]}else{raw};
        if directory::cd(path)<0 {
            if folder {return 1;}
            if let Some(at)=path.iter().rposition(|b|*b==b'/') {
                if directory::cd(&path[..at.max(1)])==0 {if xinyi::associations::open(path,directory::volume())==0 {return 0;}}
            }
        }
    }
    app.refresh();
    if app.rows() > 0 {
        app.select(0, false, false);
    }
    app.paint(w);
    xinyi::drag_drop::register();
    let mut buttons = 0;
    loop {
        let mut e = window::Event::default();
        let n = window::event(w, &mut e, true);
        if n < 0 {
            return 1;
        }
        if n == 0 {
            continue;
        }
        let repaint = match e.kind {
            window::DROP => {
                buttons = 0;
                app.receive_drop(e);
                true
            }
            window::DRAG_OVER => {
                app.drop_row = if e.y >= 80 {
                    let row = (e.y - 80) as usize / 16;
                    if row < app.rows() && app.list.entries[row].flags & 1 != 0 {
                        Some(row)
                    } else {
                        None
                    }
                } else {
                    None
                };
                true
            }
            window::DRAG_END => {
                buttons = 0;
                app.press = None;
                app.dragging = false;
                app.open_on_release = false;
                app.drop_row = None;
                app.refresh();
                true
            }
            window::RESIZE => {
                app.resize(w, e);
                true
            }
            window::CLOSE => return 0,
            window::KEY => {
                if app.key(e.code) {
                    return 0;
                }
                true
            }
            window::RESET => {
                buttons = e.buttons;
                app.menu = None;
                app.press = None;
                app.dragging = false;
                app.open_on_release = false;
                app.drop_row = None;
                app.modifiers = 0;
                true
            }
            window::POINTER => {
                let changed = app.pointer(e, buttons);
                buttons = e.buttons;
                changed
            }
            _ => false,
        };
        if app.quit {
            return 0;
        }
        if repaint {
            app.paint(w);
        }
    }
}
