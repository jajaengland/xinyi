// The scheduler pumps this compositor only between user quanta. Syscalls are
// non-preemptible kernel work; IRQs never borrow the window pool or surfaces.
use crate::{
    app_window_ui::State, desktop_ui::Rect, graphics, loader::Frames, user_memory as user_buffer, usermode,
    window_protocol as api,
};
use core::arch::asm;
const N: usize = 4;
struct Window {
    drop_target:bool,
    open_drop:bool,
    owner: u32,
    id: u32,
    ui: State,
    title: [u8; 48],
    length: usize,
    surface: Frames,
    width: u32,
    height: u32,
    requested:Option<(u32,u32)>,
    dirty: Option<Rect>,
    io_blocks: Option<u32>,
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
static mut LAUNCH: Option<(crate::app_catalog::Entry,u32,[u8;128],usize,u32)> = None;
static mut LAST_CLOCK:u32=u32::MAX;
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
pub fn take_launch() -> Option<(crate::app_catalog::Entry,u32,[u8;128],usize,u32)> {
    unsafe { (*core::ptr::addr_of_mut!(LAUNCH)).take() }
}
pub fn status(text: &[u8]) {
    unsafe {
        if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
            shell.status(text);
            if shell.open {let r=shell.menu();damage(Rect{x:r.x+16,y:r.y+r.h-20,w:r.w-32,h:16});}
        }
    }
}
pub fn collect_catalog(catalog:&mut crate::app_catalog::Catalog)->crate::fat16::Result<()> {
    let mut folders=[[0;11];64];let mut count=0;let mut overflow=false;
    crate::diskfs::list("/APPS", |name,_,directory| {
        if directory {if count<folders.len() {folders[count]=*name;count+=1;}else{overflow=true;}}else{catalog.add(name,false);}
    })?;
    if overflow {return Err(crate::fat16::Error::TooLarge);}
    for name in &folders[..count] {
        let mut id=[0;12];let n=crate::fat16::display_name(name,&mut id);
        if !crate::app_install::valid_id(&id[..n]) {continue;}
        let mut path=crate::fs_path::Path::parse("/APPS",&crate::fs_path::Path::ROOT)?;path.names[1]=*name;path.depth=2;
        if let Ok((executable,n))=crate::app_install::executable(path) {
            let text=core::str::from_utf8(&executable[..n]).unwrap();
            if let Ok(Some(_))=crate::app_metadata::load(text) {
                let mut short=*name;short[8..].copy_from_slice(&executable[n-3..n]);catalog.add_bundle(&short);
            }
        }
    }
    catalog.sort();
    Ok(())
}
pub fn refresh_catalog() {
    if crate::disk_io::busy() { return; }
    let mut catalog = crate::app_catalog::Catalog::new();
    let result=collect_catalog(&mut catalog);
    for entry in &mut catalog.entries[..catalog.len] {
        let path=core::str::from_utf8(&entry.path[..entry.path_len]).unwrap();
        if let Ok(Some(metadata))=crate::app_metadata::load(path) {entry.metadata(metadata.title(),metadata.description(),metadata.icon);}
    }
    unsafe {
        if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
            shell.catalog = catalog;
            shell.catalog_changed();
        }
    }
    if result.is_err() {
        status(b"Apps unavailable. Return to the shell to select a mounted drive.");
    }
}
pub fn io_progress(pid: u32, blocks: Option<u32>) {
    let owner = crate::pty::io_window_owner(pid);
    unsafe {
        for window in (&mut *core::ptr::addr_of_mut!(WINDOWS)).iter_mut().flatten() {
            if window.owner == owner && window.io_blocks != blocks {
                window.io_blocks = blocks;
                if window.ui.window.visible { damage(window.ui.title()); }
            }
        }
    }
}
pub fn set_tasks(tasks: [Option<crate::desktop_shell::Task>; 4]) {
    unsafe {
        if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
            let pids = tasks.map(|task| task.map_or(0, |task| task.pid));
            let changed = shell.tasks.map(|task| task.map_or(0, |task| task.pid)) != pids;
            if changed && matches!(shell.capture, Some(crate::desktop_shell::Hit::Task(_))) {
                shell.capture = None;
            }
            shell.tasks = tasks;
            if changed { crate::debug::write_fmt(format_args!("Taskbar: pids={},{},{},{}\n",pids[0],pids[1],pids[2],pids[3])); }
            if shell
                .focused
                .is_some_and(|pid| !tasks.iter().flatten().any(|t| t.pid == pid))
            {
                shell.focused = FOCUS.and_then(|i| WINDOWS[i].as_ref().map(|w| w.owner));
            }
            damage(crate::desktop_shell::Shell::bar());
        }
    }
}
pub fn begin_desktop() -> Result<(), &'static str> {
    begin()?;
    unsafe {
        SHELL = Some(crate::desktop_shell::Shell::new());
        (*core::ptr::addr_of_mut!(SHELL)).as_mut().unwrap().volume=crate::audio::system_volume(None);
    }
    refresh_catalog();
    if crate::diskfs::is_mounted()&&!crate::disk_io::busy() {let _=crate::diskfs::mkdir("/DESKTOP");}
    refresh_desktop();
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
    if !lost&&buttons&1!=0&&DRAG.is_none() {
        if let Some(crate::desktop_shell::Hit::Desktop(i))=shell.capture {
            if (x-shell.press.0).abs().max((y-shell.press.1).abs())>=5&&shell.file_volume==crate::diskfs::volume_epoch() {
                let mut payload=crate::directory_protocol::FileClipboard::new();
                // The grabbed icon is first, so group placement follows its pointer anchor.
                for j in core::iter::once(i).chain((0..shell.file_count).filter(|&j|j!=i)) {
                    if shell.selection&(1<<j)==0 {continue;}
                    let e=shell.files[j];let at=payload.count as usize;
                    payload.paths[at][..9].copy_from_slice(b"/DESKTOP/");payload.paths[at][9..9+e.length].copy_from_slice(&e.name[..e.length]);payload.lengths[at]=(9+e.length) as u16;payload.flags[at]=e.folder as u8;payload.count+=1;
                }
                if payload.count==0 {return true;}payload.volume=shell.file_volume;
                let token=DRAG_TOKEN;DRAG_TOKEN=DRAG_TOKEN.wrapping_add(1).max(1);payload.version=token;
                shell.reset(buttons);DRAG=Some(Drag{source:0,target:0,token,active:true,taken:false,payload});damage(Rect{x:0,y:0,w:800,h:544});crate::debug::write_fmt(format_args!("Drag: begin desktop\n"));return true;
            }
        }
    }
    let old_open = shell.open;
    let old_sound=shell.sound_open;
    let old_menu = shell.menu();
    shell.background=!(&*core::ptr::addr_of!(WINDOWS)).iter().flatten().any(|w|w.ui.window.visible&&w.ui.window.rect.contains(x,y));
    shell.modifiers=crate::keyboard::modifiers();
    let focus_desktop=buttons&!BUTTONS&3!=0&&shell.background&&y<544&&!shell.open&&!shell.sound_open;
    let (consumed, action, changed) = shell.input(x, y, buttons, lost);
    if changed {
        damage(Rect{x:0,y:0,w:800,h:544});
        if old_sound||shell.sound_open {damage(crate::desktop_shell::Shell::sound_panel());}
        damage(crate::desktop_shell::Shell::bar());
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
            if core::ptr::addr_of!(LAUNCH).read().is_none() { LAUNCH = Some((shell.catalog.entries[i],crate::diskfs::volume_epoch(),[0;128],0,0));
                crate::debug::write_fmt(format_args!("App load: queued path={} volume={}\n",core::str::from_utf8(&shell.catalog.entries[i].path[..shell.catalog.entries[i].path_len]).unwrap(),crate::diskfs::volume_epoch())); }
            else { status(b"An app launch is already queued. Please wait."); }
        }
        Some(crate::desktop_shell::Action::OpenDesktop(i)) => {
            if i==21 {for j in 0..shell.file_count {shell.slots[j]=j as u8;}shell.layout_dirty=true;damage(Rect{x:0,y:0,w:800,h:544});return consumed;}
            if i==20 {queue_desktop(b"/DESKTOP",crate::diskfs::volume_epoch(),true);return consumed;}
            let e=shell.files[i];let volume=shell.file_volume;let mut path=[0;128];path[..9].copy_from_slice(b"/DESKTOP/");path[9..9+e.length].copy_from_slice(&e.name[..e.length]);queue_desktop(&path[..9+e.length],volume,e.folder);
        }
        Some(crate::desktop_shell::Action::Focus(pid)) => {
            activate(pid);
        }
        Some(crate::desktop_shell::Action::Exit) => {
            if shell.tasks.iter().all(Option::is_none) && !crate::disk_io::busy() {
                STOP = true;
            } else {
                status(b"Close running apps before leaving. Ctrl+Alt+K stops a stuck focused app.");
                crate::system_sounds::play(crate::system_sounds::Event::Warning);
            }
        }
        Some(crate::desktop_shell::Action::FileAction(op))=>{desktop_file_action(op);}
        Some(crate::desktop_shell::Action::Volume(value))=>{crate::audio::system_volume(Some(value));}
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
    if focus_desktop {focus(None);}
    consumed
}

