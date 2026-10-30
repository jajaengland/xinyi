//! Shared control painting and text-field clipboard handling.
use super::{
    key,
    widgets::{Button, Rect, Scrollbar, TextInput},
    window::{self, Draw},
};
pub fn frame(w: u32, r: Rect, sunken: bool) {
    let (a, b) = if sunken {
        (0x8199ad, 0xffffff)
    } else {
        (0xffffff, 0x8199ad)
    };
    window::draw(
        w,
        &[
            Draw::rect(r.x as u32, r.y as u32, r.w as u32, r.h as u32, 0xe4ecf3),
            Draw::rect(r.x as u32, r.y as u32, r.w as u32, 1, a),
            Draw::rect(r.x as u32, r.y as u32, 1, r.h as u32, a),
            Draw::rect(r.x as u32, (r.y + r.h - 1) as u32, r.w as u32, 1, b),
            Draw::rect((r.x + r.w - 1) as u32, r.y as u32, 1, r.h as u32, b),
        ],
    );
}
pub fn button(w: u32, b: &Button, label: &[u8], focused: bool) {
    let pressed = b.pressed && b.hover;
    frame(w, b.rect, pressed);
    let d = i32::from(pressed);
    let r = b.rect;
    let width = super::text::width(label, super::text::Font::Ui).min((r.w - 16).max(0) as u32);
    window::draw(
        w,
        &[Draw::label(
            (r.x + (r.w - width as i32) / 2 + d) as u32,
            (r.y + (r.h - 16) / 2 + d) as u32,
            width,
            0,
            label,
        )],
    );
    if focused {
        window::draw(
            w,
            &[Draw::rect(
                (r.x + 4) as u32,
                (r.y + r.h - 4) as u32,
                (r.w - 8) as u32,
                1,
                0,
            )],
        );
    }
}
pub fn text_input<const N: usize>(w: u32, r: Rect, t: &TextInput<N>, focused: bool) {
    frame(w, r, true);
    window::draw(
        w,
        &[Draw::rect(
            (r.x + 2) as u32,
            (r.y + 2) as u32,
            (r.w - 4) as u32,
            (r.h - 4) as u32,
            0xffffff,
        )],
    );
    let cols = ((r.w - 12) / 8) as usize;
    let start = t.cursor.saturating_sub(cols.saturating_sub(1));
    let end = (start + cols).min(t.len);
    let y = (r.y + (r.h - 16) / 2) as u32;
    window::draw(
        w,
        &[Draw::text((r.x + 6) as u32, y, 0, &t.bytes[start..end])],
    );
    if focused {
        let (a, b) = t.range();
        let a = a.max(start);
        let b = b.min(end);
        if a < b {
            window::row(
                w,
                (r.x + 6) as u32 + (a - start) as u32 * 8,
                y,
                0x153e8e,
                0xffffff,
                &t.bytes[a..b],
            );
        }
        window::draw(
            w,
            &[Draw::rect(
                (r.x + 6) as u32 + (t.cursor - start) as u32 * 8,
                y,
                1,
                16,
                0,
            )],
        );
    }
}
pub fn edit_key<const N: usize>(t: &mut TextInput<N>, event: u32) -> bool {
    let code = event as u16;
    if event & key::CTRL != 0 && matches!(code, 86 | 118) {
        let mut bytes = [0; 1024];
        let n = super::clipboard_get(&mut bytes);
        return n >= 0 && t.insert(&bytes[..n as usize]);
    }
    t.key(event)
}
pub fn scrollbar(w: u32, s: &Scrollbar) {
    let r = s.rect;
    window::draw(
        w,
        &[Draw::rect(
            r.x as u32, r.y as u32, r.w as u32, r.h as u32, 0xe0e0e0,
        )],
    );
    for (y, label) in [(r.y, b"^" as &[u8]), (r.y + r.h - 16, b"v")] {
        let b = Button::new(Rect::new(r.x, y, r.w, 16));
        frame(w, b.rect, false);
        window::draw(w, &[Draw::text((r.x + 4) as u32, y as u32, 0, label)]);
    }
    frame(w, s.thumb(), false);
}
pub fn dialog(w: u32, title: &[u8], r: Rect) {
    let mut theme = super::appearance::Theme::DEFAULT;
    super::appearance::get(&mut theme);
    window::draw(
        w,
        &[Draw::rect(
            (r.x + 4) as u32,
            (r.y + 4) as u32,
            r.w as u32,
            r.h as u32,
            0x404040,
        )],
    );
    frame(w, r, false);
    window::draw(
        w,
        &[
            Draw::rect(
                (r.x + 3) as u32,
                (r.y + 3) as u32,
                (r.w - 6) as u32,
                22,
                theme.title,
            ),
            Draw::text(
                (r.x + 10) as u32,
                (r.y + 6) as u32,
                super::theme_model::ink(theme.title),
                title,
            ),
        ],
    );
}
