// Foreground GUI services. No user framebuffer mapping; validate/copy every
// command and text before touching the owned client surface. IRQs only queue input.
use crate::canvas::Canvas;
use crate::{
    app_window_ui::State, graphics, loader::Frames, user_memory as user_buffer, usermode, window_protocol as api,
};
use core::arch::asm;
struct App {
    id: u32,
    owner: u32,
    ui: State,
    title: [u8; 48],
    title_len: usize,
    surface: Frames,
    dirty: u32,
    width: u32,
    height: u32,
}
static mut APP: Option<App> = None;
static mut NEXT: u32 = 1;
pub fn active() -> bool {
    unsafe { (*core::ptr::addr_of!(APP)).is_some() }
}
pub fn finish() {
    let app = unsafe { (*core::ptr::addr_of_mut!(APP)).take() };
    if app.is_some() {
        graphics::restore();
        crate::mouse::discard();
        crate::debug::write_str("Window: closed and text restored\n");
    }
    drop(app);
}
fn open(title: &[u8], width: u32, height: u32) -> u32 {
    if active() {
        return (-16i32) as u32;
    }
    let id = unsafe { NEXT };
    if id > i32::MAX as u32 {
        return (-24i32) as u32;
    }
    let Some(surface) = Frames::new((width as usize * height as usize * 4).div_ceil(4096)) else {
        return (-12i32) as u32;
    };
    let _ = crate::mouse::init();
    if let Err(message) = graphics::start() {
        crate::debug::write_fmt(format_args!("Window: {}\n", message));
        return (if message.contains("memory") {
            -12i32
        } else {
            -19i32
        }) as u32;
    }
    unsafe {
        core::slice::from_raw_parts_mut(surface.base() as *mut u32, (width * height) as usize)
            .fill(0xffffff);
    }
    let mut app = App {
        id,
        owner: usermode::getpid(),
        ui: State::sized(crate::mouse::discard(), width as i32, height as i32, 72, 88),
        title: [0; 48],
        title_len: title.len(),
        surface,
        dirty: 0,
        width,
        height,
    };
    app.title[..title.len()].copy_from_slice(title);
    compose(&app);
    graphics::present_rect(0, 0, 800, 600);
    graphics::pointer(app.ui.x, app.ui.y);
    unsafe {
        NEXT += 1;
        *core::ptr::addr_of_mut!(APP) = Some(app);
    }
    crate::debug::write_fmt(format_args!(
        "Window: opened handle={} client={}x{} surface={:08x}\n",
        id,
        width,
        height,
        unsafe {
            (&*core::ptr::addr_of!(APP))
                .as_ref()
                .unwrap()
                .surface
                .base()
        }
    ));
    id
}
fn compose(app: &App) {
    let surface = unsafe {
        core::slice::from_raw_parts(
            app.surface.base() as *const u32,
            (app.width * app.height) as usize,
        )
    };
    graphics::app_scene(&app.ui, &app.title[..app.title_len], surface);
}
fn input(app: &mut App, wait: bool) -> Option<api::Event> {
    loop {
        for _ in 0..256 {
            let keyboard = crate::keyboard::next_order();
            let mouse = crate::mouse::next_order();
            let pointer = crate::mouse::lost()
                || match (keyboard, mouse) {
                    (Some(k), Some(m)) => crate::input_order::before(m, k),
                    (None, Some(_)) => true,
                    _ => false,
                };
            if !pointer {
                if keyboard.is_none() {
                    break;
                }
                if let Some(code) = crate::keyboard::poll_event() {
                    if app.ui.focused {
                        return Some(api::Event {
                            kind: api::KEY,
                            code,
                            ..api::Event::default()
                        });
                    }
                }
                continue;
            }
            let Some(mouse) = crate::mouse::poll() else {
                continue;
            };
            let old = app.ui;
            let (mut changed, mut event) = app.ui.update(mouse);
            if !mouse.lost && mouse.buttons == old.buttons {
                for _ in 0..63 {
                    if !crate::mouse::motion_before(mouse.buttons, crate::keyboard::next_order()) {
                        break;
                    }
                    let (update, next) = app.ui.update(crate::mouse::poll().unwrap());
                    changed |= update;
                    event = next;
                }
            }
            unsafe {
                asm!("sti", options(nostack));
            }
            if changed {
                compose(app);
            }
            graphics::move_pointer((old.x, old.y), (app.ui.x, app.ui.y));
            if changed {
                let damage = old.window.bounds().union(app.ui.window.bounds());
                graphics::present_guarded(
                    damage.x,
                    damage.y,
                    damage.w,
                    damage.h,
                    (app.ui.x, app.ui.y),
                );
            }
            unsafe {
                asm!("cli", options(nostack));
            }
            if let Some(mut event) = event {
                if event.kind==api::POINTER {event.code=crate::keyboard::modifiers();}
                return Some(event);
            }
        }
        if !wait {
            return None;
        }
        if crate::keyboard::queued_scancodes() != 0 || crate::mouse::pending() {
            continue;
        }
        unsafe {
            asm!("sti; hlt; cli", options(nostack));
        }
    }
}
pub fn syscall(cs: u32, number: u32, first: u32, second: u32, third: u32) -> u32 {
    let Some(image) = usermode::active_image(cs) else {
        return u32::MAX;
    };
    if matches!(number,85|86){return (-16i32) as u32;}
    if number == 60 || number == 65 {
        let (width, height) = if number == 65 {
            (third & 0xffff, third >> 16)
        } else {
            (640, 400)
        };
        if (number == 60 && third != 0)
            || !api::valid_size(width, height)
            || second == 0
            || second > 48
        {
            return (-22i32) as u32;
        }
        if !user_buffer::readable(first, second, image) {
            return (-14i32) as u32;
        }
        let mut title = [0; 48];
        unsafe {
            core::ptr::copy_nonoverlapping(first as *const u8, title.as_mut_ptr(), second as usize);
        }
        if !title[..second as usize]
            .iter()
            .all(|b| (32..=126).contains(b))
        {
            return (-22i32) as u32;
        }
        unsafe {
            asm!("sti", options(nostack));
        }
        let title_length=usermode::profile().title(&mut title,second as usize);
        let result = open(&title[..title_length], width, height);
        unsafe {
            asm!("cli", options(nostack));
        }
        return result;
    }
    let Some(app) = (unsafe { (&mut *core::ptr::addr_of_mut!(APP)).as_mut() }) else {
        return (-19i32) as u32;
    };
    if first != app.id || app.owner != usermode::getpid() {
        return (-9i32) as u32;
    }
    match number {
        61 => {
            if third > 1 {
                return (-22i32) as u32;
            }
            if !user_buffer::writable(second, 20) {
                return (-14i32) as u32;
            }
            // Render with IRQs enabled; the input routine disables them only
            // around queue checks and the atomic enable/halt sequence.
            let result = input(app, third != 0);
            if let Some(event) = result {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &event as *const _ as *const u8,
                        second as *mut u8,
                        20,
                    );
                }
                1
            } else {
                0
            }
        }
        62 => {
            if third > api::MAX_COMMANDS as u32 {
                return (-22i32) as u32;
            }
            let bytes = third * 32;
            if !user_buffer::readable(second, bytes, image) {
                return (-14i32) as u32;
            }
            let mut commands = [api::Draw::default(); api::MAX_COMMANDS];
            let mut texts = [[0u8; 96]; api::MAX_COMMANDS];
            if bytes != 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        second as *const u8,
                        commands.as_mut_ptr() as *mut u8,
                        bytes as usize,
                    );
                }
            }
            for (i, c) in commands[..third as usize].iter().enumerate() {
                if !c.valid_for(app.width, app.height) {
                    return (-22i32) as u32;
                }
                if c.kind != 0 {
                    if !user_buffer::readable(c.pointer, c.length, image) {
                        return (-14i32) as u32;
                    }
                    if c.length != 0 {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                c.pointer as *const u8,
                                texts[i].as_mut_ptr(),
                                c.length as usize,
                            );
                        }
                    }
                    if !texts[i][..c.length as usize]
                        .iter()
                        .all(|b| (32..=126).contains(b) || c.kind==3 && *b==b'\n')
                    {
                        return (-22i32) as u32;
                    }
                }
            }
            unsafe {
                asm!("sti", options(nostack));
            }
            let surface = unsafe {
                core::slice::from_raw_parts_mut(
                    app.surface.base() as *mut u32,
                    (app.width * app.height) as usize,
                )
            };
            let mut canvas = Canvas::new(
                surface,
                app.width as usize,
                app.height as usize,
                app.width as usize,
            )
            .unwrap();
            for (i, c) in commands[..third as usize].iter().enumerate() {
                let (w, h) = if c.kind == 0 {
                    canvas.rect(
                        c.x as i32,
                        c.y as i32,
                        c.width as i32,
                        c.height as i32,
                        c.color,
                    );
                    (c.width, c.height)
                } else if c.kind==2 {
                    canvas.label(c.x as i32,c.y as i32,c.width,c.color,&texts[i][..c.length as usize]);
                    (c.width,c.height)
                } else if c.kind==3 {
                    canvas.paragraph(c.x as i32,c.y as i32,c.width,c.height,c.color,&texts[i][..c.length as usize]);
                    (c.width,c.height)
                } else {
                    canvas.text(
                        c.x as i32,
                        c.y as i32,
                        c.color,
                        &texts[i][..c.length as usize],
                        &crate::ui_font::FONT,
                    );
                    (c.length * 8, 16)
                };
                if w != 0 && h != 0 {
                    for row in c.y / 16..=(c.y + h - 1) / 16 {
                        app.dirty |= 1 << row;
                    }
                }
            }
            unsafe {
                asm!("cli", options(nostack));
            }
            third
        }
        63 => {
            if second != 0 || third != 0 {
                return (-22i32) as u32;
            }
            unsafe {
                asm!("sti", options(nostack));
            }
            if app.dirty != 0 {
                compose(app);
                let client = app.ui.client();
                graphics::move_pointer((app.ui.x, app.ui.y), (app.ui.x, app.ui.y));
                let mut row = 0;
                while row < app.height as i32 / 16 {
                    if app.dirty & (1 << row) == 0 {
                        row += 1;
                        continue;
                    }
                    let first = row;
                    while row < app.height as i32 / 16 && app.dirty & (1 << row) != 0 {
                        row += 1;
                    }
                    graphics::present_guarded(
                        client.x,
                        client.y + first * 16,
                        app.width as i32,
                        (row - first) * 16,
                        (app.ui.x, app.ui.y),
                    );
                }
                app.dirty = 0;
                graphics::pointer(app.ui.x, app.ui.y);
            }
            unsafe {
                asm!("cli", options(nostack));
            }
            0
        }
        64 => {
            if second != 0 || third != 0 {
                return (-22i32) as u32;
            }
            unsafe {
                asm!("sti", options(nostack));
            }
            finish();
            unsafe {
                asm!("cli", options(nostack));
            }
            0
        }
        _ => (-22i32) as u32,
    }
}

pub fn redraw(){unsafe {if let Some(app)=(&*core::ptr::addr_of!(APP)).as_ref() {compose(app);graphics::move_pointer((app.ui.x,app.ui.y),(app.ui.x,app.ui.y));graphics::present_guarded(0,0,800,600,(app.ui.x,app.ui.y));}}}
