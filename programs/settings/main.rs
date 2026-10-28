#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::{
    appearance::{self, Theme},
    gui, key,
    widgets::{Button, Rect, TextInput},
    window::{self, Draw},
};
const COLORS: [u32; 16] = [
    0x204a9c, 0x153e8e, 0x3268b5, 0x4b80ba, 0x277d85, 0x28634f, 0x626184, 0x854a73, 0x40464f,
    0x707070, 0x969da5, 0xc4c4c4, 0xe8e8e8, 0x805534, 0x88473e, 0x193653,
];
struct Settings {
    draft: Theme,
    target: u8,
    hex: TextInput<6>,
    buttons: [Button; 4],
    editing: bool,
    status: &'static [u8],
}
impl Settings {
    fn color(&self) -> u32 {
        if self.target == 0 {
            self.draft.desktop
        } else {
            self.draft.title
        }
    }
    fn choose(&mut self, color: u32) {
        if self.target == 0 {
            self.draft.desktop = color;
        } else {
            self.draft.title = color;
        }
        self.editing = false;
        self.hex.set(b"");
        self.status = b"Apply previews; Save persists on the mounted drive.";
    }
    fn apply(&mut self, save: bool) {
        let n = appearance::apply(&self.draft, save);
        self.status = match n {
            0 if save => b"Saved /GUI.CFG on the current drive.",
            0 => b"Applied for this session. Save to keep after reboot.",
            -13 => b"GUI.CFG is read-only. Changes were not saved.",
            -19 => b"No mounted drive. Apply works without saving.",
            _ => b"Could not save settings. Current colours retained.",
        };
    }
    fn paint(&self, w: u32) {
        window::draw(
            w,
            &[
                Draw::rect(0, 0, 480, 320, 0xf4f4f4),
                Draw::text(16, 12, 0, b"Appearance"),
                Draw::text(16, 42, 0, b"Desktop background"),
                Draw::text(16, 78, 0, b"Active title bar"),
            ],
        );
        for (i, color) in [self.draft.desktop, self.draft.title].iter().enumerate() {
            let y = 36 + i as u32 * 36;
            window::draw(
                w,
                &[
                    Draw::rect(
                        284,
                        y,
                        180,
                        28,
                        if self.target == i as u8 {
                            0x153e8e
                        } else {
                            0x707070
                        },
                    ),
                    Draw::rect(288, y + 4, 172, 20, *color),
                ],
            );
        }
        for (i, color) in COLORS.iter().enumerate() {
            let x = 16 + (i % 8) as u32 * 56;
            let y = 120 + (i / 8) as u32 * 40;
            window::draw(
                w,
                &[
                    Draw::rect(
                        x,
                        y,
                        48,
                        32,
                        if *color == self.color() { 0 } else { 0xc4c4c4 },
                    ),
                    Draw::rect(x + 3, y + 3, 42, 26, *color),
                ],
            );
        }
        let encoded = self.draft.encode();
        let text = if self.editing {
            &self.hex.bytes[..self.hex.len]
        } else if self.target == 0 {
            &encoded[..6]
        } else {
            &encoded[7..13]
        };
        window::draw(
            w,
            &[
                Draw::text(16, 212, 0, b"RGB hex:"),
                Draw::text(232, 212, 0, b"Enter 6 digits; Enter accepts"),
            ],
        );
        let mut display = TextInput::<6>::new();
        display.set(text);
        gui::text_input(
            w,
            Rect::new(96, 206, 120, 28),
            if self.editing { &self.hex } else { &display },
            self.editing,
        );
        for (i, label) in [b"Apply" as &[u8], b"Save", b"Defaults", b"Close"]
            .iter()
            .enumerate()
        {
            gui::button(w, &self.buttons[i], label, false);
        }
        window::draw(
            w,
            &[Draw::text(
                16,
                296,
                0,
                &self.status[..self.status.len().min(56)],
            )],
        );
        window::present(w);
    }
}
fn main() -> u32 {
    let w = window::open_sized(b"Settings", 480, 320);
    if w < 0 {
        return 1;
    }
    let w = w as u32;
    let mut theme = Theme::DEFAULT;
    appearance::get(&mut theme);
    let mut s = Settings {
        draft: theme,
        target: 0,
        hex: TextInput::new(),
        buttons: [
            Button::new(Rect::new(16, 252, 96, 28)),
            Button::new(Rect::new(128, 252, 96, 28)),
            Button::new(Rect::new(240, 252, 96, 28)),
            Button::new(Rect::new(352, 252, 96, 28)),
        ],
        editing: false,
        status: b"Choose a colour target, then a swatch or RGB value.",
    };
    let mut buttons = 0;
    s.paint(w);
    loop {
        let mut e = window::Event::default();
        if window::event(w, &mut e, true) < 0 {
            return 1;
        }
        let mut repaint = false;
        match e.kind {
            window::CLOSE => return 0,
            window::RESET => {
                buttons = e.buttons;
                for b in &mut s.buttons {
                    b.reset(e.buttons);
                }
            }
            window::KEY => {
                let code = e.code as u16;
                if code == 27 {
                    if s.editing {
                        s.editing = false;
                    } else {
                        return 0;
                    }
                } else if e.code & key::CTRL != 0 && matches!(code, 83 | 115) {
                    s.apply(true);
                } else if s.editing {
                    if code == 10 {
                        if let Some(color) = appearance::hex(&s.hex.bytes[..s.hex.len]) {
                            s.choose(color);
                        } else {
                            s.status = b"Enter exactly six hexadecimal digits.";
                        }
                    } else {
                        gui::edit_key(&mut s.hex, e.code);
                    }
                }
                repaint = true;
            }
            window::POINTER => {
                if e.buttons & 1 != 0 && buttons & 1 == 0 {
                    if (30..104).contains(&e.y) {
                        s.target = if e.y < 70 { 0 } else { 1 };
                        s.editing = false;
                    } else if (120..192).contains(&e.y) && (16..464).contains(&e.x) {
                        let col = (e.x - 16) / 56;
                        let row = (e.y - 120) / 40;
                        if col < 8 && (e.x - 16) % 56 < 48 && (e.y - 120) % 40 < 32 {
                            s.choose(COLORS[(row * 8 + col) as usize]);
                        }
                    } else if (96..216).contains(&e.x) && (206..234).contains(&e.y) {
                        s.editing = true;
                        s.hex.set(b"");
                    }
                }
                let old_pressed = s
                    .buttons
                    .iter()
                    .fold(0u32, |n, b| (n << 1) | u32::from(b.pressed && b.hover));
                let mut action = None;
                for (i, b) in s.buttons.iter_mut().enumerate() {
                    if b.pointer(e.x, e.y, e.buttons) {
                        action = Some(i);
                    }
                }
                if let Some(i) = action {
                    match i {
                        0 => s.apply(false),
                        1 => s.apply(true),
                        2 => {
                            s.draft = Theme::DEFAULT;
                            s.editing = false;
                            s.status = b"Defaults selected. Apply or Save to use them.";
                        }
                        3 => return 0,
                        _ => (),
                    }
                }
                let new_pressed = s
                    .buttons
                    .iter()
                    .fold(0u32, |n, b| (n << 1) | u32::from(b.pressed && b.hover));
                repaint = (e.buttons & 1 != 0 && buttons & 1 == 0)
                    || action.is_some()
                    || old_pressed != new_pressed;
                buttons = e.buttons;
            }
            _ => (),
        }
        if repaint {
            s.paint(w);
        }
    }
}
