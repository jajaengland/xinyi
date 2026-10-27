// Hardware-independent hit testing and press/release capture for the mouse test.
use crate::mouse_packet::Event;
pub const MENU_W: i32 = 180;
pub const MENU_H: i32 = 76;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Button(usize),
    Menu(usize),
}
pub struct State {
    pub x: i32,
    pub y: i32,
    pub buttons: u8,
    pub menu: Option<(i32, i32)>,
    pub capture: Option<Target>,
    pub clicks: [u32; 2],
    pub last: Option<usize>,
}
impl State {
    pub fn new(buttons: u8) -> Self {
        Self {
            x: 400,
            y: 300,
            buttons,
            menu: None,
            capture: None,
            clicks: [0; 2],
            last: None,
        }
    }
    pub fn hit(&self) -> Option<Target> {
        if let Some((x, y)) = self.menu {
            if inside(self.x, self.y, x + 4, y + 4, MENU_W - 8, 28) {
                return Some(Target::Menu(0));
            }
            if inside(self.x, self.y, x + 4, y + 40, MENU_W - 8, 28) {
                return Some(Target::Menu(1));
            }
            return None;
        }
        for i in 0..2 {
            if inside(self.x, self.y, 260 + i as i32 * 160, 240, 120, 44) {
                return Some(Target::Button(i));
            }
        }
        None
    }
    pub fn update(&mut self, event: Event) -> bool {
        let before = (self.menu, self.capture, self.clicks, self.last);
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
            self.menu = Some((self.x.min(800 - MENU_W), self.y.min(600 - MENU_H)));
            self.capture = None;
        } else {
            if pressed & 1 != 0 {
                self.capture = self.hit();
                // Dismiss an outside menu click without activating controls below.
                if self.menu.is_some() && self.capture.is_none() {
                    self.menu = None;
                }
            }
            if released & 1 != 0 {
                if let Some(target) = self.capture {
                    if Some(target) == self.hit() {
                        match target {
                            Target::Button(i) => {
                                self.clicks[i] = self.clicks[i].saturating_add(1);
                                self.last = Some(i);
                            }
                            Target::Menu(0) => {
                                self.clicks = [0; 2];
                                self.last = None;
                                self.menu = None;
                            }
                            Target::Menu(_) => self.menu = None,
                        }
                    }
                }
                self.capture = None;
            }
        }
        before != (self.menu, self.capture, self.clicks, self.last)
            || ((self.capture.is_some() || self.menu.is_some()) && old_hit != self.hit())
    }
}
fn inside(px: i32, py: i32, x: i32, y: i32, w: i32, h: i32) -> bool {
    px >= x && py >= y && px < x + w && py < y + h
}
