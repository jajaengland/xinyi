// Desktop chrome and captured menu/task selection; no hardware or allocation.
use crate::{app_catalog::Catalog, desktop_ui::Rect};
#[derive(Clone, Copy)]
pub struct Task {
    pub pid: u32,
    pub name: [u8; 8],
    pub length: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Apps,
    App(usize),
    Task(usize),
    Exit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Launch(usize),
    Focus(u32),
    Exit,
}
pub struct Shell {
    pub catalog: Catalog,
    pub tasks: [Option<Task>; 4],
    pub focused: Option<u32>,
    pub open: bool,
    pub capture: Option<Hit>,
    pub menu_selection: Option<usize>,
    pub x: i32,
    pub y: i32,
    buttons: u8,
    blocked: bool,
    pub message: [u8; 88],
    pub length: usize,
}
impl Shell {
    pub const fn new() -> Self {
        Self {
            catalog: Catalog::new(),
            tasks: [None; 4],
            focused: None,
            open: false,
            capture: None,
            menu_selection: None,
            x: 400,
            y: 300,
            buttons: 0,
            blocked: false,
            message: [0; 88],
            length: 0,
        }
    }
    pub fn status(&mut self, text: &[u8]) {
        self.length = text.len().min(self.message.len());
        self.message[..self.length].copy_from_slice(&text[..self.length]);
    }
    pub fn apps() -> Rect {
        Rect {
            x: 8,
            y: 3,
            w: 72,
            h: 18,
        }
    }
    pub fn task(index: usize) -> Rect {
        Rect {
            x: 88 + index as i32 * 148,
            y: 3,
            w: 140,
            h: 18,
        }
    }
    pub fn exit() -> Rect {
        Rect {
            x: 744,
            y: 3,
            w: 48,
            h: 18,
        }
    }
    pub fn menu(&self) -> Rect {
        Rect {
            x: 8,
            y: 30,
            w: 200,
            h: self.catalog.len.max(1) as i32 * 24 + 8,
        }
    }
    pub fn hit(&self) -> Option<Hit> {
        if Self::apps().contains(self.x, self.y) {
            return Some(Hit::Apps);
        }
        if self.open {
            let r = self.menu();
            if r.contains(self.x, self.y) {
                let i = ((self.y - r.y - 4) / 24).max(0) as usize;
                if self.y >= r.y + 4
                    && i < self.catalog.len
                    && self.y < r.y + 4 + (i + 1) as i32 * 24
                {
                    return Some(Hit::App(i));
                }
            }
            return None;
        }
        if Self::exit().contains(self.x, self.y) {
            return Some(Hit::Exit);
        }
        for i in 0..4 {
            if self.tasks[i].is_some() && Self::task(i).contains(self.x, self.y) {
                return Some(Hit::Task(i));
            }
        }
        None
    }
    pub fn reset(&mut self, buttons: u8) {
        self.open = false;
        self.capture = None;
        self.menu_selection = None;
        self.buttons = buttons;
        self.blocked = false;
    }
    /// Cycle in stable taskbar order, including minimized tasks and empty slots.
    pub fn next_task(&self, reverse: bool) -> Option<u32> {
        let current = self.tasks.iter().position(|task| {
            task.is_some_and(|task| Some(task.pid) == self.focused)
        });
        let start = current.unwrap_or(if reverse { 0 } else { 3 });
        for offset in 1..=4 {
            let index = if reverse { (start + 4 - offset) % 4 } else { (start + offset) % 4 };
            if let Some(task) = self.tasks[index] { return Some(task.pid); }
        }
        None
    }
    /// Global Apps shortcut and menu navigation consume keys before applications.
    pub fn key(&mut self, code: u32) -> (bool, Option<Action>) {
        let key = code as u16;
        if key == 27 && code & (1 << 17) != 0 && code & (1 << 18) == 0 {
            self.open = !self.open;
            self.capture = None;
            self.menu_selection = if self.open && self.catalog.len > 0 { Some(0) } else { None };
            return (true, None);
        }
        if !self.open { return (false, None); }
        match key {
            27 => { self.open = false; self.menu_selection = None; },
            0x100 | 0x101 | 0x104 | 0x105 if self.catalog.len > 0 => {
                let last = self.catalog.len - 1;
                self.menu_selection = Some(match key {
                    0x100 => self.menu_selection.map_or(last, |i| if i == 0 { last } else { i - 1 }),
                    0x101 => self.menu_selection.map_or(0, |i| (i + 1) % self.catalog.len),
                    0x104 => 0,
                    _ => last,
                });
            },
            10 => {
                self.open = false;
                return (true, self.menu_selection.take().filter(|&i| i < self.catalog.len).map(Action::Launch));
            },
            _ => (),
        }
        (true, None)
    }
    // Outside presses dismiss menus without clicking through to an application.
    pub fn input(
        &mut self,
        x: i32,
        y: i32,
        buttons: u8,
        lost: bool,
    ) -> (bool, Option<Action>, bool) {
        let previous = (self.open, self.capture, self.hit(), self.menu_selection);
        if (x, y) != (self.x, self.y) || buttons != self.buttons || lost {
            self.menu_selection = None;
        }
        self.x = x;
        self.y = y;
        let pressed = buttons & !self.buttons;
        let released = self.buttons & !buttons;
        self.buttons = buttons;
        let consumed = self.open || self.capture.is_some() || self.blocked || y < 24;
        if lost {
            self.capture = None;
            self.open = false;
            self.blocked = false;
            return (consumed, None, true);
        }
        let mut action = None;
        if pressed & 1 != 0 {
            self.blocked = consumed;
            self.capture = self.hit();
            if self.open && self.capture.is_none() {
                self.open = false;
            }
        }
        if released & 1 != 0 {
            if let Some(hit) = self.capture {
                if Some(hit) == self.hit() {
                    match hit {
                        Hit::Apps => self.open = !self.open,
                        Hit::App(i) => {
                            self.open = false;
                            action = Some(Action::Launch(i));
                        }
                        Hit::Task(i) => action = self.tasks[i].map(|t| Action::Focus(t.pid)),
                        Hit::Exit => action = Some(Action::Exit),
                    }
                }
            }
            self.capture = None;
            self.blocked = false;
        }
        (
            consumed,
            action,
            previous != (self.open, self.capture, self.hit(), self.menu_selection),
        )
    }
}
