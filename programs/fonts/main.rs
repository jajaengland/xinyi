#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::{
    gui,
    text::{self, Font},
    widgets::{Button, Rect},
    window::{self, Draw},
};
const SAMPLE: &[u8] = b"Illi 0123456789 | Wide words, tiny letters.";
const PARAGRAPH:&[u8]=b"Text wraps at word boundaries. LongWordsWithoutSpacesAlsoWrapSafely.\nNew lines stay new lines.";
fn paint(w: u32, width: u32, height: u32, box_width: u32, buttons: &[Button; 2]) {
    assert_eq!(window::draw(
        w,
        &[
            Draw::rect(0, 0, width, height, 0xf4f4f4),
            Draw::label(
                16,
                12,
                width - 32,
                0,
                b"Xinyi text: measured, wrapped and clipped",
            ),
            Draw::label(
                16,
                44,
                width - 32,
                0x505050,
                b"Fixed-width editing / proportional interface",
            ),
            Draw::text(16, 68, 0, SAMPLE),
            Draw::rect(16, 86, text::width(SAMPLE, Font::Mono), 1, 0x3273bb),
            Draw::label(16, 100, width - 32, 0, SAMPLE),
            Draw::rect(16, 118, text::width(SAMPLE, Font::Ui), 1, 0x3273bb),
            Draw::label(
                16,
                136,
                width - 32,
                0x505050,
                b"Word wrapping: +/- adjusts the box, Space toggles width",
            ),
            Draw::rect(16, 160, box_width, 112, 0xffffff),
            Draw::rect(16 + box_width, 160, 2, 112, 0x3273bb),
            Draw::paragraph(24, 168, box_width - 16, 96, 0, PARAGRAPH),
            Draw::label(
                16,
                288,
                width - 32,
                0x505050,
                b"Long labels end with dots and stay inside their bounds:",
            ),
            Draw::rect(16, 312, 128, 20, 0xdbe9f8),
            Draw::label(
                20,
                314,
                120,
                0,
                b"This application name is much too long for this box",
            ),
        ],
    ),14);
    for (b, label) in buttons.iter().zip([b"Toggle width" as &[u8], b"Close"]) {
        gui::button(w, b, label, false);
    }
    window::draw(
        w,
        &[Draw::label(
            16,
            height - 28,
            width - 32,
            0x505050,
            b"Resize or maximise this window. Escape closes it.",
        )],
    );
    window::present(w);
}
fn main() -> u32 {
    let w = window::open_sized(b"Fonts and graphics", 640, 432);
    if w < 0 {
        return 1;
    }
    let w = w as u32;
    window::resizable(w, 384, 432, 792, 544);
    let (mut width, mut height, mut box_width) = (640, 432, 248);
    let mut buttons = [
        Button::new(Rect::new(16, 352, 160, 28)),
        Button::new(Rect::new(192, 352, 96, 28)),
    ];
    paint(w, width, height, box_width, &buttons);
    loop {
        let mut e = window::Event::default();
        if window::event(w, &mut e, true) < 0 {
            return 1;
        }
        match e.kind {
            window::CLOSE => return 0,
            window::KEY => match e.code as u16 {
                27 | 113 => return 0,
                32 => {
                    box_width = if box_width < width - 32 {
                        width - 32
                    } else {
                        160
                    }
                }
                43 | 61 => box_width = (box_width + 16).min(width - 32),
                45 => box_width = box_width.saturating_sub(16).max(64),
                _ => continue,
            },
            window::RESIZE => {
                if window::resize(w, e) == 0 {
                    let full=box_width==width-32;
                    width = e.x as u32;
                    height = e.y as u32;
                    box_width = if full {width-32}else{box_width.min(width - 32)};
                }
            }
            window::RESET => {
                for b in &mut buttons {
                    b.reset(e.buttons);
                }
            }
            window::POINTER => {
                let mut action = None;
                for (i, b) in buttons.iter_mut().enumerate() {
                    if b.pointer(e.x, e.y, e.buttons) {
                        action = Some(i);
                    }
                }
                match action {
                    Some(0) => {
                        box_width = if box_width < width - 32 {
                            width - 32
                        } else {
                            160
                        }
                    }
                    Some(1) => return 0,
                    _ => (),
                }
            }
            _ => continue,
        }
        paint(w, width, height, box_width, &buttons);
    }
}
