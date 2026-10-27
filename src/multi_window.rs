// The scheduler pumps this compositor only between user quanta. Syscalls are
// non-preemptible kernel work; IRQs never borrow the window pool or surfaces.
use crate::{
    app_window_ui::State, desktop_ui::Rect, graphics, loader::Frames, user_buffer, usermode,
    window_protocol as api,
};
use core::arch::asm;
const N: usize = 4;
struct Window {
    owner: u32,
    id: u32,
    ui: State,
    title: [u8; 48],
    length: usize,
    surface: Frames,
    width: u32,
    height: u32,
    dirty: Option<Rect>,
    events: crate::window_queue::Queue,
}
impl Window {
    fn push(&mut self, event: api::Event) {
        self.events.push(event, false);
    }
    fn pop(&mut self) -> Option<api::Event> {
        self.events.pop()
    }
}
static mut WINDOWS: [Option<Window>; N] = [const { None }; N];
static mut ORDER: [usize; N] = [0, 1, 2, 3];
static mut FOCUS: Option<usize> = None;
static mut CAPTURE: Option<usize> = None;
static mut POINTER: (i32, i32) = (400, 300);
static mut DRAWN: (i32, i32) = (400, 300);
static mut BUTTONS: u8 = 0;
static mut DAMAGE: Option<Rect> = None;
static mut NEXT: u32 = 1;
static mut STOP: bool = false;
static mut KILL: Option<u32> = None;
static mut SHELL: Option<crate::desktop_shell::Shell> = None;
static mut LAUNCH: Option<crate::app_catalog::Entry> = None;
pub fn has_window(pid: u32) -> bool {
    unsafe {
        (&*core::ptr::addr_of!(WINDOWS))
            .iter()
            .flatten()
            .any(|w| w.owner == pid)
    }
}
pub fn desktop_mode() -> bool {
    unsafe { (&*core::ptr::addr_of!(SHELL)).is_some() }
}
pub fn take_launch() -> Option<crate::app_catalog::Entry> {
    unsafe { (*core::ptr::addr_of_mut!(LAUNCH)).take() }
}
pub fn status(text: &[u8]) {
    unsafe {
        if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
            shell.status(text);
            damage(Rect {
                x: 24,
                y: 64,
                w: 752,
                h: 16,
            });
        }
    }
}
pub fn refresh_catalog() {
    let mut catalog = crate::app_catalog::Catalog::new();
    let result = crate::diskfs::list("/APPS", |name, _, directory| catalog.add(name, directory));
    catalog.sort();
    unsafe {
        if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
            shell.catalog = catalog;
        }
    }
    if result.is_err() {
        status(b"Apps unavailable. Return to the shell to select a mounted drive.");
    }
}
pub fn set_tasks(tasks: [Option<crate::desktop_shell::Task>; 4]) {
    unsafe {
        if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
            shell.tasks = tasks;
            if shell
                .focused
                .is_some_and(|pid| !tasks.iter().flatten().any(|t| t.pid == pid))
            {
                shell.focused = FOCUS.and_then(|i| WINDOWS[i].as_ref().map(|w| w.owner));
            }
            damage(Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 24,
            });
        }
    }
}
pub fn begin_desktop() -> Result<(), &'static str> {
    begin()?;
    unsafe {
        SHELL = Some(crate::desktop_shell::Shell::new());
    }
    status(b"Apps launches programs. Click a task to switch; close apps before Exit.");
    refresh_catalog();
    unsafe {
        damage(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        });
    }
    flush();
    Ok(())
}
pub fn suspend() {
    unsafe {
        CAPTURE = None;
        if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
            shell.reset(0);
        }
    }
    graphics::restore();
    crate::mouse::discard();
}
pub fn resume() -> Result<(), &'static str> {
    graphics::start()?;
    unsafe {
        BUTTONS = crate::mouse::discard();
        CAPTURE = None;
        if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
            shell.reset(BUTTONS);
        }
        for w in (&mut *core::ptr::addr_of_mut!(WINDOWS))
            .iter_mut()
            .flatten()
        {
            w.ui.update(crate::mouse_packet::Event {
                lost: true,
                buttons: BUTTONS,
                ..crate::mouse_packet::Event::default()
            });
            w.push(api::Event {
                kind: api::RESET,
                buttons: BUTTONS as u32,
                ..api::Event::default()
            });
        }
        damage(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        });
    }
    flush();
    Ok(())
}
unsafe fn chrome(x: i32, y: i32, buttons: u8, lost: bool) -> bool {
    let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) else {
        return false;
    };
    if (*core::ptr::addr_of!(CAPTURE)).is_some() {
        shell.reset(buttons);
        return false;
    }
    let old_open = shell.open;
    let old_menu = shell.menu();
    let (consumed, action, changed) = shell.input(x, y, buttons, lost);
    if changed {
        damage(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 24,
        });
        if old_open {
            damage(Rect {
                w: old_menu.w + 6,
                h: old_menu.h + 6,
                ..old_menu
            });
        }
        if shell.open {
            let r = shell.menu();
            damage(Rect {
                w: r.w + 6,
                h: r.h + 6,
                ..r
            });
        }
    }
    let opened = shell.open && !old_open;
    match action {
        Some(crate::desktop_shell::Action::Launch(i)) => {
            LAUNCH = Some(shell.catalog.entries[i]);
        }
        Some(crate::desktop_shell::Action::Focus(pid)) => {
            let index = (&*core::ptr::addr_of!(WINDOWS))
                .iter()
                .position(|w| w.as_ref().is_some_and(|w| w.owner == pid));
            if let Some(i)=index {if let Some(w)=&mut WINDOWS[i] {damage(w.ui.window.bounds());w.ui.restore();damage(w.ui.window.bounds());crate::debug::write_fmt(format_args!("Multiwindow: restored pid={} pos={},{}\n",w.owner,w.ui.window.rect.x,w.ui.window.rect.y));}}
            focus(index);
            if index.is_none() {
                if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
                    shell.focused = Some(pid);
                    damage(Rect {
                        x: 0,
                        y: 0,
                        w: 800,
                        h: 24,
                    });
                }
            }
        }
        Some(crate::desktop_shell::Action::Exit) => {
            if shell.tasks.iter().all(Option::is_none) {
                STOP = true;
            } else {
                status(b"Close running apps before leaving. Ctrl+Alt+K stops a stuck focused app.");
            }
        }
        None => (),
    }
    if opened {
        refresh_catalog();
        let shell = (&*core::ptr::addr_of!(SHELL)).as_ref().unwrap();
        let r = shell.menu();
        damage(Rect {
            w: r.w + 6,
            h: r.h + 6,
            ..r
        });
    }
    consumed
}

