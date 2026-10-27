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
        self.buttons = buttons;
        self.blocked = false;
    }
    // Outside presses dismiss menus without clicking through to an application.
    pub fn input(
        &mut self,
        x: i32,
        y: i32,
        buttons: u8,
        lost: bool,
    ) -> (bool, Option<Action>, bool) {
        let previous = (self.open, self.capture, self.hit());
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
            previous != (self.open, self.capture, self.hit()),
        )
    }
}
