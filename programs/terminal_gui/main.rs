#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::{
    gui, key, pty,
    widgets::{Rect, Scrollbar},
    window,
};
fn banner(window: u32, width: u32, text: &[u8]) {
    window::draw(
        window,
        &[
            window::Draw::rect(0, 0, width - 16, 32, 0xc4c4c4),
            window::Draw::text(8, 8, 0, text),
        ],
    );
    window::present(window);
}
fn main() -> u32 {
    let handle = window::open_sized(b"Terminal", 656, 400);
    if handle < 0 {
        return 1;
    }
    let window = handle as u32;
    window::resizable(window, 656, 400, 792, 544);
    let mut width = 656u32;
    let mut height = 400i32;
    window::draw(window, &[window::Draw::rect(0, 0, width, height as u32, 0)]);
    let session = pty::open();
    if session < 0 {
        return 1;
    }
    let session = session as u32;
    let (mut scroll, mut anchor, mut dragging, mut buttons, mut closing) =
        (0u32, 0u32, false, 0u32, false);
    let mut message: Option<&[u8]> = None;
    let mut bar = Scrollbar::new(Rect::new(640, 0, 16, 400));
    let mut painted = None;
    let mut last_scrolls = pty::control(session, 7).max(0) as u32;
    // Optional initial command, used when opening an executable from Files.
    let mut args = [0; 128];
    let n = xinyi::arguments(&mut args);
    if n > 0 {
        for &b in &args[..n as usize] {
            pty::key(session, b as u32);
        }
        pty::key(session, 10);
    }
    loop {
        if pty::control(session, 4) > 0 {
            return 0;
        }
        let history = pty::control(session, 6).max(0) as u32;
        let output_scrolls = pty::control(session, 7).max(0) as u32;
        if scroll > 0 {
            scroll = if output_scrolls >= last_scrolls {
                scroll
                    .saturating_add(output_scrolls - last_scrolls)
                    .min(history)
            } else {
                0
            };
        }
        last_scrolls = output_scrolls;
        bar.range(history, height as u32 / 16);
        scroll = scroll.min(history);
        bar.value = history.saturating_sub(scroll);
        for _ in 0..64 {
            let mut event = window::Event::default();
            let result = window::event(window, &mut event, false);
            if result < 0 {
                return 1;
            }
            if result == 0 {
                break;
            }
            match event.kind {
                window::RESIZE => {
                    if window::resize(window, event) == 0 {
                        width = event.x as u32;
                        height = event.y;
                        window::draw(window, &[window::Draw::rect(0, 0, width, height as u32, 0)]);
                        pty::viewport(session, height as u32 / 16);
                        bar.rect.x = width as i32 - 16;
                        bar.rect.h = height;
                        bar.reset(0);
                        scroll = 0;
                        dragging = false;
                        painted = None;
                    }
                }
                window::CLOSE => {
                    let result = pty::control(session, if closing { 2 } else { 1 });
                    if result == 0 {
                        return 0;
                    }
                    closing = true;
                    message = Some(b"Program running. X again terminates it. Escape cancels.");
                }
                window::RESET => {
                    dragging = false;
                    buttons = event.buttons;
                    bar.reset(event.buttons);
                }
                window::POINTER => {
                    let on_bar =
                        !dragging && (bar.rect.contains(event.x, event.y) || bar.dragging());
                    if bar.pointer(event.x, event.y, event.buttons) {
                        scroll = history - bar.value;
                        pty::clear_selection(session);
                    }
                    if on_bar {
                        dragging = false;
                        buttons = event.buttons;
                        continue;
                    }
                    // The PTY retains its compatible 80-column text screen.
                    // Extra client width is background, not additional cells.
                    let at = (event.y.clamp(0, height - 1) / 16) as u32 * 80
                        + (event.x.clamp(0, 639) / 8) as u32;
                    if event.x >= 640 && !dragging {
                        buttons = event.buttons;
                        continue;
                    }
                    if event.buttons & 1 != 0 && buttons & 1 == 0 {
                        anchor = at;
                        dragging = true;
                        pty::select(session, anchor, at);
                    } else if dragging {
                        pty::select(session, anchor, at);
                    }
                    if event.buttons & 1 == 0 {
                        dragging = false;
                    }
                    buttons = event.buttons;
                }
                window::KEY => {
                    let code = event.code as u16;
                    let ctrl = event.code & key::CTRL != 0;
                    let shift = event.code & key::SHIFT != 0;
                    if message.take().is_some() {
                        closing = false;
                        pty::control(session, 5);
                        if code == 27 {
                            continue;
                        }
                    }
                    if ctrl && shift && matches!(code, 67 | 99) {
                        if pty::copy(session) < 0 {
                            message = Some(
                                b"Selection exceeds the 1024-byte clipboard. Escape dismisses.",
                            );
                        }
                        continue;
                    }
                    if ctrl && matches!(code, 86 | 118) {
                        let mut text = [0; 1024];
                        let n = xinyi::clipboard_get(&mut text);
                        if n > 0 && pty::paste(session, &text[..n as usize]) < 0 {
                            message =
                                Some(b"Paste does not fit the input buffer. Escape dismisses.");
                        }
                        continue;
                    }
                    if shift && matches!(code, key::PAGE_UP | key::PAGE_DOWN) {
                        pty::clear_selection(session);
                        if code == key::PAGE_UP {
                            scroll = (scroll + 12).min(history);
                        } else {
                            scroll = scroll.saturating_sub(12);
                        }
                        continue;
                    }
                    if ctrl && matches!(code, 81 | 113) && pty::control(session, 3) == 0 {
                        if pty::control(session, 1) == 0 {
                            return 0;
                        }
                        closing = true;
                        message = Some(b"Program starting. X terminates it. Escape cancels.");
                        continue;
                    }
                    pty::clear_selection(session);
                    scroll = 0;
                    pty::key(session, event.code);
                }
                _ => (),
            }
        }
        if let Some(text) = message {
            banner(window, width, text);
        } else if pty::present(session, window, scroll) < 0 {
            return 0;
        }
        bar.value = history.saturating_sub(scroll);
        let state = (bar.value, bar.max, bar.dragging());
        if painted != Some(state) {
            gui::scrollbar(window, &bar);
            window::present(window);
            painted = Some(state);
        }
        xinyi::sleep_ms(10);
    }
}