unsafe fn damage(rect: Rect) {
    DAMAGE = Some(DAMAGE.map_or(rect, |old| old.union(rect)));
}
unsafe fn focus(index: Option<usize>) {
    assert!(index.is_none_or(|i| i < N));
    FOCUS = index;
    if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
        shell.focused = index.and_then(|i| WINDOWS[i].as_ref().map(|w| w.owner));
        damage(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 24,
        });
    }
    if let Some(index) = index {
        // Raising changes occlusion across the whole window, even when no
        // client draw or movement follows the focus click.
        if ORDER[N - 1] != index {
            if let Some(w) = &WINDOWS[index] {
                damage(w.ui.window.bounds());
            }
        }
        let mut next = [0; N];
        let mut at = 0;
        for &i in &*core::ptr::addr_of!(ORDER) {
            if i != index {
                next[at] = i;
                at += 1;
            }
        }
        next[N - 1] = index;
        ORDER = next;
    }
    for (i, window) in (&mut *core::ptr::addr_of_mut!(WINDOWS))
        .iter_mut()
        .enumerate()
    {
        if let Some(w) = window {
            if w.ui.focused != (Some(i) == index) {
                damage(w.ui.window.title());
            }
            w.ui.focused = Some(i) == index;
        }
    }
}
pub fn begin() -> Result<(), &'static str> {
    let _ = crate::mouse::init();
    graphics::start()?;
    unsafe {
        WINDOWS = [const { None }; N];
        ORDER = [0, 1, 2, 3];
        FOCUS = None;
        CAPTURE = None;
        POINTER = (400, 300);
        DRAWN = POINTER;
        BUTTONS = crate::mouse::discard();
        STOP = false;
        KILL = None;
        SHELL = None;
        LAUNCH = None;
        DAMAGE = Some(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        });
    }
    flush();
    crate::debug::write_str("Multitask: frame ready\n");
    Ok(())
}
pub fn finish() {
    unsafe {
        WINDOWS = [const { None }; N];
        SHELL = None;
        LAUNCH = None;
        FOCUS = None;
        CAPTURE = None;
        DAMAGE = None;
    }
    graphics::restore();
    crate::mouse::discard();
}
pub fn pending(pid: u32) -> bool {
    unsafe {
        (&*core::ptr::addr_of!(WINDOWS))
            .iter()
            .flatten()
            .any(|w| w.owner == pid && w.events.pending())
    }
}
pub fn take_kill() -> Option<u32> {
    unsafe { (*core::ptr::addr_of_mut!(KILL)).take() }
}
pub fn remove(pid: u32) {
    unsafe {
        for i in 0..N {
            if WINDOWS[i].as_ref().is_some_and(|w| w.owner == pid) {
                let w = WINDOWS[i].take().unwrap();
                damage(w.ui.window.bounds());
                if CAPTURE == Some(i) {
                    CAPTURE = None;
                }
                if FOCUS == Some(i) {
                    let next = (&*core::ptr::addr_of!(ORDER))
                        .iter()
                        .rev()
                        .copied()
                        .find(|&j| WINDOWS[j].as_ref().is_some_and(|w|w.ui.window.visible));
                    focus(next);
                }
            }
        }
    }
    flush();
}
fn flush() {
    unsafe {
        if let Some(rect) = (*core::ptr::addr_of_mut!(DAMAGE)).take() {
            let mut views = [None; N];
            for (i, w) in (&*core::ptr::addr_of!(WINDOWS)).iter().enumerate() {
                if let Some(w) = w {
                    views[i] = Some(graphics::WindowView {
                        ui: w.ui,
                        title: &w.title[..w.length],
                        pixels: core::slice::from_raw_parts(
                            w.surface.base() as *const u32,
                            (w.width * w.height) as usize,
                        ),
                    });
                }
            }
            graphics::multi_scene(
                &views,
                &*core::ptr::addr_of!(ORDER),
                (&*core::ptr::addr_of!(SHELL)).as_ref(),
            );
            graphics::move_pointer(DRAWN, POINTER);
            graphics::present_guarded(rect.x, rect.y, rect.w, rect.h, POINTER);
        } else if DRAWN != POINTER {
            graphics::move_pointer(DRAWN, POINTER);
        }
        DRAWN = POINTER;
    }
}
pub fn pump() -> bool {
    for _ in 0..256 {
        let key = crate::keyboard::next_order();
        let mouse = crate::mouse::next_order();
        let pointer = crate::mouse::lost()
            || match (key, mouse) {
                (Some(k), Some(m)) => crate::input_order::before(m, k),
                (None, Some(_)) => true,
                _ => false,
            };
        if !pointer {
            if key.is_none() {
                break;
            }
            let Some(code) = crate::keyboard::poll_event() else {
                continue;
            };
            unsafe {
                if code as u16 == 27 {
                    if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
                        if shell.open {
                            let r = shell.menu();
                            shell.reset(BUTTONS);
                            damage(Rect {
                                w: r.w + 6,
                                h: r.h + 6,
                                ..r
                            });
                            damage(Rect {
                                x: 0,
                                y: 0,
                                w: 800,
                                h: 24,
                            });
                            continue;
                        }
                        if shell.tasks.iter().all(Option::is_none) {
                            STOP = true;
                            break;
                        }
                    } else {
                        STOP = true;
                        break;
                    }
                }
                if code & ((1 << 17) | (1 << 18)) == ((1 << 17) | (1 << 18))
                    && matches!(code as u16, 75 | 107)
                {
                    KILL = if let Some(shell) = &*core::ptr::addr_of!(SHELL) {
                        shell.focused
                    } else {
                        FOCUS.and_then(|i| WINDOWS[i].as_ref().map(|w| w.owner))
                    };
                    continue;
                }
                if let Some(i) = FOCUS {
                    if let Some(w) = &mut WINDOWS[i] {
                        w.push(api::Event {
                            kind: api::KEY,
                            code,
                            ..api::Event::default()
                        });
                    }
                }
            }
            continue;
        }
        let Some(event) = crate::mouse::poll() else {
            continue;
        };
        unsafe {
            let old = POINTER;
            POINTER = (
                (old.0 + event.dx as i32).clamp(0, 799),
                (old.1 + event.dy as i32).clamp(0, 599),
            );
            if chrome(POINTER.0, POINTER.1, event.buttons, event.lost) {
                BUTTONS = event.buttons;
                continue;
            }
            let pressed = event.buttons & !BUTTONS;
            let released = BUTTONS & !event.buttons;
            if event.lost {
                CAPTURE = None;
                for w in (&mut *core::ptr::addr_of_mut!(WINDOWS))
                    .iter_mut()
                    .flatten()
                {
                    w.ui.x = POINTER.0;
                    w.ui.y = POINTER.1;
                    w.ui.update(event);
                    w.push(api::Event {
                        kind: api::RESET,
                        buttons: event.buttons as u32,
                        ..api::Event::default()
                    });
                    damage(w.ui.window.bounds());
                }
                BUTTONS = event.buttons;
                continue;
            }
            if pressed & 7 != 0 {
                let hit = (&*core::ptr::addr_of!(ORDER))
                    .iter()
                    .rev()
                    .copied()
                    .find(|&i| {
                        WINDOWS[i]
                            .as_ref()
                            .is_some_and(|w| w.ui.window.visible&&w.ui.window.rect.contains(POINTER.0, POINTER.1))
                    });
                focus(hit);
                if pressed & 1 != 0 {
                    CAPTURE = hit;
                }
            }
            let target = CAPTURE.or(FOCUS);
            assert!(target.is_none_or(|i| i < N));
            if let Some(i) = target {
                if let Some(w) = &mut WINDOWS[i] {
                    let previous = w.ui.window.bounds();
                    w.ui.x = old.0;
                    w.ui.y = old.1;
                    w.ui.buttons = BUTTONS;
                    let (changed, input) = w.ui.update(event);
                    if changed {
                        damage(previous.union(w.ui.window.bounds()));
                    }
                    if let Some(input) = input {
                        if input.kind == api::CLOSE {
                            crate::debug::write_fmt(format_args!(
                                "Multiwindow: close request pid={}\n",
                                w.owner
                            ));
                        }
                        w.events.push(input, event.buttons == BUTTONS);
                    }
                }
            }
            if FOCUS.is_some_and(|i|WINDOWS[i].as_ref().is_some_and(|w|!w.ui.window.visible)) {
                let next=ORDER.iter().rev().copied().find(|&i|WINDOWS[i].as_ref().is_some_and(|w|w.ui.window.visible));
                if let Some(i)=FOCUS {crate::debug::write_fmt(format_args!("Multiwindow: minimized pid={}\n",WINDOWS[i].as_ref().unwrap().owner));}
                CAPTURE=None;focus(next);
            }
            if released & 1 != 0 {
                CAPTURE = None;
            }
            BUTTONS = event.buttons;
        }
    }
    flush();
    unsafe { STOP }
}
pub fn syscall(cs: u32, number: u32, first: u32, second: u32, third: u32) -> u32 {
    let Some(image) = usermode::active_image(cs) else {
        return u32::MAX;
    };
    let owner = usermode::getpid();
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
            if (&*core::ptr::addr_of!(WINDOWS))
                .iter()
                .flatten()
                .any(|w| w.owner == owner)
            {
                return (-16i32) as u32;
            }
        }
        let Some(index) = (unsafe {
            (&*core::ptr::addr_of!(WINDOWS))
                .iter()
                .position(Option::is_none)
        }) else {
            return (-24i32) as u32;
        };
        let Some(surface) = Frames::new((width as usize * height as usize * 4).div_ceil(4096))
        else {
            return (-12i32) as u32;
        };
        unsafe {
            core::slice::from_raw_parts_mut(surface.base() as *mut u32, (width * height) as usize)
                .fill(0xffffff);
            let id = NEXT;
            if id > i32::MAX as u32 {
                return (-24i32) as u32;
            }
            NEXT += 1;
            let x = (56 + index as i32 * 352).min(800 - width as i32 - 22);
            let y = (124 + index as i32 * 96).min(530 - height as i32);
            let mut ui = State::sized(BUTTONS, width as i32, height as i32, x, y.max(24));
            ui.x = POINTER.0;
            ui.y = POINTER.1;
            crate::debug::write_fmt(format_args!(
                "Multiwindow: open pid={} handle={} surface={:08x} client={}x{} pos={},{}\n",
                owner,
                id,
                surface.base(),
                width,
                height,
                x,
                y.max(24)
            ));
            ui.minimizable=desktop_mode();
            WINDOWS[index] = Some(Window {
                owner,
                id,
                ui,
                title,
                length: second as usize,
                surface,
                width,
                height,
                dirty: None,
                events: crate::window_queue::Queue::new(),
            });
            focus(Some(index));
            damage(WINDOWS[index].as_ref().unwrap().ui.window.bounds());
            asm!("sti", options(nostack));
            flush();
            asm!("cli", options(nostack));
            return id;
        }
    }
    let Some(index) = (unsafe {
        (&*core::ptr::addr_of!(WINDOWS)).iter().position(|w| {
            w.as_ref()
                .is_some_and(|w| w.owner == owner && w.id == first)
        })
    }) else {
        return (-9i32) as u32;
    };
    let w = unsafe { WINDOWS[index].as_mut().unwrap() };
    match number {
        61 => {
            if third > 1 {
                return (-22i32) as u32;
            }
            if !user_buffer::writable(second, 20) {
                return (-14i32) as u32;
            }
            if let Some(event) = w.pop() {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &event as *const _ as *const u8,
                        second as *mut u8,
                        20,
                    );
                }
                1
            } else {
                if third != 0 {
                    crate::multitask::wait_window();
                }
                0
            }
        }
        62 => {
            if third > 16 || !user_buffer::readable(second, third.saturating_mul(32), image) {
                return (if third > 16 { -22i32 } else { -14i32 }) as u32;
            }
            let mut commands = [api::Draw::default(); 16];
            let mut texts = [[0u8; 80]; 16];
            if third != 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        second as *const u8,
                        commands.as_mut_ptr() as *mut u8,
                        third as usize * 32,
                    );
                }
            }
            for (i, c) in commands[..third as usize].iter().enumerate() {
                if !c.valid_for(w.width, w.height) {
                    return (-22i32) as u32;
                }
                if c.kind == 1 {
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
                        .all(|b| (32..=126).contains(b))
                    {
                        return (-22i32) as u32;
                    }
                }
            }
            unsafe {
                asm!("sti", options(nostack));
            }
            let pixels = unsafe {
                core::slice::from_raw_parts_mut(
                    w.surface.base() as *mut u32,
                    (w.width * w.height) as usize,
                )
            };
            let mut c = crate::canvas::Canvas::new(
                pixels,
                w.width as usize,
                w.height as usize,
                w.width as usize,
            )
            .unwrap();
            for (i, cmd) in commands[..third as usize].iter().enumerate() {
                let (width, height) = if cmd.kind == 0 {
                    c.rect(
                        cmd.x as i32,
                        cmd.y as i32,
                        cmd.width as i32,
                        cmd.height as i32,
                        cmd.color,
                    );
                    (cmd.width, cmd.height)
                } else {
                    c.text(
                        cmd.x as i32,
                        cmd.y as i32,
                        cmd.color,
                        &texts[i][..cmd.length as usize],
                        &crate::ui_font::FONT,
                    );
                    (cmd.length * 8, 16)
                };
                if width != 0 && height != 0 {
                    let r = Rect {
                        x: cmd.x as i32,
                        y: cmd.y as i32,
                        w: width as i32,
                        h: height as i32,
                    };
                    w.dirty = Some(w.dirty.map_or(r, |old| old.union(r)));
                }
            }
            unsafe {
                asm!("cli", options(nostack));
            }
            third
        }
        63 => {
            if second | third != 0 {
                return (-22i32) as u32;
            }
            if let Some(dirty) = w.dirty.take().filter(|_|w.ui.window.visible) {
                let client = w.ui.client();
                unsafe {
                    damage(Rect {
                        x: client.x + dirty.x,
                        y: client.y + dirty.y,
                        ..dirty
                    });
                    asm!("sti", options(nostack));
                }
                flush();
                unsafe {
                    asm!("cli", options(nostack));
                }
            }
            0
        }
        64 => {
            if second | third != 0 {
                return (-22i32) as u32;
            }
            unsafe {
                asm!("sti", options(nostack));
            }
            remove(owner);
            unsafe {
                asm!("cli", options(nostack));
            }
            0
        }
        _ => (-22i32) as u32,
    }
}

pub fn terminal_present(owner:u32,handle:u32,cells:&[crate::pty_model::Cell;2000],rows:u32,cursor:Option<(usize,usize)>)->u32 {
 let Some(index)=(unsafe {(&*core::ptr::addr_of!(WINDOWS)).iter().position(|w|w.as_ref().is_some_and(|w|w.owner==owner&&w.id==handle))}) else{return (-9i32) as u32;};
 unsafe {let w=WINDOWS[index].as_mut().unwrap();if w.width!=640||w.height!=400 {return (-22i32) as u32;}let pixels=core::slice::from_raw_parts_mut(w.surface.base() as *mut u32,640*400);asm!("sti",options(nostack));crate::graphics::terminal_cells(pixels,cells,rows,cursor);let client=w.ui.client();
 for y in 0..25 {if w.ui.window.visible&&rows&(1<<y)!=0 {damage(Rect{x:client.x,y:client.y+y*16,w:640,h:16});}}flush();asm!("cli",options(nostack));}0
}