unsafe fn damage(rect: Rect) {
    DAMAGE = Some(DAMAGE.map_or(rect, |old| old.union(rect)));
}
unsafe fn dismiss_menu() {
    if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
        if let Some(r)=shell.context_rect(){damage(Rect{w:r.w+4,h:r.h+4,..r});}
        if shell.sound_open {damage(crate::desktop_shell::Shell::sound_panel());}
        if shell.open {
            let rect = shell.menu();
            damage(Rect { w: rect.w + 6, h: rect.h + 6, ..rect });
        }
        shell.reset(BUTTONS);
        damage(crate::desktop_shell::Shell::bar());
    }
}
unsafe fn activate(pid: u32) {
    if let Some(index) = (*core::ptr::addr_of_mut!(CAPTURE)).take() {
        if let Some(window) = &mut WINDOWS[index] {
            if let Some(preview) = window.ui.preview() { damage(preview); }
            window.ui.update(crate::mouse_packet::Event { lost: true, buttons: BUTTONS, ..crate::mouse_packet::Event::default() });
            window.push(api::Event { kind: api::RESET, buttons: BUTTONS as u32, ..api::Event::default() });
            damage(window.ui.window.bounds());
        }
    }
    let index = (&*core::ptr::addr_of!(WINDOWS)).iter()
        .position(|window| window.as_ref().is_some_and(|window| window.owner == pid));
    if let Some(index) = index {
        let window = WINDOWS[index].as_mut().unwrap();
        damage(window.ui.window.bounds());
        window.ui.restore();
        damage(window.ui.window.bounds());
        crate::debug::write_fmt(format_args!("Multiwindow: restored pid={} pos={},{}\n",pid,window.ui.window.rect.x,window.ui.window.rect.y));
    }
    focus(index);
    if index.is_none() {
        if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) { shell.focused = Some(pid); }
    }
}
unsafe fn focus(index: Option<usize>) {
    assert!(index.is_none_or(|i| i < N));
    FOCUS = index;
    if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
        shell.keyboard=index.is_none();
        shell.focused = index.and_then(|i| WINDOWS[i].as_ref().map(|w| w.owner));
        damage(crate::desktop_shell::Shell::bar());
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
        DRAG=None;DESKTOP_DROP=false;
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
    save_desktop_layout();
    unsafe {
        DRAG=None;DESKTOP_DROP=false;
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
    unsafe {if DRAG.as_ref().is_some_and(|d|d.source==pid||d.target==pid) {cancel_drag();}}
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
                        io_blocks: w.io_blocks,
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
    unsafe {
        if let Some(d)=DRAG.as_mut().filter(|d|d.active) {let cut=((crate::keyboard::modifiers()&(1<<16))!=0) as u16;if cut!=d.payload.cut {d.payload.cut=cut;damage(Rect{x:(POINTER.0+16).min(680),y:(POINTER.1+18).min(516),w:116,h:24});}}
        let seconds=crate::interrupts::uptime_seconds();
        if seconds!=LAST_CLOCK {
            LAST_CLOCK=seconds;
            refresh_desktop();
            save_desktop_layout();
            if let Some(shell)=(&mut *core::ptr::addr_of_mut!(SHELL)).as_mut() {
                let old=(shell.clock,shell.volume);
                if let Some(now)=crate::rtc::try_read() {shell.clock=[b'0'+now.hour/10,b'0'+now.hour%10,b':',b'0'+now.minute/10,b'0'+now.minute%10];}
                shell.volume=crate::audio::system_volume(None);
                if old!=(shell.clock,shell.volume) {damage(crate::desktop_shell::Shell::bar());if shell.sound_open {damage(crate::desktop_shell::Shell::sound_panel());}}
            }
        }
    }
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
                if DRAG.as_ref().is_some_and(|d|d.active) {
                    if code as u16==27 {cancel_drag();continue;}
                    if code as u16==0x123&&code&(1<<18)!=0 || matches!(code as u16,75|107)&&code&((1<<17)|(1<<18))==((1<<17)|(1<<18)) {cancel_drag();}else{continue;}
                }
                if code as u16==27 && code & ((1<<17)|(1<<18)) == ((1<<17)|(1<<18)) {
                    if crate::disk_io::cancel_load() {status(b"Cancelling app load. Existing apps keep running.");}
                    continue;
                }
                if code as u16 == 9 && code & (1 << 18) != 0 && code & (1 << 17) == 0 {
                    if let Some(shell) = &*core::ptr::addr_of!(SHELL) {
                        let next = shell.next_task(code & (1 << 16) != 0);
                        dismiss_menu();
                        if let Some(pid) = next {
                            activate(pid);
                            crate::debug::write_fmt(format_args!("Multiwindow: keyboard focus pid={}\n",pid));
                        }
                        continue;
                    }
                }
                if code as u16 == 0x123 && code & (1 << 18) != 0 && code & (1 << 17) == 0 {
                    dismiss_menu();
                    if let Some(window) = FOCUS.and_then(|index| WINDOWS[index].as_mut()) {
                        window.push(api::Event { kind: api::CLOSE, ..api::Event::default() });
                        crate::debug::write_fmt(format_args!("Multiwindow: close request pid={}\n",window.owner));
                    }
                    continue;
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
                if code as u16 == 27 && code & (1 << 17) != 0 && code & (1 << 18) == 0
                    && (&*core::ptr::addr_of!(SHELL)).as_ref().is_some_and(|shell| !shell.open) {
                    refresh_catalog();
                }
                if let Some(shell) = &mut *core::ptr::addr_of_mut!(SHELL) {
                    let menu = shell.menu();
                    if let Some(r)=shell.context_rect(){damage(Rect{w:r.w+4,h:r.h+4,..r});}
                    let (consumed, action) = shell.key(code);
                    if consumed {
                        if shell.keyboard {damage(Rect{x:0,y:0,w:800,h:544});}
                        damage(Rect { w: menu.w + 6, h: menu.h + 6, ..menu });
                        damage(crate::desktop_shell::Shell::bar());
                        if let Some(crate::desktop_shell::Action::Launch(index)) = action {
                            if core::ptr::addr_of!(LAUNCH).read().is_none() { LAUNCH = Some((shell.catalog.entries[index],crate::diskfs::volume_epoch(),[0;128],0,0));
                                crate::debug::write_fmt(format_args!("App load: queued path={} volume={}\n",core::str::from_utf8(&shell.catalog.entries[index].path[..shell.catalog.entries[index].path_len]).unwrap(),crate::diskfs::volume_epoch())); }
                            else { status(b"An app launch is already queued. Please wait."); }
                        }
                        if let Some(crate::desktop_shell::Action::FileAction(op))=action {desktop_file_action(op);}
                        if let Some(crate::desktop_shell::Action::OpenDesktop(i))=action {
                            let e=shell.files[i];let volume=shell.file_volume;let mut path=[0;128];path[..9].copy_from_slice(b"/DESKTOP/");path[9..9+e.length].copy_from_slice(&e.name[..e.length]);queue_desktop(&path[..9+e.length],volume,e.folder);
                        }
                        continue;
                    }
                }
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
                            damage(crate::desktop_shell::Shell::bar());
                            continue;
                        }
                        if shell.tasks.iter().all(Option::is_none) && !crate::disk_io::busy() {
                            STOP = true;
                            break;
                        }
                    } else {
                        STOP = true;
                        break;
                    }
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
            if DRAG.as_ref().is_some_and(|d|d.active) {drag_pointer(event.buttons,event.lost,old);BUTTONS=event.buttons;continue;}
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
                    let previous = w.ui.preview().map_or(w.ui.window.bounds(),|p|w.ui.window.bounds().union(p));
                    w.ui.x = old.0;
                    w.ui.y = old.1;
                    w.ui.buttons = BUTTONS;
                    let (changed, input) = w.ui.update_at(event,crate::interrupts::timer_ticks());
                    if changed {
                        damage(previous.union(w.ui.window.bounds()).union(w.ui.preview().unwrap_or(w.ui.window.bounds())));
                    }
                    if let Some(mut input) = input {
                        if input.kind==api::RESIZE {w.requested=Some((input.x as u32,input.y as u32));}
                        if input.kind==api::POINTER {input.code=crate::keyboard::modifiers();}
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
        let title_length=crate::usermode::profile().title(&mut title,second as usize);
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
            if desktop_mode() {ui.work_area=crate::desktop_shell::Shell::work_area();ui.window.rect.y=ui.window.rect.y.min((crate::desktop_shell::TASKBAR_Y-ui.window.rect.h-8).max(8));}
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
                ui.window.rect.y
            ));
            ui.minimizable=desktop_mode();
            WINDOWS[index] = Some(Window {
                drop_target:false,open_drop:false,
                owner,
                id,
                ui,
                title,
                length: title_length,
                surface,
                requested:None,
                width,
                height,
                dirty: None,
                io_blocks: None,
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
    if number==85 {
        let Some(limits)=api::resize_limits(second,third,w.width,w.height)else{return (-22i32) as u32;};
        w.ui.limits=Some(limits);
        w.ui.cancel_resize();
        w.requested=None;
        // Enabling resize adds a title button and moves minimize. Present the
        // changed chrome now, even if the pointer never visits this window.
        let bounds=w.ui.window.bounds();
        unsafe {damage(bounds);asm!("sti",options(nostack));}
        flush();
        unsafe {asm!("cli",options(nostack));}
        return 0;
    }
    if number==86 {if third!=0{return (-22i32) as u32;}let size=(second&65535,second>>16);if w.requested!=Some(size){return (-116i32) as u32;}let Some(l)=w.ui.limits else{return (-22i32) as u32;};if !api::valid_size(size.0,size.1)||size.0<l.0 as u32||size.1<l.1 as u32||size.0>l.2 as u32||size.1>l.3 as u32{return (-22i32) as u32;}
        let old=w.ui.window.bounds();
        if size!=(w.width,w.height) {
            let Some(surface)=Frames::new((size.0 as usize*size.1 as usize*4).div_ceil(4096))else{w.requested=None;w.ui.cancel_resize();return (-12i32) as u32;};
            let pixels=unsafe{core::slice::from_raw_parts_mut(surface.base() as *mut u32,(size.0*size.1) as usize)};pixels.fill(0xffffff);
            let source=unsafe{core::slice::from_raw_parts(w.surface.base() as *const u32,(w.width*w.height) as usize)};
            for y in 0..size.1.min(w.height) as usize {let n=size.0.min(w.width) as usize;pixels[y*size.0 as usize..y*size.0 as usize+n].copy_from_slice(&source[y*w.width as usize..y*w.width as usize+n]);}
            w.surface=surface;w.width=size.0;w.height=size.1;
        }
        w.ui.resized(size.0 as i32,size.1 as i32);w.requested=None;w.dirty=None;
        let bounds=old.union(w.ui.window.bounds());crate::debug::write_fmt(format_args!("Multiwindow: resized pid={} surface={:08x} client={}x{} pos={},{} maximized={}\n",owner,w.surface.base(),w.width,w.height,w.ui.window.rect.x,w.ui.window.rect.y,w.ui.is_maximized()));unsafe{damage(bounds);asm!("sti",options(nostack));}flush();unsafe{asm!("cli",options(nostack));}return 0;
    }
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
            let mut texts = [[0u8; 96]; 16];
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
                } else if cmd.kind==2 {
                    c.label(cmd.x as i32,cmd.y as i32,cmd.width,cmd.color,&texts[i][..cmd.length as usize]);
                    (cmd.width,cmd.height)
                } else if cmd.kind==3 {
                    c.paragraph(cmd.x as i32,cmd.y as i32,cmd.width,cmd.height,cmd.color,&texts[i][..cmd.length as usize]);
                    (cmd.width,cmd.height)
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

pub fn terminal_present(owner:u32,handle:u32,cells:&[crate::pty_model::Cell;2720],rows:u64,cursor:Option<(usize,usize)>)->u32 {
 let Some(index)=(unsafe {(&*core::ptr::addr_of!(WINDOWS)).iter().position(|w|w.as_ref().is_some_and(|w|w.owner==owner&&w.id==handle))}) else{return (-9i32) as u32;};
 unsafe {let w=WINDOWS[index].as_mut().unwrap();if !(640..=792).contains(&w.width)||!(400..=544).contains(&w.height) {return (-22i32) as u32;}let pixels=core::slice::from_raw_parts_mut(w.surface.base() as *mut u32,w.width as usize*w.height as usize);asm!("sti",options(nostack));crate::graphics::terminal_cells(pixels,w.width as usize,w.height as usize,cells,rows,cursor);let client=w.ui.client();
 for y in 0..w.height/16 {if w.ui.window.visible&&rows&(1<<y)!=0 {damage(Rect{x:client.x,y:client.y+y as i32*16,w:640,h:16});}}flush();asm!("cli",options(nostack));}0
}

pub fn redraw(){unsafe {damage(Rect{x:0,y:0,w:800,h:600});}flush();}

struct Drag {source:u32,target:u32,token:u32,active:bool,taken:bool,payload:crate::directory_protocol::FileClipboard}
static mut DRAG:Option<Drag>=None;
static mut DRAG_TOKEN:u32=1;
static mut DESKTOP_DROP:bool=false;
pub fn desktop_loaded(pid:Option<u32>,token:u32) {unsafe {
    if token!=0&&DESKTOP_DROP&&DRAG.as_ref().is_some_and(|d|d.token==token&&!d.active) {
        DESKTOP_DROP=false;if let Some(pid)=pid {DRAG.as_mut().unwrap().target=pid;}else{cancel_drag();}
    }
}}

unsafe fn cancel_drag() {
    if let Some(d)=DRAG.take() {for w in WINDOWS.iter_mut().flatten() {if w.owner==d.source||w.owner==d.target {w.push(api::Event{kind:api::DRAG_END,..api::Event::default()});}}}
    DESKTOP_DROP=false;
    CAPTURE=None;for w in WINDOWS.iter_mut().flatten(){w.ui.update(crate::mouse_packet::Event{lost:true,buttons:BUTTONS,..crate::mouse_packet::Event::default()});}
    damage(Rect{x:0,y:0,w:800,h:544});
}
unsafe fn drop_hit()->Option<usize> {
    ORDER.iter().rev().copied().find(|&i|WINDOWS[i].as_ref().is_some_and(|w|w.ui.window.visible&&w.ui.window.rect.contains(POINTER.0,POINTER.1)))
        .filter(|&i|WINDOWS[i].as_ref().is_some_and(|w|w.drop_target&&{let c=w.ui.client();(if w.open_drop {c}else{Rect{x:c.x+8,y:c.y+80,w:c.w-16,h:c.h-112}}).contains(POINTER.0,POINTER.1)}))
}
unsafe fn drag_pointer(buttons:u8,lost:bool,previous:(i32,i32)) {
    if let Some(shell)=SHELL.as_mut(){shell.reset(buttons);}
    if lost {cancel_drag();return;}
    let desktop=wallpaper_hit();let target=drop_hit();let Some(d)=DRAG.as_mut() else{return;};
    let old=d.target;d.target=target.and_then(|i|WINDOWS[i].as_ref().map(|w|w.owner)).unwrap_or(0);
    d.payload.cut=((crate::keyboard::modifiers()&(1<<16))!=0) as u16;
    if old!=d.target {for w in WINDOWS.iter_mut().flatten(){if w.owner==old {w.push(api::Event{kind:api::DRAG_OVER,x:-1,y:-1,..api::Event::default()});}}}
    if buttons&1==0 {
        if let Some(i)=target {
            d.active=false;let w=WINDOWS[i].as_mut().unwrap();let c=w.ui.client();w.push(api::Event{kind:api::DROP,code:d.token,x:POINTER.0-c.x,y:POINTER.1-c.y,buttons:0});
            for w in WINDOWS.iter_mut().flatten(){w.ui.update(crate::mouse_packet::Event{lost:true,buttons:0,..crate::mouse_packet::Event::default()});}
            CAPTURE=None;focus(Some(i));
            crate::debug::write_fmt(format_args!("Drag: drop target={}\n",DRAG.as_ref().unwrap().target));
        }else if desktop {
            if d.source==0&&d.payload.volume==crate::diskfs::volume_epoch() {
                if let Some(shell)=SHELL.as_mut() {
                    let path=&d.payload.paths[0][..d.payload.lengths[0] as usize];
                    if let Some(source)=(0..shell.file_count).find(|&i|path.len()==9+shell.files[i].length&&path[9..]==shell.files[i].name[..shell.files[i].length]) {
                        let folder=(0..shell.file_count).any(|i|shell.selection&(1<<i)==0&&shell.files[i].folder&&shell.file_rect(i).contains(POINTER.0,POINTER.1));
                        if !folder {
                            let slot=crate::desktop_shell::Shell::nearest_slot(POINTER.0,POINTER.1);shell.arrange_group(source,slot);
                            crate::debug::write_fmt(format_args!("Desktop: arranged slot={}\n",slot));cancel_drag();return;
                        }
                    }
                }
            }
            let mut destination=[0;128];destination[..8].copy_from_slice(b"/DESKTOP");let mut n=8;
            if let Some(shell)=SHELL.as_ref() {for i in 0..shell.file_count {let e=shell.files[i];if e.folder&&shell.file_rect(i).contains(POINTER.0,POINTER.1) {destination[8]=b'/';destination[9..9+e.length].copy_from_slice(&e.name[..e.length]);n=9+e.length;break;}}}
            if queue_desktop(&destination[..n],d.payload.volume,true) {LAUNCH.as_mut().unwrap().4=d.token;d.active=false;DESKTOP_DROP=true;CAPTURE=None;if let Some(s)=SHELL.as_mut(){s.reset(0);}crate::debug::write_fmt(format_args!("Drag: queued desktop transfer\n"));}else{cancel_drag();}
        }else {cancel_drag();}
    }else if let Some(i)=target {let w=WINDOWS[i].as_mut().unwrap();let c=w.ui.client();w.events.push(api::Event{kind:api::DRAG_OVER,code:d.payload.cut as u32,x:POINTER.0-c.x,y:POINTER.1-c.y,buttons:buttons as u32},true);}
    if buttons&1==0||DRAG.as_ref().is_none_or(|d|d.target!=old) {damage(Rect{x:0,y:0,w:800,h:544});}
    else {let badge=|p:(i32,i32)|Rect{x:(p.0+16).min(680),y:(p.1+18).min(516),w:116,h:24};damage(badge(previous).union(badge(POINTER)));}
}
pub fn paint_drag(canvas:&mut crate::canvas::Canvas<'_>) {
    unsafe {let Some(d)=DRAG.as_ref().filter(|d|d.active) else{return;};
        let color=if let Some(i)=drop_hit(){let c=WINDOWS[i].as_ref().unwrap().ui.client();let r=if WINDOWS[i].as_ref().unwrap().open_drop {c}else{Rect{x:c.x+8,y:c.y+80,w:c.w-16,h:c.h-112}};for (x,y,w,h) in [(r.x,r.y,r.w,2),(r.x,r.y+r.h-2,r.w,2),(r.x,r.y,2,r.h),(r.x+r.w-2,r.y,2,r.h)] {canvas.rect(x,y,w,h,0x238a60);}0x238a60}else if wallpaper_hit(){0x238a60}else{0x606c78};
        let x=(POINTER.0+16).min(680);let y=(POINTER.1+18).min(516);canvas.rect(x,y,116,24,color);canvas.text(x+6,y+4,0xffffff,if d.source==0&&wallpaper_hit() {b"Arrange / drop"}else if drop_hit().is_some_and(|i|WINDOWS[i].as_ref().unwrap().open_drop) {b"Open file"}else if d.payload.cut==0 {b"Copy files"}else{b"Move files"},&crate::ui_font::FONT);
    }
}
pub fn drag_syscall(cs:u32,op:u32,pointer:u32,size:u32)->u32 {
    let Some(image)=usermode::active_image(cs) else{return u32::MAX;};let owner=usermode::getpid();
    unsafe {
        let Some(index)=WINDOWS.iter().position(|w|w.as_ref().is_some_and(|w|w.owner==owner)) else{return (-9i32) as u32;};
        if matches!(op,0|4) {if pointer|size!=0{return (-22i32) as u32;}let w=WINDOWS[index].as_mut().unwrap();w.drop_target=true;w.open_drop=op==4;if op==0 {if let Some(d)=DRAG.as_ref().filter(|d|!d.active&&d.target==owner&&!d.taken){w.push(api::Event{kind:api::DROP,code:d.token,x:8,y:0,..api::Event::default()});}}return 0;}
        if op==3 {if pointer|size!=0{return (-22i32) as u32;}if DRAG.as_ref().is_some_and(|d|d.target==owner&&!d.active){cancel_drag();return 0;}return (-9i32) as u32;}
        if size!=3468||!matches!(op,1|2) {return (-22i32) as u32;}
        if op==1 {
            if DRAG.is_some()||CAPTURE!=Some(index)||BUTTONS&1==0||!WINDOWS[index].as_ref().unwrap().drop_target||WINDOWS[index].as_ref().unwrap().open_drop {return (-16i32) as u32;}
            if !user_buffer::readable_range(pointer,size,image){return (-14i32) as u32;}
            let mut payload=core::ptr::read_unaligned(pointer as *const crate::directory_protocol::FileClipboard);
            if payload.count==0||payload.count>32||payload.cut>1{return (-22i32) as u32;}
            if !crate::diskfs::is_mounted(){return (-19i32) as u32;}
            if payload.volume!=crate::diskfs::volume_epoch(){return (-116i32) as u32;}
            if let Err(e)=crate::file_clipboard::validate(&mut payload){return e;}
            let token=DRAG_TOKEN;DRAG_TOKEN=DRAG_TOKEN.wrapping_add(1).max(1);payload.version=token;
            DRAG=Some(Drag{source:owner,target:0,token,active:true,taken:false,payload});damage(Rect{x:0,y:0,w:800,h:544});
            crate::debug::write_fmt(format_args!("Drag: begin source={} count={}\n",owner,payload.count));return 0;
        }
        if !user_buffer::writable(pointer,size){return (-14i32) as u32;}
        let Some(d)=DRAG.as_mut().filter(|d|!d.active&&!d.taken&&d.target==owner) else{return (-9i32) as u32;};
        let expected=core::ptr::read_unaligned(pointer as *const crate::directory_protocol::FileClipboard).version;
        if expected!=d.token||d.payload.volume!=crate::diskfs::volume_epoch(){return (-116i32) as u32;}
        core::ptr::copy_nonoverlapping(&d.payload as *const _ as *const u8,pointer as *mut u8,3468);d.taken=true;0
    }
}

fn refresh_desktop() {
    unsafe {
        let Some(shell)=SHELL.as_mut() else{return;};
        let volume=crate::diskfs::volume_epoch();
        if shell.file_volume!=volume {shell.file_count=0;shell.selection=0;shell.selected=None;shell.file_volume=volume;shell.layout_loaded=false;shell.layout_dirty=false;shell.context=None;damage(Rect{x:132,y:24,w:416,h:520});}
        if crate::disk_io::busy()||DRAG.is_some()||shell.capture.is_some()||shell.context.is_some(){return;}
        let mut files=[crate::desktop_shell::DesktopEntry::EMPTY;20];let mut count=0;
        let _=crate::diskfs::list("/DESKTOP",|name,_,folder| {if count<20 {let mut label=[0;12];let n=crate::fat16::display_name(name,&mut label);files[count]=crate::desktop_shell::DesktopEntry{name:label,length:n,folder};count+=1;}});
        files[..count].sort_unstable_by(|a,b|b.folder.cmp(&a.folder).then(a.name.cmp(&b.name)));
        if files!=shell.files||count!=shell.file_count {shell.update_files(files,count);damage(Rect{x:132,y:24,w:416,h:520});}
        if !shell.layout_loaded {
            shell.layout_loaded=true;let mut data=[0;crate::fat16::MAX_FILE_SIZE];
            if let Ok(n)=crate::diskfs::read("/DESK.CFG",&mut data) {if shell.apply_layout(&data[..n]) {crate::debug::write_fmt(format_args!("Desktop: layout restored\n"));damage(Rect{x:132,y:24,w:416,h:520});}}
        }

    }
}
unsafe fn queue_desktop(path:&[u8],volume:u32,folder:bool)->bool {
    let Some(shell)=SHELL.as_ref() else{return false;};
    if LAUNCH.is_some()||shell.tasks.iter().all(Option::is_some) {status(b"Close a window before opening a desktop item.");crate::system_sounds::play(crate::system_sounds::Event::Warning);return false;}
    let Some(i)=shell.find(b"FILES") else{return false;};
    let mut args=[0;128];let prefix=if folder {b"--folder " as &[u8]}else{b""};args[..prefix.len()].copy_from_slice(prefix);args[prefix.len()..prefix.len()+path.len()].copy_from_slice(path);
    LAUNCH=Some((shell.catalog.entries[i],volume,args,prefix.len()+path.len(),0));true
}
unsafe fn wallpaper_hit()->bool {
    SHELL.as_ref().is_some_and(|s|!s.open&&!s.sound_open&&s.context.is_none())&&POINTER.1<544&&
    !WINDOWS.iter().flatten().any(|w|w.ui.window.visible&&w.ui.window.rect.contains(POINTER.0,POINTER.1))
}

fn save_desktop_layout() {
    if crate::disk_io::busy(){return;}
    unsafe {let Some(shell)=SHELL.as_mut() else{return;};
        if !shell.layout_dirty||DRAG.is_some()||shell.capture.is_some()||shell.context.is_some(){return;}
        shell.layout_dirty=false;
        if shell.file_volume!=crate::diskfs::volume_epoch()||!crate::diskfs::is_mounted(){return;}
        let mut data=[0;305];let n=shell.encode_layout(&mut data);
        if crate::diskfs::save_large("/DESK.CFG",&data[..n]).is_ok(){crate::debug::write_fmt(format_args!("Desktop: layout saved\n"));}
        else {crate::debug::write_fmt(format_args!("Desktop: layout save failed\n"));crate::system_sounds::play(crate::system_sounds::Event::Error);}
    }
}

unsafe fn desktop_file_action(op:u8) {
    let Some(shell)=SHELL.as_ref() else{return;};
    if op<2||op==3 {
        if shell.selection==0 {crate::system_sounds::play(crate::system_sounds::Event::Empty);return;}
        let mut payload=crate::directory_protocol::FileClipboard::new();payload.cut=if op==3 {0}else{op as u16};payload.volume=shell.file_volume;
        for i in 0..shell.file_count {if shell.selection&(1<<i)!=0 {let e=shell.files[i];let n=payload.count as usize;payload.paths[n][..9].copy_from_slice(b"/DESKTOP/");payload.paths[n][9..9+e.length].copy_from_slice(&e.name[..e.length]);payload.lengths[n]=(9+e.length) as u16;payload.flags[n]=e.folder as u8;payload.count+=1;}}
        if op==3 {
            if DRAG.is_some()||payload.volume!=crate::diskfs::volume_epoch()||!crate::diskfs::is_mounted(){crate::system_sounds::play(crate::system_sounds::Event::Error);return;}
            if queue_desktop(b"/DESKTOP",payload.volume,true) {
                let token=DRAG_TOKEN;DRAG_TOKEN=DRAG_TOKEN.wrapping_add(1).max(1);payload.version=token;
                let launch=LAUNCH.as_mut().unwrap();launch.2=[0;128];let args=b"--recycle /DESKTOP";launch.2[..args.len()].copy_from_slice(args);launch.3=args.len();launch.4=token;
                DRAG=Some(Drag{source:0,target:0,token,active:false,taken:false,payload});DESKTOP_DROP=true;
                crate::debug::write_fmt(format_args!("Desktop: recycle queued count={}\n",payload.count));
            }return;
        }
        let result=crate::file_clipboard::set(payload);
        if (result as i32)<0 {crate::system_sounds::play(crate::system_sounds::Event::Error);}else{crate::debug::write_fmt(format_args!("Desktop: clipboard count={} cut={}\n",payload.count,op));}
        return;
    }
    let clip=crate::file_clipboard::snapshot();
    if clip.count==0 {crate::system_sounds::play(crate::system_sounds::Event::Empty);return;}
    if DRAG.is_some()||clip.volume!=crate::diskfs::volume_epoch(){crate::system_sounds::play(crate::system_sounds::Event::Error);return;}
    if queue_desktop(b"/DESKTOP",clip.volume,true) {
        let launch=LAUNCH.as_mut().unwrap();let mut args=[0;128];args[..8].copy_from_slice(b"--paste ");let mut v=clip.version;
        for i in (8..18).rev(){args[i]=b'0'+(v%10) as u8;v/=10;}args[18]=b' ';args[19..27].copy_from_slice(b"/DESKTOP");launch.2=args;launch.3=27;
        crate::debug::write_fmt(format_args!("Desktop: paste queued version={}\n",clip.version));
    }
}
