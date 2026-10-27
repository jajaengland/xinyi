use crate::{
    desktop_ui::{Rect, Window},
    mouse_packet, window_protocol as api,
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    Title,
    Close,
    Minimize,
    Client,
}
#[derive(Clone, Copy)]
pub struct State {
    pub window: Window,
    pub x: i32,
    pub y: i32,
    pub focused: bool,
    pub minimizable: bool,
    pub buttons: u8,
    capture: Option<Capture>,
    offset: (i32, i32),
}
impl State {
    pub fn new(buttons: u8) -> Self {
        Self {
            window: Window {
                rect: Rect {
                    x: 72,
                    y: 88,
                    w: 656,
                    h: 438,
                },
                visible: true,
            },
            x: 400,
            y: 300,
            focused: true,
            minimizable: false,
            buttons,
            capture: None,
            offset: (0, 0),
        }
    }
    pub fn sized(buttons: u8, width: i32, height: i32, x: i32, y: i32) -> Self {
        let mut state = Self::new(buttons);
        state.window.rect = Rect {
            x,
            y,
            w: width + 16,
            h: height + 38,
        };
        state
    }
    pub fn client(&self) -> Rect {
        let r = self.window.rect;
        Rect {
            x: r.x + 8,
            y: r.y + 34,
            w: r.w - 16,
            h: r.h - 38,
        }
    }
    pub fn restore(&mut self) {
        self.window.visible = true;
        let r = &mut self.window.rect;
        r.x = r.x.clamp(0, 800 - r.w - 6);
        r.y = r.y.clamp(24, 594 - r.h);
        self.capture = None;
    }
    pub fn minimize_pressed(&self) -> bool {
        self.capture == Some(Capture::Minimize) && self.window.minimize().contains(self.x, self.y)
    }
    pub fn close_pressed(&self) -> bool {
        self.capture == Some(Capture::Close) && self.window.close().contains(self.x, self.y)
    }
    pub fn update(&mut self, input: mouse_packet::Event) -> (bool, Option<api::Event>) {
        let old = (
            self.window,
            self.focused,
            self.close_pressed(),
            self.minimize_pressed(),
        );
        let captured = self.capture;
        self.x = (self.x + input.dx as i32).clamp(0, 799);
        self.y = (self.y + input.dy as i32).clamp(0, 599);
        let pressed = input.buttons & !self.buttons;
        let released = self.buttons & !input.buttons;
        self.buttons = input.buttons;
        if input.lost {
            self.capture = None;
            return (
                true,
                Some(api::Event {
                    kind: api::RESET,
                    buttons: input.buttons as u32,
                    ..api::Event::default()
                }),
            );
        }
        if pressed & 1 != 0 {
            self.focused = self.window.rect.contains(self.x, self.y);
            self.capture = if self.window.close().contains(self.x, self.y) {
                Some(Capture::Close)
            } else if self.minimizable && self.window.minimize().contains(self.x, self.y) {
                Some(Capture::Minimize)
            } else if self.window.title().contains(self.x, self.y) {
                Some(Capture::Title)
            } else if self.client().contains(self.x, self.y) {
                Some(Capture::Client)
            } else {
                None
            };
            self.offset = (self.x - self.window.rect.x, self.y - self.window.rect.y);
        }
        if input.buttons & 1 != 0 || released & 1 != 0 {
            if self.capture == Some(Capture::Title) {
                let r = &mut self.window.rect;
                r.x = (self.x - self.offset.0).clamp(32 - r.w, 768);
                r.y = (self.y - self.offset.1).clamp(24, 568);
            }
        }
        if released & 1 != 0
            && self.capture == Some(Capture::Minimize)
            && self.window.minimize().contains(self.x, self.y)
        {
            self.window.visible = false;
            self.focused = false;
            self.capture = None;
        }
        let client = self.client();
        let mut event = None;
        if released & 1 != 0
            && self.capture == Some(Capture::Close)
            && self.window.close().contains(self.x, self.y)
        {
            event = Some(api::Event {
                kind: api::CLOSE,
                ..api::Event::default()
            });
        } else if self.capture == Some(Capture::Client)
            || (client.contains(self.x, self.y)
                && self.focused
                && captured.is_none()
                && self.capture.is_none())
        {
            event = Some(api::Event {
                kind: api::POINTER,
                x: self.x - client.x,
                y: self.y - client.y,
                buttons: input.buttons as u32,
                ..api::Event::default()
            });
        }
        if released & 1 != 0 {
            self.capture = None;
        }
        (
            old != (
                self.window,
                self.focused,
                self.close_pressed(),
                self.minimize_pressed(),
            ),
            event,
        )
    }
}
