// Kernel-owned pseudo-terminal sessions. Only serialized host/syscall code accesses them.
use crate::{
    fs_path::Path,
    loader::Frames,
    pty_model::{Cell, Console},
};
use core::fmt::Write;
struct Slot {
    id: u32,
    owner: u32,
    frames: Frames,
}
struct Session {
    console: Console,
    shell: Option<crate::shell::Shell>,
    directory: Path,
    keys: [u32; 1024],
    head: usize,
    len: usize,
    pub foreground: u32,
    pending: bool,
    kill: u32,
    closed: bool,
    cache: [Cell; 2000],
    frozen: [Cell; 2000],
    selection: Option<(usize, usize)>,
    freeze: bool,
    last_cursor: Option<(usize, usize)>,
    view: usize,
}
impl Session {
    fn new() -> Self {
        Self {
            console: Console::new(),
            shell: Some(crate::shell::Shell::new()),
            directory: crate::diskfs::directory(),
            keys: [0; 1024],
            head: 0,
            len: 0,
            foreground: 0,
            pending: false,
            kill: 0,
            closed: false,
            cache: [Cell { ch: 0, color: 0 }; 2000],
            frozen: [Cell { ch: 0, color: 0 }; 2000],
            selection: None,
            freeze: false,
            last_cursor: None,
            view: 0,
        }
    }
}
const _: () = assert!(core::mem::size_of::<Session>() <= 32768);
static mut SLOTS: [Option<Slot>; 4] = [const { None }; 4];
static mut CURRENT: Option<u32> = None;
static mut NEXT: u32 = 1;
pub fn current() -> Option<u32> {
    unsafe { *core::ptr::addr_of!(CURRENT) }
}
pub fn set_current(id: Option<u32>) {
    unsafe {
        CURRENT = id;
    }
}
fn pointer(id: u32) -> Option<*mut Session> {
    unsafe {
        (&*core::ptr::addr_of!(SLOTS))
            .iter()
            .flatten()
            .find(|s| s.id == id)
            .map(|s| s.frames.base() as *mut Session)
    }
}
pub fn for_owner(owner: u32) -> Option<u32> {
    unsafe {
        (&*core::ptr::addr_of!(SLOTS))
            .iter()
            .flatten()
            .find(|s| s.owner == owner)
            .map(|s| s.id)
    }
}
pub fn exists(id: u32) -> bool {
    pointer(id).is_some_and(|p| unsafe { !(*p).closed })
}
pub fn console() -> Option<&'static mut Console> {
    let p = pointer(current()?)?;
    Some(unsafe { &mut (*p).console })
}
pub fn write(text: &[u8]) {
    if let Some(c) = console() {
        c.write(text);
    }
}
pub fn print(args: core::fmt::Arguments) {
    struct Writer;
    impl core::fmt::Write for Writer {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            write(s.as_bytes());
            Ok(())
        }
    }
    let _ = Writer.write_fmt(args);
}
pub fn pending(id: u32) -> bool {
    pointer(id).is_some_and(|p| unsafe { (*p).len != 0 || (*p).closed })
}
pub fn read_key(id: u32) -> Option<u32> {
    let p = pointer(id)?;
    unsafe {
        let s = &mut *p;
        if s.len == 0 {
            return None;
        }
        let key = s.keys[s.head];
        s.head = (s.head + 1) % 1024;
        s.len -= 1;
        Some(key)
    }
}
pub fn can_launch(id: u32) -> bool {
    pointer(id).is_some_and(|p| unsafe { (*p).foreground == 0 && !(*p).pending && !(*p).closed })
}
pub fn launch(name: &str, args: &str) -> bool {
    let Some(id) = current() else {
        return false;
    };
    let mut line = [0; 128];
    let n = name.len() + usize::from(!args.is_empty()) + args.len();
    if n > 128 {
        crate::println!("Program arguments exceed 128 bytes.");
        return false;
    }
    line[..name.len()].copy_from_slice(name.as_bytes());
    if !args.is_empty() {
        line[name.len()] = b' ';
        line[name.len() + 1..n].copy_from_slice(args.as_bytes());
    }
    match crate::multitask::request_launch(core::str::from_utf8(&line[..n]).unwrap()) {
        Ok(()) => {
            if let Some(p) = pointer(id) {
                unsafe {
                    (*p).pending = true;
                }
            }
            true
        }
        Err(_) => {
            crate::println!("No free process slot, or a launch is already pending.");
            false
        }
    }
}
pub fn attach(id: u32, pid: u32) {
    if let Some(p) = pointer(id) {
        unsafe {
            (*p).foreground = pid;
            (*p).pending = false;
            (*p).len = 0;
        }
    }
}
pub fn child_done(id: u32, pid: u32, message: &str) {
    let Some(p) = pointer(id) else {
        return;
    };
    unsafe {
        let s = &mut *p;
        if s.closed {
            return;
        }
        if s.foreground != pid && !(pid == 0 && s.pending) {
            return;
        }
        s.foreground = 0;
        s.pending = false;
        s.len = 0;
        s.console.color = 0x0a;
        s.console.visible = true;
        s.console.position(0, 24);
    }
    let previous = current();
    set_current(Some(id));
    let directory = crate::diskfs::directory();
    crate::diskfs::set_directory(unsafe { (*p).directory });
    crate::println!("{}", message);
    crate::println!();
    let mut shell = unsafe { (*p).shell.take().unwrap() };
    shell.prompt();
    unsafe {
        (*p).shell = Some(shell);
    }
    crate::diskfs::set_directory(directory);
    set_current(previous);
}
pub fn exit_shell(id: u32) {
    if let Some(p) = pointer(id) {
        unsafe {
            (*p).closed = true;
        }
    }
}
pub fn close_owner(owner: u32) {
    unsafe {
        for slot in &mut *core::ptr::addr_of_mut!(SLOTS) {
            if slot.as_ref().is_some_and(|s| s.owner == owner) {
                if let Some(s) = slot.take() {
                    let p = s.frames.base() as *mut Session;
                    let child = (*p).foreground;
                    if child != 0 {
                        crate::multitask::queue_kill(child);
                    }
                    core::ptr::drop_in_place(p);
                }
            }
        }
    }
}
pub fn take_kill() -> Option<u32> {
    unsafe {
        for slot in (&*core::ptr::addr_of!(SLOTS)).iter().flatten() {
            let s = &mut *(slot.frames.base() as *mut Session);
            if s.kill != 0 {
                let pid = s.kill;
                s.kill = 0;
                return Some(pid);
            }
        }
    }
    None
}
pub fn finish() {
    unsafe {
        CURRENT = None;
        SLOTS = [const { None }; 4];
    }
}
pub fn service() {
    let mut ids = [0; 4];
    unsafe {
        for (i, slot) in (&*core::ptr::addr_of!(SLOTS)).iter().enumerate() {
            if let Some(slot) = slot {
                ids[i] = slot.id;
            }
        }
    }
    for id in ids {
        let Some(p) = pointer(id) else {
            continue;
        };
        if unsafe { (*p).closed || (*p).pending || (*p).foreground != 0 } {
            continue;
        }
        let previous = current();
        let directory = crate::diskfs::directory();
        set_current(Some(id));
        crate::diskfs::set_directory(unsafe { (*p).directory });
        let mut shell = unsafe { (*p).shell.take().unwrap() };
        for _ in 0..64 {
            let Some(key) = read_key(id) else {
                break;
            };
            if key & (1 << 17) != 0 && matches!(key as u16, 68 | 100) && shell.length == 0 {
                exit_shell(id);
                break;
            }
            if let Some((line, n)) = shell.key(key) {
                crate::run_command(core::str::from_utf8(&line[..n]).unwrap_or(""));
                if unsafe { (*p).pending || (*p).closed } {
                    break;
                }
                crate::println!();
                shell.prompt();
            }
        }
        unsafe {
            (*p).directory = crate::diskfs::directory();
            (*p).shell = Some(shell);
        }
        crate::diskfs::set_directory(directory);
        set_current(previous);
    }
}
fn key(id: u32, key: u32) -> u32 {
    let Some(p) = pointer(id) else {
        return (-9i32) as u32;
    };
    unsafe {
        let s = &mut *p;
        if key & !0x000fffff != 0 || !matches!(key as u16,0..=127|0x100..=0x129) {
            return (-22i32) as u32;
        }
        s.freeze = false;
        s.selection = None;
        s.view = 0;
        s.console.touch();
        if key & (1 << 17) != 0
            && key & (1 << 16) == 0
            && matches!(key as u16, 67 | 99)
            && s.foreground != 0
        {
            s.kill = s.foreground;
            return 0;
        }
        if s.len == 1024 {
            return (-28i32) as u32;
        }
        s.keys[(s.head + s.len) % 1024] = key;
        s.len += 1;
        0
    }
}
fn owned(id: u32, owner: u32) -> Option<*mut Session> {
    unsafe {
        (&*core::ptr::addr_of!(SLOTS))
            .iter()
            .flatten()
            .find(|s| s.id == id && s.owner == owner)
            .map(|s| s.frames.base() as *mut Session)
    }
}
pub fn syscall(cs: u32, number: u32, a: u32, b: u32, c: u32) -> u32 {
    let Some(image) = crate::usermode::active_image(cs) else {
        return u32::MAX;
    };
    let owner = crate::usermode::getpid();
    if number == 72 && a == 0 {
        if b | c != 0 {
            return (-22i32) as u32;
        }
        if !crate::multitask::active()
            || !crate::multi_window::desktop_mode()
            || !crate::multi_window::has_window(owner)
        {
            return (-16i32) as u32;
        }
        unsafe {
            if (&*core::ptr::addr_of!(SLOTS))
                .iter()
                .flatten()
                .any(|s| s.owner == owner)
            {
                return (-16i32) as u32;
            }
        }
        let Some(index) = (unsafe {
            (&*core::ptr::addr_of!(SLOTS))
                .iter()
                .position(Option::is_none)
        }) else {
            return (-24i32) as u32;
        };
        let Some(frames) = Frames::new(8) else {
            return (-12i32) as u32;
        };
        let p = frames.base() as *mut Session;
        unsafe {
            p.write(Session::new());
            let id = NEXT;
            if id > i32::MAX as u32 {
                return (-24i32) as u32;
            }
            NEXT += 1;
            SLOTS[index] = Some(Slot { id, owner, frames });
            let old = current();
            set_current(Some(id));
            crate::println!("Welcome to Xinyi.");
            crate::println!("Type help for the same shell commands as the console.");
            crate::println!();
            let mut shell = (*p).shell.take().unwrap();
            shell.prompt();
            (*p).shell = Some(shell);
            set_current(old);
            crate::debug::write_fmt(format_args!(
                "PTY: open id={} owner={} font={:08x}\n",
                id,
                owner,
                crate::graphics::font_address()
            ));
            return id;
        }
    }
    let id = if number == 72 { b } else { a };
    let Some(p) = owned(id, owner) else {
        return (-9i32) as u32;
    };
    if number == 72 {
        if c != 0 {
            return (-22i32) as u32;
        }
        unsafe {
            match a {
                1 if (*p).foreground != 0 || (*p).pending => (-16i32) as u32,
                1 | 2 => {
                    (*p).closed = true;
                    if (*p).foreground != 0 {
                        (*p).kill = (*p).foreground;
                    }
                    0
                }
                3 => (*p).foreground,
                4 => u32::from((*p).closed),
                5 => {
                    (*p).cache.fill(Cell { ch: 0, color: 0 });
                    (*p).last_cursor = None;
                    0
                }
                _ => (-22i32) as u32,
            }
        }
    } else if number == 74 {
        if c != 0 {
            return (-22i32) as u32;
        }
        key(id, b)
    } else if number == 75 {
        if c > 1024 {
            return (-22i32) as u32;
        }
        if !crate::user_buffer::readable(b, c, image) {
            return (-14i32) as u32;
        }
        let mut text = [0; 1024];
        if c != 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(b as *const u8, text.as_mut_ptr(), c as usize);
            }
        }
        if !text[..c as usize]
            .iter()
            .all(|b| matches!(*b, 9 | 10 | 13 | 32..=126))
        {
            return (-22i32) as u32;
        }
        let shell = unsafe { (*p).foreground == 0 && !(*p).pending };
        let mut converted = [0; 1024];
        let (mut n, mut cr) = (0, false);
        for &byte in &text[..c as usize] {
            if byte == 10 && cr {
                cr = false;
                continue;
            }
            cr = byte == 13;
            let count = if shell && byte == 9 { 4 } else { 1 };
            if n + count > 1024 {
                return (-28i32) as u32;
            }
            converted[n..n + count].fill(if shell && matches!(byte, 9 | 10 | 13) {
                b' '
            } else if byte == 13 {
                10
            } else {
                byte
            });
            n += count;
        }
        unsafe {
            let s = &*p;
            if n > 1024 - s.len || shell && n + s.shell.as_ref().map_or(0, |s| s.length) > 128 {
                return (-28i32) as u32;
            }
        }
        for &byte in &converted[..n] {
            key(id, byte as u32);
        }
        n as u32
    } else if number == 76 {
        if b > 2000 || c > 2000 {
            return (-22i32) as u32;
        }
        unsafe {
            let s = &mut *p;
            if !s.freeze {
                s.console.snapshot(s.view, &mut s.frozen);
                s.freeze = true;
            }
            s.selection = Some((b.min(c) as usize, b.max(c) as usize));
            s.console.touch();
        }
        0
    } else if number == 77 {
        if c != 0 || b > 1 {
            return (-22i32) as u32;
        }
        if b == 1 {
            unsafe {
                (*p).selection = None;
                (*p).freeze = false;
                (*p).console.touch();
            }
            return 0;
        }
        let Some((start, end)) = (unsafe { (*p).selection }) else {
            return 0;
        };
        if start == end {
            return 0;
        }
        let mut text = [0; 1024];
        let mut n = 0;
        unsafe {
            for row in start / 80..=(end - 1) / 80 {
                let from = if row == start / 80 { start % 80 } else { 0 };
                let until = if row == (end - 1) / 80 {
                    (end - 1) % 80 + 1
                } else {
                    80
                };
                let cells = &(&(*p).frozen)[row * 80..row * 80 + 80];
                let last = cells[..until]
                    .iter()
                    .rposition(|v| v.ch != b' ')
                    .map_or(from, |i| i + 1)
                    .max(from);
                for cell in &cells[from..last] {
                    if n == 1024 {
                        return (-28i32) as u32;
                    }
                    text[n] = if matches!(cell.ch, 32..=126) {
                        cell.ch
                    } else {
                        b'?'
                    };
                    n += 1;
                }
                if row != (end - 1) / 80 {
                    if n == 1024 {
                        return (-28i32) as u32;
                    }
                    text[n] = 10;
                    n += 1;
                }
            }
        }
        crate::clipboard::set_text(&text[..n]);
        n as u32
    } else if number == 73 {
        if c > 64 {
            return (-22i32) as u32;
        }
        unsafe {
            if (*p).closed {
                return (-32i32) as u32;
            }
            let s = &mut *p;
            s.view = (c as usize).min(s.console.history_len);
            let mut cells = [Cell { ch: 0, color: 0 }; 2000];
            if s.freeze {
                cells = s.frozen;
            } else {
                s.console.snapshot(s.view, &mut cells);
            }
            if let Some((a, b)) = s.selection {
                for cell in &mut cells[a..b] {
                    cell.color = 0x1f;
                }
            }
            let cursor = if s.view == 0 && !s.freeze && s.console.visible {
                Some((s.console.x, s.console.y))
            } else {
                None
            };
            let mut rows = 0u32;
            for y in 0..25 {
                if cells[y * 80..y * 80 + 80] != s.cache[y * 80..y * 80 + 80] {
                    rows |= 1 << y;
                }
            }
            if cursor != s.last_cursor {
                if let Some((_, y)) = cursor {
                    rows |= 1 << y;
                }
                if let Some((_, y)) = s.last_cursor {
                    rows |= 1 << y;
                }
            }
            if rows == 0 {
                return 0;
            }
            let result = crate::multi_window::terminal_present(owner, b, &cells, rows, cursor);
            if result == 0 {
                s.cache = cells;
                s.last_cursor = cursor;
            }
            result
        }
    } else {
        u32::MAX
    }
}
