// Fixed-size window manager model. No hardware, heap or application execution.
use crate::mouse_packet::Event;
pub const MENU_W: i32 = 180;
pub const MENU_H: i32 = 76;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            w: (self.x + self.w).max(other.x + other.w) - x,
            h: (self.y + self.h).max(other.y + other.h) - y,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub rect: Rect,
    pub visible: bool,
}
impl Window {
    pub fn bounds(self) -> Rect {
        Rect {
            w: self.rect.w + 6,
            h: self.rect.h + 6,
            ..self.rect
        }
    }
    pub fn title(self) -> Rect {
        Rect {
            x: self.rect.x + 4,
            y: self.rect.y + 4,
            w: self.rect.w - 8,
            h: 26,
        }
    }
    pub fn minimize(self) -> Rect {let mut r=self.close();r.x-=24;r}
    pub fn close(self) -> Rect {
        Rect {
            x: self.rect.x + self.rect.w - 26,
            y: self.rect.y + 7,
            w: 18,
            h: 18,
        }
    }
    pub fn button(self, index: usize) -> Rect {
        Rect {
            x: self.rect.x + 24 + index as i32 * 160,
            y: self.rect.y + 80,
            w: 128,
            h: 36,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Title(usize),
    Close(usize),
    Client(usize),
    Button(usize),
    Menu(usize),
    AppsButton,
    App(usize),
}
impl Target {
    fn window(self) -> Option<usize> {
        match self {
            Self::Title(i) | Self::Close(i) | Self::Client(i) => Some(i),
            Self::Button(_) => Some(0),
            _ => None,
        }
    }
}
#[derive(Clone, Copy)]
pub struct State {
    pub x: i32,
    pub y: i32,
    pub buttons: u8,
    pub windows: [Window; 2],
    pub order: [usize; 2],
    pub focus: Option<usize>,
    pub capture: Option<Target>,
    pub menu: Option<(i32, i32)>,
    pub clicks: [u32; 2],
    pub apps_open: bool,
    pub app_count: usize,
    pub launch: Option<usize>,
    drag_offset: (i32, i32),
}
impl State {
    pub fn new(buttons: u8) -> Self {
        Self {
            x: 400,
            y: 300,
            buttons,
            windows: Self::initial_windows(),
            order: [0, 1],
            focus: Some(1),
            capture: None,
            menu: None,
            clicks: [0; 2],
            apps_open: false,
            app_count: 0,
            launch: None,
            drag_offset: (0, 0),
        }
    }
    fn initial_windows() -> [Window; 2] {
        [
            Window {
                rect: Rect {
                    x: 100,
                    y: 140,
                    w: 360,
                    h: 230,
                },
                visible: true,
            },
            Window {
                rect: Rect {
                    x: 300,
                    y: 210,
                    w: 340,
                    h: 210,
                },
                visible: true,
            },
        ]
    }
    pub fn apps_button() -> Rect {
        Rect {
            x: 8,
            y: 3,
            w: 72,
            h: 18,
        }
    }
    pub fn apps_menu(&self) -> Rect {
        let h = self.app_count.clamp(1, 16) as i32 * 24 + 8;
        Rect {
            x: 8,
            y: 30,
            w: 200,
            h,
        }
    }
    pub fn hit(&self) -> Option<Target> {
        if self.apps_open {
            let menu = self.apps_menu();
            if menu.contains(self.x, self.y) {
                for i in 0..self.app_count.min(16) {
                    if (Rect {
                        x: menu.x + 4,
                        y: menu.y + 4 + i as i32 * 24,
                        w: menu.w - 8,
                        h: 24,
                    })
                    .contains(self.x, self.y)
                    {
                        return Some(Target::App(i));
                    }
                }
                return None;
            }
            return if Self::apps_button().contains(self.x, self.y) {
                Some(Target::AppsButton)
            } else {
                None
            };
        }
        if let Some((x, y)) = self.menu {
            for i in 0..2 {
                if (Rect {
                    x: x + 4,
                    y: y + 4 + i as i32 * 36,
                    w: MENU_W - 8,
                    h: 28,
                })
                .contains(self.x, self.y)
                {
                    return Some(Target::Menu(i));
                }
            }
            return None;
        }
        if Self::apps_button().contains(self.x, self.y) {
            return Some(Target::AppsButton);
        }
        for &id in self.order.iter().rev() {
            let window = self.windows[id];
            if !window.visible || !window.rect.contains(self.x, self.y) {
                continue;
            }
            if window.close().contains(self.x, self.y) {
                return Some(Target::Close(id));
            }
            if window.title().contains(self.x, self.y) {
                return Some(Target::Title(id));
            }
            if id == 0 {
                for i in 0..2 {
                    if window.button(i).contains(self.x, self.y) {
                        return Some(Target::Button(i));
                    }
                }
            }
            // An opaque client area blocks hit testing of covered windows.
            return Some(Target::Client(id));
        }
        None
    }
    fn raise(&mut self, id: usize) {
        self.order = [1 - id, id];
        self.focus = Some(id);
    }
    pub fn update(&mut self, event: Event) -> bool {
        let before = (
            self.windows,
            self.order,
            self.focus,
            self.capture,
            self.menu,
            self.clicks,
            self.apps_open,
            self.launch,
        );
        let old_hit = self.hit();
        self.x = (self.x + event.dx as i32).clamp(0, 799);
        self.y = (self.y + event.dy as i32).clamp(0, 599);
        if event.lost {
            self.capture = None;
            self.buttons = event.buttons;
            return true;
        }
        let pressed = event.buttons & !self.buttons;
        let released = self.buttons & !event.buttons;
        self.buttons = event.buttons;
        if pressed & 2 != 0 {
            self.apps_open = false;
            self.menu = Some((self.x.min(800 - MENU_W - 6), self.y.clamp(30, 600 - MENU_H - 6)));
            self.capture = None;
        } else {
            if pressed & 1 != 0 {
                let target = self.hit();
                self.capture = target;
                if self.apps_open {
                    if target.is_none() {
                        self.apps_open = false;
                    }
                } else if self.menu.is_some() {
                    if target.is_none() {
                        self.menu = None;
                    }
                } else if let Some(id) = target.and_then(Target::window) {
                    self.raise(id);
                    if let Some(Target::Title(_)) = target {
                        let r = self.windows[id].rect;
                        self.drag_offset = (self.x - r.x, self.y - r.y);
                    }
                } else if target != Some(Target::AppsButton) {
                    self.focus = None;
                }
            }
            if event.buttons & 1 != 0 || released & 1 != 0 {
                if let Some(Target::Title(id)) = self.capture {
                    let r = &mut self.windows[id].rect;
                    r.x = (self.x - self.drag_offset.0).clamp(0, 800 - r.w - 6);
                    r.y = (self.y - self.drag_offset.1).clamp(24, 594 - r.h);
                }
            }
            if released & 1 != 0 {
                if let Some(target) = self.capture {
                    if Some(target) == self.hit() {
                        match target {
                            Target::AppsButton => {
                                self.apps_open = !self.apps_open;
                                self.menu = None;
                            }
                            Target::App(index) => {
                                self.launch = Some(index);
                                self.apps_open = false;
                            }
                            Target::Close(id) => {
                                self.windows[id].visible = false;
                                self.focus = self
                                    .order
                                    .iter()
                                    .rev()
                                    .copied()
                                    .find(|&i| self.windows[i].visible);
                            }
                            Target::Button(i) => self.clicks[i] = self.clicks[i].saturating_add(1),
                            Target::Menu(0) => {
                                self.windows = Self::initial_windows();
                                self.order = [0, 1];
                                self.focus = Some(1);
                                self.menu = None;
                            }
                            Target::Menu(_) => self.menu = None,
                            _ => (),
                        }
                    }
                }
                self.capture = None;
            }
        }
        before
            != (
                self.windows,
                self.order,
                self.focus,
                self.capture,
                self.menu,
                self.clicks,
                self.apps_open,
                self.launch,
            )
            || ((self.capture.is_some() || self.menu.is_some() || self.apps_open) && old_hit != self.hit())
    }
}
