// Timer preemption only at ring 3; Rust kernel frames are never suspended.
use crate::{
    cpu_context::Cpu,
    loader::{self, Frames},
    paging,
    scheduler_model::{self, State},
    syscall_frame::SyscallFrame,
    usermode,
};
use core::arch::asm;
pub const LIMIT: usize = 8;
#[derive(Clone, Copy)]
enum Pause {
    Timer,
    Yield,
    Sleep(u32),
    Window,
    Console(u32),
    Tty(u32),
    Detach(u32),
}
static mut ACTIVE: bool = false;
static mut DESKTOP: bool = false;
static mut PAUSE: Option<Pause> = None;
static mut SAVED: Option<Cpu> = None;
struct Launch {
    name: [u8; 105],
    length: usize,
    args: [u8; 128],
    argc: usize,
    directory: crate::fs_path::Path,
    tty: Option<u32>,
}
static mut REQUEST: Option<Launch> = None;
static mut RUNNING: usize = 0;
static mut ROOTS: usize = 0;
static mut KILLS: [u32; 8] = [0; 8];
pub fn queue_kill(pid: u32) {
    unsafe {
        if let Some(slot) = (&mut *core::ptr::addr_of_mut!(KILLS))
            .iter_mut()
            .find(|v| **v == 0)
        {
            *slot = pid;
        }
    }
}
fn take_kill() -> Option<u32> {
    unsafe {
        for slot in &mut *core::ptr::addr_of_mut!(KILLS) {
            if *slot != 0 {
                let pid = *slot;
                *slot = 0;
                return Some(pid);
            }
        }
    }
    None
}
pub fn request_launch(command: &str) -> Result<(), i32> {
    if !active() || !unsafe { DESKTOP } {
        return Err(-16);
    }
    if crate::pty::current().is_some_and(|id| !crate::pty::can_launch(id)) {
        return Err(-16);
    }
    if unsafe { RUNNING } >= LIMIT || crate::pty::current().is_none() && unsafe { ROOTS } >= 4 {
        return Err(-24);
    }
    if unsafe { (&*core::ptr::addr_of!(REQUEST)).is_some() } {
        return Err(-16);
    }
    let (name, args) = command
        .trim()
        .split_once(char::is_whitespace)
        .unwrap_or((command.trim(), ""));
    let args = args.trim();
    if name.is_empty() || name.len() > 105 || args.len() > 128 {
        return Err(-22);
    }
    let mut request = Launch {
        name: [0; 105],
        length: name.len(),
        args: [0; 128],
        argc: args.len(),
        directory: crate::diskfs::directory(),
        tty: crate::pty::current(),
    };
    request.name[..name.len()].copy_from_slice(name.as_bytes());
    request.args[..args.len()].copy_from_slice(args.as_bytes());
    unsafe {
        REQUEST = Some(request);
    }
    pause(Pause::Yield);
    Ok(())
}
pub fn active() -> bool {
    unsafe { *core::ptr::addr_of!(ACTIVE) }
}
fn pause(reason: Pause) {
    unsafe {
        *core::ptr::addr_of_mut!(PAUSE) = Some(reason);
    }
}
pub fn wait_window() {
    pause(Pause::Window);
}
pub fn syscall(number: u32, first: u32, second: u32, third: u32) -> Option<u32> {
    if !active() {
        return None;
    }
    if let Some(id) = crate::pty::current() {
        if matches!(number, 60 | 65) {
            pause(Pause::Detach(number));
            return Some(0);
        }
        if number == 50 {
            if first > 1 || second | third != 0 {
                return Some((-22i32) as u32);
            }
            if let Some(key) = crate::pty::read_key(id) {
                return Some(key);
            }
            if first != 0 {
                pause(Pause::Tty(id));
            }
            return Some(0);
        }
        if matches!(number, 4 | 53..=56) {
            return None;
        }
    }
    if matches!(number, 4 | 50 | 53..=56) {
        if unsafe { DESKTOP } && !crate::multi_window::has_window(usermode::getpid()) {
            pause(Pause::Console(number));
            return Some(0);
        }
        return Some((-16i32) as u32);
    }
    if matches!(number, 57 | 58) && !unsafe { DESKTOP } {
        return Some((-16i32) as u32);
    }
    if number == 52 {
        if first > 60_000 || second != 0 || third != 0 {
            return Some((-22i32) as u32);
        }
        let ticks = if first == 0 { 0 } else { (first + 9) / 10 + 1 };
        pause(Pause::Sleep(
            crate::interrupts::timer_ticks().wrapping_add(ticks),
        ));
        return Some(0);
    }
    if number == 66 {
        if first | second | third != 0 {
            return Some((-22i32) as u32);
        }
        pause(Pause::Yield);
        return Some(0);
    }
    None
}
#[no_mangle]
pub extern "C" fn xinyi_scheduled_irq(frame: &SyscallFrame, segments: *const u32) -> u32 {
    if !active() || frame.vector != 32 || frame.cs & 3 != 3 {
        return 0;
    }
    unsafe {
        *core::ptr::addr_of_mut!(SAVED) = Some(Cpu::capture(frame, segments));
    }
    pause(Pause::Timer);
    1
}
#[no_mangle]
pub extern "C" fn xinyi_scheduled_syscall(frame: &SyscallFrame, segments: *const u32) -> u32 {
    if !active() || frame.cs & 3 != 3 {
        return 0;
    }
    let pending = unsafe { *core::ptr::addr_of!(PAUSE) };
    if pending.is_none() {
        return 0;
    }
    let mut cpu = unsafe { Cpu::capture(frame, segments) };
    if let Some(number) = match pending {
        Some(Pause::Window) => Some(61),
        Some(Pause::Console(n)) => Some(n),
        Some(Pause::Tty(_)) => Some(50),
        Some(Pause::Detach(n)) => Some(n),
        _ => None,
    } {
        cpu.eax = number;
        cpu.eip = cpu.eip.wrapping_sub(2);
    }
    unsafe {
        *core::ptr::addr_of_mut!(SAVED) = Some(cpu);
    }
    1
}
#[repr(C, align(16))]
struct Fpu([u8; 108]);
impl Fpu {
    fn fresh() -> Self {
        let mut state = Self([0; 108]);
        unsafe {
            asm!("fninit", options(nostack));
        }
        state.save();
        state
    }
    fn save(&mut self) {
        unsafe {
            asm!("fnsave [{0}]",in(reg)self.0.as_mut_ptr(),options(nostack));
        }
    }
    fn restore(&self) {
        unsafe {
            asm!("frstor [{0}]",in(reg)self.0.as_ptr(),options(nostack));
        }
    }
}
struct HostFpu {
    state: Fpu,
    cr0: u32,
    cr4: u32,
}
impl HostFpu {
    fn begin() -> Self {
        let (cr0, cr4): (u32, u32);
        unsafe {
            asm!("mov {0}, cr0",out(reg)cr0);
            asm!("mov {0}, cr4",out(reg)cr4);
            asm!("mov cr0, {0}",in(reg)((cr0|0x22)&!0x0c));
            // Current freestanding ABI has SIMD disabled. Save x87/MMX state and
            // keep SSE/AVX unavailable rather than sharing unsaved SIMD registers.
            asm!("mov cr4, {0}",in(reg)(cr4&!0x40600));
        }
        let mut state = Fpu([0; 108]);
        state.save();
        Self { state, cr0, cr4 }
    }
}
impl Drop for HostFpu {
    fn drop(&mut self) {
        self.state.restore();
        unsafe {
            asm!("mov cr4, {0}",in(reg)self.cr4);
            asm!("mov cr0, {0}",in(reg)self.cr0);
        }
    }
}
struct Job {
    pid: u32,
    program: loader::LoadedProgram,
    space: paging::AddressSpace,
    stack: Frames,
    cpu: Cpu,
    fpu: Fpu,
    state: State,
    preemptions: u32,
    name: [u8; 8],
    name_length: usize,
    directory: crate::fs_path::Path,
    tty: Option<u32>,
    task_slot: Option<usize>,
    terminal_owner: Option<u32>,
}
fn load(path: &str, arguments: &[u8]) -> Result<Job, &'static str> {
    let staging = Frames::new(16).ok_or("Not enough memory for the program loader.")?;
    let data = unsafe { core::slice::from_raw_parts_mut(staging.base() as *mut u8, 65536) };
    let result = crate::diskfs::read_program(path, data);
    let result = if matches!(result, Err(crate::fat16::Error::NotFound))
        && !path.contains('/')
        && !path.contains('\\')
    {
        let extension = if path.contains('.') { "" } else { ".BIN" };
        let mut name = [0; 32];
        let len = 6 + path.len() + extension.len();
        if len > 32 {
            return Err("Invalid program name.");
        }
        name[..6].copy_from_slice(b"/APPS/");
        name[6..6 + path.len()].copy_from_slice(path.as_bytes());
        name[6 + path.len()..len].copy_from_slice(extension.as_bytes());
        crate::diskfs::read_program(core::str::from_utf8(&name[..len]).unwrap(), data)
    } else {
        result
    };
    let size = result.map_err(|e| e.message())?;
    if size == 0 {
        return Err("Program is empty.");
    }
    if arguments.len() > 128 {
        return Err("Program arguments exceed 128 bytes.");
    }
    let mut program =
        loader::load_binary(&data[..size]).ok_or("Not enough memory for program code/stack.")?;
    drop(staging);
    program.arguments[..arguments.len()].copy_from_slice(arguments);
    program.argument_length = arguments.len();
    let space = paging::AddressSpace::new(&program).ok_or("Not enough memory for page tables.")?;
    let stack = Frames::new(16).ok_or("Not enough memory for a task kernel stack.")?;
    unsafe {
        core::slice::from_raw_parts_mut(stack.base() as *mut u32, 16).fill(0x58475354);
    }
    let pid = crate::task::reserve_id().ok_or("Process IDs exhausted.")?;
    let stem = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .split('.')
        .next()
        .unwrap_or("")
        .as_bytes();
    let mut name = [0; 8];
    let name_length = stem.len().min(8);
    name[..name_length].copy_from_slice(&stem[..name_length]);
    Ok(Job {
        pid,
        program,
        space,
        stack,
        cpu: Cpu::new(),
        fpu: Fpu::fresh(),
        state: State::Ready,
        preemptions: 0,
        name,
        name_length,
        directory: crate::diskfs::directory(),
        tty: None,
        task_slot: None,
        terminal_owner: None,
    })
}
pub fn demo(argument: &str) {
    if !matches!(argument, "" | "hog" | "fault" | "check" | "wait") {
        crate::println!("Usage: multitask [hog|fault|check|wait]");
        return;
    }
    if !paging::isolation_available() || core::arch::x86::__cpuid(1).edx & 1 == 0 {
        crate::println!("Multitasking requires protected paging and an x87 FPU.");
        return;
    }
    let _host_fpu = HostFpu::begin();
    let args = match argument {
        "hog" => b"A hog" as &[u8],
        "fault" => b"A fault",
        "check" => b"A check",
        "wait" => b"A wait",
        _ => b"A",
    };
    let mut jobs: [Option<Job>; LIMIT] = [const { None }; LIMIT];
    let first = match load("/APPS/COUNTER.BIN", args) {
        Ok(job) => job,
        Err(message) => {
            crate::println!("Multitask: {}", message);
            return;
        }
    };
    let second = match load(
        "/APPS/COUNTER.BIN",
        if argument == "check" {
            b"B check"
        } else {
            b"B"
        },
    ) {
        Ok(job) => job,
        Err(message) => {
            crate::println!("Multitask: {}", message);
            return;
        }
    };
    crate::debug::write_fmt(format_args!(
        "Scheduler: jobs {}/{} cr3={:08x}/{:08x} stacks={:08x}/{:08x}\n",
        first.pid,
        second.pid,
        first.space.directory(),
        second.space.directory(),
        first.stack.top(),
        second.stack.top()
    ));
    jobs[0] = Some(first);
    jobs[1] = Some(second);
    run(jobs, false);
}
pub fn desktop() {
    if !paging::isolation_available() || core::arch::x86::__cpuid(1).edx & 1 == 0 {
        crate::println!("Desktop requires protected paging and an x87 FPU.");
        return;
    }
    let _host_fpu = HostFpu::begin();
    run([const { None }; LIMIT], true);
}
pub fn terminal_desktop() {
    if !paging::isolation_available() || core::arch::x86::__cpuid(1).edx & 1 == 0 {
        crate::println!("Terminal requires protected paging and an x87 FPU.");
        return;
    }
    let _host_fpu = HostFpu::begin();
    let mut jobs = [const { None }; LIMIT];
    match load("/APPS/TERMINAL.BIN", b"") {
        Ok(mut job) => {
            job.task_slot = Some(0);
            jobs[0] = Some(job);
        }
        Err(e) => {
            crate::println!("{}", e);
            return;
        }
    }
    run(jobs, true);
}
fn root_slot(jobs: &[Option<Job>; LIMIT]) -> Option<usize> {
    (0..4).find(|&i| !jobs.iter().flatten().any(|j| j.task_slot == Some(i)))
}
fn sync_tasks(jobs: &[Option<Job>; LIMIT]) {
    unsafe {
        RUNNING = jobs.iter().flatten().count();
        ROOTS = jobs
            .iter()
            .flatten()
            .filter(|j| j.task_slot.is_some())
            .count();
    }
    let mut tasks = [None; 4];
    for job in jobs.iter().flatten() {
        if let Some(i) = job.task_slot {
            tasks[i] = Some(crate::desktop_shell::Task {
                pid: job.pid,
                name: job.name,
                length: job.name_length,
            });
        }
    }
    crate::multi_window::set_tasks(tasks);
}
fn run(mut jobs: [Option<Job>; LIMIT], desktop: bool) {
    if let Err(message) = if desktop {
        crate::multi_window::begin_desktop()
    } else {
        crate::multi_window::begin()
    } {
        crate::println!("Multitask: {}", message);
        return;
    }
    unsafe {
        ACTIVE = true;
        DESKTOP = desktop;
        REQUEST = None;
        PAUSE = None;
        SAVED = None;
    }
    sync_tasks(&jobs);
    if desktop {
        crate::debug::write_str("Desktop scheduler: ready\n");
    }
    let (mut last, mut switches, mut preemptions, mut sleeps, mut waits) =
        (LIMIT - 1, 0u32, 0u32, 0u32, 0u32);
    loop {
        // A text task keeps its context while its newly-created terminal opens.
        for i in 0..LIMIT {
            if let Some(owner) = jobs[i].as_ref().and_then(|j| j.terminal_owner) {
                if let Some(id) = crate::pty::for_owner(owner) {
                    let job = jobs[i].as_mut().unwrap();
                    job.tty = Some(id);
                    job.terminal_owner = None;
                    job.state = State::Ready;
                    crate::pty::attach(id, job.pid);
                    crate::debug::write_fmt(format_args!(
                        "Desktop: text attached pid={} terminal={}\n",
                        job.pid, owner
                    ));
                } else if !jobs.iter().flatten().any(|j| j.pid == owner) {
                    queue_kill(jobs[i].as_ref().unwrap().pid);
                }
            }
        }
        crate::pty::service();
        while let Some(pid) = crate::pty::take_kill() {
            queue_kill(pid);
        }
        if crate::multi_window::pump() {
            break;
        }
        check_host_stack();
        if let Some(request) = unsafe { (*core::ptr::addr_of_mut!(REQUEST)).take() } {
            if request.tty.is_some_and(|id| !crate::pty::exists(id)) {
                continue;
            }
            if let Some(index) = jobs.iter().position(Option::is_none) {
                let previous = crate::diskfs::directory();
                crate::diskfs::set_directory(request.directory);
                let name = core::str::from_utf8(&request.name[..request.length]).unwrap();
                let loaded = load(name, &request.args[..request.argc]);
                crate::diskfs::set_directory(previous);
                match loaded {
                    Ok(mut job) => {
                        job.tty = request.tty;
                        if let Some(id) = job.tty {
                            crate::pty::attach(id, job.pid);
                        } else {
                            job.task_slot = root_slot(&jobs);
                        }
                        crate::debug::write_fmt(format_args!(
                            "Desktop: terminal launch pid={} name={}\n",
                            job.pid, name
                        ));
                        jobs[index] = Some(job);
                        crate::multi_window::status(b"App launched from terminal.");
                    }
                    Err(error) => {
                        if let Some(id) = request.tty {
                            crate::pty::child_done(id, 0, error);
                        } else {
                            crate::multi_window::status(error.as_bytes());
                        }
                    }
                }
                sync_tasks(&jobs);
            } else {
                crate::multi_window::status(
                    b"No free app slots. Close an app before launching another.",
                );
            }
        }
        if let Some(entry) = crate::multi_window::take_launch() {
            if let Some((index, slot)) = jobs.iter().position(Option::is_none).zip(root_slot(&jobs))
            {
                let path = core::str::from_utf8(&entry.path[..entry.path_len]).unwrap();
                match load(path, b"") {
                    Ok(mut job) => {
                        job.task_slot = Some(slot);
                        crate::debug::write_fmt(format_args!(
                            "Desktop: launch pid={} path={}\n",
                            job.pid, path
                        ));
                        jobs[index] = Some(job);
                        crate::multi_window::status(
                            b"App started. Click its window or task to switch focus.",
                        );
                    }
                    Err(error) => {
                        crate::debug::write_fmt(format_args!(
                            "Desktop: launch failed {}: {}\n",
                            path, error
                        ));
                        crate::multi_window::status(error.as_bytes());
                    }
                }
                sync_tasks(&jobs);
                crate::multi_window::refresh_catalog();
            } else {
                crate::multi_window::status(
                    b"Four apps are already running. Close one before starting another.",
                );
            }
        }
        if let Some(pid) = crate::multi_window::take_kill() {
            queue_kill(pid);
        }
        while let Some(pid) = take_kill() {
            if let Some(index) = jobs
                .iter()
                .position(|j| j.as_ref().is_some_and(|j| j.pid == pid))
            {
                crate::debug::write_fmt(format_args!("Scheduler: forced exit pid={}\n", pid));
                crate::multi_window::remove(pid);
                crate::pty::close_owner(pid);
                if let Some(id) = jobs[index].as_ref().and_then(|j| j.tty) {
                    crate::pty::child_done(id, pid, "Interrupted.");
                }
                jobs[index] = None;
                sync_tasks(&jobs);
                crate::multi_window::refresh_catalog();
                crate::multi_window::status(b"Focused app stopped. Other apps are still running.");
            }
        }
        if !desktop && jobs.iter().all(Option::is_none) {
            break;
        }
        let now = crate::interrupts::timer_ticks();
        let mut next = None;
        for offset in 1..=LIMIT {
            let index = (last + offset) % LIMIT;
            if let Some(job) = &jobs[index] {
                if job.terminal_owner.is_some() {
                    continue;
                }
                if scheduler_model::runnable(
                    job.state,
                    now,
                    if let Some(id) = job.tty {
                        crate::pty::pending(id)
                    } else {
                        crate::multi_window::pending(job.pid)
                    },
                ) {
                    next = Some(index);
                    break;
                }
            }
        }
        let Some(index) = next else {
            crate::idle::wait_for_pointer();
            continue;
        };
        last = index;
        let free_slot = jobs.iter().position(Option::is_none);
        let job = jobs[index].as_mut().unwrap();
        job.state = State::Ready;
        switches = switches.wrapping_add(1);
        unsafe {
            PAUSE = None;
            SAVED = None;
        }
        let previous_directory = crate::diskfs::directory();
        crate::diskfs::set_directory(job.directory);
        crate::pty::set_current(job.tty);
        job.fpu.restore();
        let outcome = usermode::quantum(&job.program, &job.space, &job.stack, job.pid, &job.cpu);
        job.fpu.save();
        crate::pty::set_current(None);
        job.directory = crate::diskfs::directory();
        crate::diskfs::set_directory(previous_directory);
        check_host_stack();
        assert!(
            unsafe { core::slice::from_raw_parts(job.stack.base() as *const u32, 16) }
                .iter()
                .all(|&word| word == 0x58475354),
            "task kernel stack overflow"
        );
        let (reason, context) = unsafe {
            (
                (*core::ptr::addr_of_mut!(PAUSE)).take(),
                (*core::ptr::addr_of_mut!(SAVED)).take(),
            )
        };
        if let Some(cpu) = context {
            job.cpu = cpu;
        }
        if matches!(reason, Some(Pause::Console(_))) {
            let directory = crate::diskfs::directory();
            crate::diskfs::set_directory(job.directory);
            let terminal = load("/APPS/TERMINAL.BIN", b"");
            crate::diskfs::set_directory(directory);
            match (free_slot, terminal) {
                (Some(slot), Ok(mut terminal)) => {
                    terminal.task_slot = job.task_slot.take();
                    job.terminal_owner = Some(terminal.pid);
                    job.state = State::Waiting;
                    crate::debug::write_fmt(format_args!(
                        "Desktop: text terminal pid={} terminal={}\n",
                        job.pid, terminal.pid
                    ));
                    jobs[slot] = Some(terminal);
                    sync_tasks(&jobs);
                }
                (_, Err(message)) => {
                    crate::multi_window::status(message.as_bytes());
                    queue_kill(job.pid);
                }
                _ => {
                    crate::multi_window::status(b"No room for a terminal. Close another app.");
                    queue_kill(job.pid);
                }
            }
            continue;
        }
        let finished = match outcome {
            Some(usermode::Outcome::Exit(code)) => {
                crate::debug::write_fmt(format_args!(
                    "Scheduler: exit pid={} code={} preemptions={}\n",
                    job.pid, code, job.preemptions
                ));
                true
            }
            Some(usermode::Outcome::Fault { vector, .. }) => {
                crate::debug::write_fmt(format_args!(
                    "Scheduler: fault pid={} vector={}\n",
                    job.pid, vector
                ));
                true
            }
            None => {
                assert!(context.is_some());
                job.state = match reason {
                    Some(Pause::Sleep(deadline)) => {
                        sleeps += 1;
                        State::Sleep(deadline)
                    }
                    Some(Pause::Detach(_)) => State::Ready,
                    Some(Pause::Tty(_)) => State::Waiting,
                    Some(Pause::Window) => {
                        waits += 1;
                        State::Waiting
                    }
                    Some(Pause::Timer) => {
                        preemptions += 1;
                        job.preemptions += 1;
                        State::Ready
                    }
                    _ => State::Ready,
                };
                false
            }
        };
        if matches!(reason, Some(Pause::Detach(_))) {
            let id = job.tty.unwrap();
            let slot = root_slot(&jobs);
            let job = jobs[index].as_mut().unwrap();
            if let Some(slot) = slot {
                job.tty = None;
                job.task_slot = Some(slot);
                crate::pty::child_done(id, job.pid, "Graphical application started.");
            } else {
                job.cpu.eip = job.cpu.eip.wrapping_add(2);
                job.cpu.eax = (-24i32) as u32;
            }
            sync_tasks(&jobs);
        }
        if finished {
            let job = jobs[index].as_ref().unwrap();
            if let Some(id) = job.tty {
                crate::pty::child_done(
                    id,
                    job.pid,
                    if matches!(outcome, Some(usermode::Outcome::Fault { .. })) {
                        "User program faulted."
                    } else {
                        "User program exited."
                    },
                );
            }
            let pid = job.pid;
            crate::pty::close_owner(pid);
            crate::multi_window::remove(pid);
            jobs[index] = None;
            if desktop {
                sync_tasks(&jobs);
                crate::multi_window::refresh_catalog();
                crate::multi_window::status(
                    b"App finished. Apps launches another; Exit returns to the shell when empty.",
                );
            }
        }
    }
    unsafe {
        ACTIVE = false;
        DESKTOP = false;
        REQUEST = None;
        RUNNING = 0;
        PAUSE = None;
        SAVED = None;
    }
    crate::pty::finish();
    crate::multi_window::finish();
    drop(jobs);
    crate::debug::write_fmt(format_args!(
        "Scheduler: summary switches={} preemptions={} sleeps={} waits={}\n",
        switches, preemptions, sleeps, waits
    ));
    if desktop {
        crate::println!("Desktop finished. All task windows and memory released.");
    } else {
        crate::println!("Multitasking demo finished. All task windows and memory released.");
    }
}

unsafe extern "C" {
    static stack_bottom: u32;
}
fn check_host_stack() {
    assert!(
        unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(stack_bottom), 16) }
            .iter()
            .all(|&word| word == 0x58475354),
        "supervisor stack overflow"
    );
}
