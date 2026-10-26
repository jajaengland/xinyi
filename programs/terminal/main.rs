#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::key;

// A terminal exercise, not a file editor: text stays on the screen only.
fn main() -> u32 {
    xinyi::clear_screen(0x17);
    xinyi::draw_text(0, 0, 0x70, b" Xinyi typing demo - arrows move, type overwrites, Escape quits ");
    xinyi::draw_text(0, 24, 0x70, b" Screen only: no file saving yet. ");
    let (mut x, mut y) = (0u8, 2u8);
    loop {
        xinyi::move_cursor(x as u16, y as u16);
        let event = xinyi::read_key(true) as u32;
        match event as u16 {
            27 => return 0,
            key::LEFT => x = x.saturating_sub(1),
            key::RIGHT => x = (x + 1).min(79),
            key::UP => y = y.saturating_sub(1).max(1),
            key::DOWN => y = (y + 1).min(23),
            key::HOME => x = 0,
            key::END => x = 79,
            10 => { x = 0; y = (y + 1).min(23); }
            8 | key::DELETE => {
                if event as u16 == 8 { x = x.saturating_sub(1); }
                xinyi::draw_text(x, y, 0x17, b" ");
            }
            code @ 32..=126 if event & (key::CTRL | key::ALT) == 0 => {
                xinyi::draw_text(x, y, 0x17, &[code as u8]);
                x = (x + 1).min(79);
            }
            _ => (),
        }
    }
}
