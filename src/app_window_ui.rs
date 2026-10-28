use crate::{
    desktop_ui::{Rect, Window},
    mouse_packet, window_protocol as api,
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    Title,
    Close,
    Minimize,
    Maximize,
    Client,
    Resize,
}
#[derive(Clone, Copy)]
pub struct State {
    pub window: Window,
    pub x: i32,
    pub y: i32,
    pub focused: bool,
    pub minimizable: bool,
    pub buttons: u8,
    pub limits: Option<(i32, i32, i32, i32)>,
    preview: Option<Rect>,
    capture: Option<Capture>,
    offset: (i32, i32),
    maximized: Option<(Rect, i32, i32)>,
    placement: Option<(Rect, bool)>,
    title_click: Option<(u32, i32, i32)>,
    press_at: (i32, i32),
    dragged: bool,
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
            limits: None,
            preview: None,
            capture: None,
            offset: (0, 0),
            maximized: None,
            placement: None,
            title_click: None,
            press_at: (0, 0),
            dragged: false,
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
        if let Some((_, w, h)) = self.maximized {
            return Rect {
                x: r.x + (r.w - w) / 2,
                y: r.y + 28,
                w,
                h,
            };
        }
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
        if self.maximized.is_none() {
            r.x = r.x.clamp(0, (800 - r.w - 6).max(0));
            r.y = r.y.clamp(24, (594 - r.h).max(24));
        }
        self.capture = None;
        self.preview = None;
        self.title_click = None;
    }
    pub fn is_maximized(&self) -> bool {
        self.maximized.is_some()
    }
    pub fn title(&self) -> Rect {
        let mut title = self.window.title();
        if self.is_maximized() { title.h = 22; }
        title
    }
    pub fn maximize_button(&self) -> Rect {
        let mut r = self.window.close();
        r.x -= 24;
        r
    }
    pub fn minimize_button(&self) -> Rect {
        let mut r = self.window.minimize();
        if self.limits.is_some() {
            r.x -= 24;
        }
        r
    }
    pub fn maximize_pressed(&self) -> bool {
        self.capture == Some(Capture::Maximize) && self.maximize_button().contains(self.x, self.y)
    }
    fn toggle_maximize(&mut self) -> Option<api::Event> {
        // Reissue from the committed state if an earlier proposal was lost.
        // Repeated clicks cannot publish geometry before the app acknowledges it.
        let (a, b, w, h) = self.limits?;
        let (target, maximize, w, h) = if let Some((mut normal, _, _)) = self.maximized {
            let nw = (normal.w - 16).clamp(a, w);
            let nh = (normal.h - 38).clamp(b, h);
            normal.w = nw + 16;
            normal.h = nh + 38;
            (normal, false, nw, nh)
        } else {
            (
                Rect {
                    x: 0,
                    y: 24,
                    w: 800,
                    h: 576,
                },
                true,
                w.min(792),
                h.min(544),
            )
        };
        self.placement = Some((target, maximize));
        Some(api::Event {
            kind: api::RESIZE,
            x: w,
            y: h,
            ..api::Event::default()
        })
    }
    /// Publish placement only after the app accepts and surface allocation succeeds.
    pub fn resized(&mut self, w: i32, h: i32) {
        if let Some((rect, maximize)) = self.placement.take() {
            self.maximized = if maximize {
                Some((self.window.rect, w, h))
            } else {
                None
            };
            self.window.rect = rect;
        } else {
            self.window.rect.w = w + 16;
            self.window.rect.h = h + 38;
        }
        self.title_click = None;
    }
    pub fn cancel_resize(&mut self) {
        self.placement = None;
    }
    pub fn preview(&self) -> Option<Rect> {
        self.preview
    }
    pub fn grip(&self) -> Rect {
        let r = self.window.rect;
        Rect {
            x: r.x + r.w - 14,
            y: r.y + r.h - 14,
            w: 14,
            h: 14,
        }
    }
    pub fn minimize_pressed(&self) -> bool {
        self.capture == Some(Capture::Minimize) && self.minimize_button().contains(self.x, self.y)
    }
    pub fn close_pressed(&self) -> bool {
        self.capture == Some(Capture::Close) && self.window.close().contains(self.x, self.y)
    }
    pub fn update(&mut self, input: mouse_packet::Event) -> (bool, Option<api::Event>) {
        self.update_at(input, 0)
    }
    pub fn update_at(
        &mut self,
        input: mouse_packet::Event,
        now: u32,
    ) -> (bool, Option<api::Event>) {
        let old = (
            self.window,
            self.focused,
            self.close_pressed(),
            self.minimize_pressed(),
            self.maximize_pressed(),
            self.preview,
        );
        let captured = self.capture;
        self.x = (self.x + input.dx as i32).clamp(0, 799);
        self.y = (self.y + input.dy as i32).clamp(0, 599);
        let pressed = input.buttons & !self.buttons;
        let released = self.buttons & !input.buttons;
        self.buttons = input.buttons;
        if input.lost {
            self.capture = None;
            self.preview = None;
            self.title_click = None;
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
            } else if self.limits.is_some() && self.maximize_button().contains(self.x, self.y) {
                Some(Capture::Maximize)
            } else if self.minimizable && self.minimize_button().contains(self.x, self.y) {
                Some(Capture::Minimize)
            } else if self.limits.is_some()
                && !self.is_maximized()
                && self.placement.is_none()
                && self.grip().contains(self.x, self.y)
            {
                Some(Capture::Resize)
            } else if self.title().contains(self.x, self.y) {
                Some(Capture::Title)
            } else if self.client().contains(self.x, self.y) {
                Some(Capture::Client)
            } else {
                None
            };
            self.offset = (self.x - self.window.rect.x, self.y - self.window.rect.y);
            self.press_at = (self.x, self.y);
            self.dragged = false;
            if self.capture != Some(Capture::Title) {
                self.title_click = None;
            }
        }
        if input.buttons & 1 != 0 || released & 1 != 0 {
            if self.capture == Some(Capture::Title) {
                self.dragged |=
                    (self.x - self.press_at.0).abs() > 4 || (self.y - self.press_at.1).abs() > 4;
                if self.dragged && !self.is_maximized() && self.placement.is_none() {
                    let r = &mut self.window.rect;
                    r.x = (self.x - self.offset.0).clamp(32 - r.w, 768);
                    r.y = (self.y - self.offset.1).clamp(24, 568);
                }
            }
        }
        if released & 1 != 0
            && self.capture == Some(Capture::Minimize)
            && self.minimize_button().contains(self.x, self.y)
        {
            self.window.visible = false;
            self.focused = false;
            self.capture = None;
        }
        let mut resize = None;
        if released & 1 != 0 {
            if self.capture == Some(Capture::Maximize)
                && self.maximize_button().contains(self.x, self.y)
            {
                resize = self.toggle_maximize();
            } else if self.capture == Some(Capture::Title) {
                if !self.dragged && self.title().contains(self.x, self.y) {
                    let double = self.title_click.is_some_and(|(tick, x, y)| {
                        now.wrapping_sub(tick) <= 35
                            && (self.x - x).abs() <= 4
                            && (self.y - y).abs() <= 4
                    });
                    if double {
                        self.title_click = None;
                        resize = self.toggle_maximize();
                    } else {
                        self.title_click = Some((now, self.x, self.y));
                    }
                } else {
                    self.title_click = None;
                }
            }
        }
        if self.capture == Some(Capture::Resize) {
            if let Some((a, b, c, d)) = self.limits {
                let r = self.window.rect;
                let w = (((self.x - r.x + (r.w - self.offset.0) - 16 + 4) / 8) * 8).clamp(a, c);
                let h = (((self.y - r.y + (r.h - self.offset.1) - 38 + 8) / 16) * 16).clamp(b, d);
                self.preview = Some(Rect {
                    w: w + 16,
                    h: h + 38,
                    ..r
                });
                if released & 1 != 0 {
                    if w != r.w - 16 || h != r.h - 38 {
                        resize = Some(api::Event {
                            kind: api::RESIZE,
                            x: w,
                            y: h,
                            ..api::Event::default()
                        });
                    }
                    self.preview = None;
                }
            }
        }
        let client = self.client();
        let mut event = resize;
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
                self.maximize_pressed(),
                self.preview,
            ),
            event,
        )
    }
}
