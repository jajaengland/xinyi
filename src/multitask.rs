// User preemption at ring 3; disk calls use one explicitly yielding worker.
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
    Io(crate::disk_io::Request),
}
static mut ACTIVE: bool = false;
static mut DESKTOP: bool = false;
static mut PAUSE: Option<Pause> = None;
static mut SAVED: Option<Cpu> = None;
#[derive(Clone,Copy)]
struct Launch {
    volume:u32,
    profile: crate::permissions::Profile,
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
static mut DIRECTORIES:[Option<crate::fs_path::Path>;LIMIT]=[None;LIMIT];
static mut KILLS: [u32; 8] = [0; 8];
#[derive(Clone, Copy)]
struct Process {
    pid: u32,
    name: [u8; 8],
    name_length: usize,
    state: State,
    memory_kib: u32,
    terminal: bool,
    disk: bool,
    profile: crate::permissions::Profile,
}
// Published by the dispatcher before servicing shell commands. Never borrow
// live Job records while a terminal command can request their termination.
static mut PROCESSES: [Option<Process>; LIMIT] = [None; LIMIT];

fn sync_processes(jobs: &[Option<Job>; LIMIT]) {
    let snapshot = core::array::from_fn(|i| jobs[i].as_ref().map(|job| Process {
        pid: job.pid,
        name: job.name,
        name_length: job.name_length,
        state: job.state,
        memory_kib: (job.program.image.top() - job.program.image.base()
            + job.program.stack.top() - job.program.stack.base()
            + job.stack.top() - job.stack.base()) / 1024 + job.space.memory_kib(),
        terminal: job.tty.is_some() || job.terminal_owner.is_some(),
        disk: job.io.is_some(),
        profile: job.program.profile,
    }));
    unsafe { core::ptr::addr_of_mut!(PROCESSES).write(snapshot); }
}

pub fn list_processes() {
    let snapshot = unsafe { core::ptr::addr_of!(PROCESSES).read() };
    crate::println!("PID  NAME      STATE    KiB  TYPE  ACCESS");
    for process in snapshot.iter().flatten() {
        let state = if process.disk { "disk" } else { match process.state {
            State::Ready => "ready",
            State::Sleep(_) => "sleep",
            State::Waiting => "waiting",
        }};
        let name = core::str::from_utf8(&process.name[..process.name_length]).unwrap_or("?");
        crate::println!("{:<4} {:<8} {:<7} {:>5}  {}  {}", process.pid, name, state,
            process.memory_kib, if process.terminal { "text" } else { "app" },process.profile.label());
    }
    crate::println!("Total processes: {}", snapshot.iter().flatten().count());
    crate::println!("KiB includes private pages and kernel stack; excludes GUI buffers.");
}
pub fn app_running(name:&[u8])->bool {
    let snapshot=unsafe{core::ptr::addr_of!(PROCESSES).read()};
    snapshot.iter().flatten().any(|p|p.name[..p.name_length].eq_ignore_ascii_case(name))
}

pub fn request_kill(pid: u32) -> Result<(), &'static str> {
    let snapshot = unsafe { core::ptr::addr_of!(PROCESSES).read() };
    if !snapshot.iter().flatten().any(|process| process.pid == pid) {
        return Err("Process not found");
    }
    let queue = unsafe { &mut *core::ptr::addr_of_mut!(KILLS) };
    if queue.contains(&pid) { return Ok(()); }
    let slot = queue.iter_mut().find(|slot| **slot == 0)
        .ok_or("Termination queue full; try again")?;
    *slot = pid;
    Ok(())
}
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
    if crate::usermode::active_image(0x1b).is_some() && !crate::usermode::profile().allows(71,0) {return Err(-13);}
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
    let parsed=crate::permissions::parse(command).map_err(|_|-22)?;
    let (name,args)=(parsed.name,parsed.arguments);
    if name.is_empty() || name.len() > 105 || args.len() > 128 {
        return Err(-22);
    }
    let mut request = Launch {
        volume: crate::diskfs::volume_epoch(),
        profile: parsed.profile,
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
    if unsafe { DESKTOP } && crate::disk_io::handles(number, first) {
        pause(Pause::Io(crate::disk_io::Request { number, a: first, b: second, c: third,
            volume: crate::diskfs::volume_epoch() }));
        return Some(0);
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
    io: Option<crate::disk_io::Request>,
    kill_after_io: bool,
    console_pending: bool,
    console_volume:u32,
}
fn load(path: &str, arguments: &[u8]) -> Result<Job, &'static str> {
    job_from_program(path,arguments,loader::from_disk(path)?)
}
fn job_from_program(path:&str,arguments:&[u8],mut program:loader::LoadedProgram)->Result<Job,&'static str> {
    if arguments.len()>128 {return Err("Program arguments exceed 128 bytes.");}
    program.profile=program.profile.restrict(crate::permissions::Profile::for_name(path));
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
    let entry=program.layout.entry;
    Ok(Job {
        pid,
        program,
        space,
        stack,
        cpu: {let mut cpu=Cpu::new();cpu.eip=entry;cpu},
        fpu: Fpu::fresh(),
        state: State::Ready,
        preemptions: 0,
        name,
        name_length,
        directory: crate::diskfs::directory(),
        tty: None,
        task_slot: None,
        terminal_owner: None,
        io: None,
        kill_after_io: false,
        console_pending: false,
        console_volume:0,
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
fn sync_tasks(jobs: &mut [Option<Job>; LIMIT]) {
    let mut slots: [Option<usize>; LIMIT] = core::array::from_fn(|i| jobs[i].as_ref().and_then(|job| job.task_slot));
    scheduler_model::compact_roots(&mut slots);
    for (job, slot) in jobs.iter_mut().zip(slots) {
        if let Some(job) = job { job.task_slot = slot; }
    }
    unsafe {DIRECTORIES=core::array::from_fn(|i|jobs[i].as_ref().map(|j|j.directory));}
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
#[derive(Clone,Copy)]
enum LoadSource {Menu,Command,Console(u32)}
struct PendingLoad {request:Launch,source:LoadSource,drag_token:u32}
fn load_failed(pending:&PendingLoad,jobs:&mut [Option<Job>;LIMIT],message:&str) {
    if matches!(pending.source,LoadSource::Menu) {crate::multi_window::desktop_loaded(None,pending.drag_token);}
    crate::system_sounds::play(crate::system_sounds::Event::Error);
    crate::debug::write_fmt(format_args!("App load: failed name={} reason={}\n",core::str::from_utf8(&pending.request.name[..pending.request.length]).unwrap(),message));
    if let LoadSource::Console(pid)=pending.source {
        if let Some(job)=jobs.iter_mut().flatten().find(|j|j.pid==pid) {job.console_pending=false;queue_kill(pid);}
    } else if let Some(id)=pending.request.tty {crate::pty::child_done(id,0,message);}
    crate::multi_window::status(message.as_bytes());
}
fn finish_load(pending:PendingLoad,jobs:&mut [Option<Job>;LIMIT],result:Result<loader::LoadedProgram,&'static str>,cancelled:bool) {
    use crate::load_model::{ready,Ready};
    let parent_open=pending.request.tty.is_none_or(crate::pty::launch_pending);
    let parent_alive=match pending.source {LoadSource::Console(pid)=>jobs.iter().flatten().any(|j|j.pid==pid&&j.console_pending),_=>true};
    let root=matches!(pending.source,LoadSource::Menu)||matches!(pending.source,LoadSource::Command)&&pending.request.tty.is_none();
    let room=jobs.iter().any(Option::is_none)&&(!root||root_slot(jobs).is_some());
    let decision=ready(cancelled,pending.request.volume,crate::diskfs::volume_epoch(),parent_open,parent_alive,room);
    if decision!=Ready::Publish {
        drop(result);
        load_failed(&pending,jobs,match decision {
            Ready::Cancelled=>"App load cancelled.",Ready::StaleVolume=>"App source volume changed; launch again.",
            Ready::ClosedParent|Ready::MissingParent=>"App launch cancelled: parent closed.",_=>"No room for another application.",
        });return;
    }
    let program=match result {Ok(p)=>p,Err(message)=>{load_failed(&pending,jobs,message);return;}};
    let request=&pending.request;let name=core::str::from_utf8(&request.name[..request.length]).unwrap();
    let previous=crate::diskfs::directory();crate::diskfs::set_directory(request.directory);
    let loaded=job_from_program(name,&request.args[..request.argc],program);
    crate::diskfs::set_directory(previous);
    let mut job=match loaded {Ok(j)=>j,Err(message)=>{load_failed(&pending,jobs,message);return;}};
    job.program.profile=job.program.profile.restrict(request.profile);
    if matches!(pending.source,LoadSource::Menu) {crate::multi_window::desktop_loaded(Some(job.pid),pending.drag_token);}
    match pending.source {
        LoadSource::Console(pid)=>{
            let child=jobs.iter_mut().flatten().find(|j|j.pid==pid).unwrap();
            child.console_pending=false;job.task_slot=child.task_slot.take();child.terminal_owner=Some(job.pid);
            crate::debug::write_fmt(format_args!("Desktop: text terminal pid={} terminal={}\n",pid,job.pid));
        }
        LoadSource::Command=>{
            job.tty=request.tty;
            if let Some(id)=job.tty {crate::pty::attach(id,job.pid);} else {job.task_slot=root_slot(jobs);}
            crate::debug::write_fmt(format_args!("Desktop: terminal launch pid={} name={}\n",job.pid,name));
        }
        LoadSource::Menu=>{
            job.task_slot=root_slot(jobs);
            crate::debug::write_fmt(format_args!("Desktop: launch pid={} path={}\n",job.pid,name));
        }
    }
    let index=jobs.iter().position(Option::is_none).unwrap();jobs[index]=Some(job);sync_tasks(jobs);
    crate::multi_window::status(b"");
}
fn next_load(jobs:&[Option<Job>;LIMIT])->Option<PendingLoad> {
    if let Some(child)=jobs.iter().flatten().find(|j|j.console_pending) {
        let mut name=[0;105];name[..18].copy_from_slice(b"/APPS/TERMINAL.BIN");
        return Some(PendingLoad{source:LoadSource::Console(child.pid),drag_token:0,request:Launch{volume:child.console_volume,profile:crate::permissions::Profile::Trusted,name,length:18,args:[0;128],argc:0,directory:child.directory,tty:None}});
    }
    if let Some(request)=unsafe {(*core::ptr::addr_of_mut!(REQUEST)).take()} {return Some(PendingLoad{source:LoadSource::Command,drag_token:0,request});}
    if let Some((entry,volume,args,argc,drag_token))=crate::multi_window::take_launch() {
        let mut name=[0;105];name[..entry.path_len].copy_from_slice(&entry.path[..entry.path_len]);
        return Some(PendingLoad{source:LoadSource::Menu,drag_token,request:Launch{volume,profile:crate::permissions::Profile::Trusted,name,length:entry.path_len,args,argc,directory:crate::diskfs::directory(),tty:None}});
    }None
}
fn run(mut jobs: [Option<Job>; LIMIT], desktop: bool) {
    let mut io_worker = if desktop {
        let Some(worker) = crate::disk_io::Worker::new() else {
            crate::println!("Desktop: not enough memory for the disk worker.");
            return;
        };
        Some(worker)
    } else { None };
    let mut pending_load:Option<PendingLoad>=None;
    let mut prefer_load=true;
    let mut io_last = LIMIT - 1;
    let mut io_idle_slices = 0u8;
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
    sync_tasks(&mut jobs);
    if desktop {
        crate::debug::write_str("Desktop scheduler: ready\n");
    }
    let (mut last, mut switches, mut preemptions, mut sleeps, mut waits) =
        (LIMIT - 1, 0u32, 0u32, 0u32, 0u32);
    loop {
        // A text task keeps its context while its newly-created terminal opens.
        crate::network::poll();
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
        sync_processes(&jobs);
        crate::pty::service();
        while let Some(pid) = crate::pty::take_kill() {
            queue_kill(pid);
        }
        if crate::multi_window::pump() {
            break;
        }
        check_host_stack();
        if let Some(pid) = crate::multi_window::take_kill() {
            queue_kill(pid);
        }
        while let Some(pid) = take_kill() {
            if let Some(index) = jobs
                .iter()
                .position(|j| j.as_ref().is_some_and(|j| j.pid == pid))
            {
                if crate::disk_io::owner() == Some(pid) {
                    jobs[index].as_mut().unwrap().kill_after_io = true;
                    crate::debug::write_fmt(format_args!("Disk I/O: termination deferred pid={}\n",pid));
                    crate::multi_window::status(b"Finishing the active disk operation before stopping this app.");
                    continue;
                }
                crate::debug::write_fmt(format_args!("Scheduler: forced exit pid={}\n", pid));
                crate::multi_window::remove(pid);
                crate::audio::close_owner(pid);
                crate::pty::close_owner(pid);
                if let Some(id) = jobs[index].as_ref().and_then(|j| j.tty) {
                    crate::pty::child_done(id, pid, "Interrupted.");
                }
                jobs[index] = None;
                sync_tasks(&mut jobs);
                crate::multi_window::refresh_catalog();
                crate::multi_window::status(b"");
            }
        }
        if !desktop && jobs.iter().all(Option::is_none) {
            break;
        }
        if let Some(worker) = &mut io_worker {
            if !crate::disk_io::busy() {
                let syscall_waiting=jobs.iter().flatten().any(|j|j.io.is_some());
                if (prefer_load||!syscall_waiting)&&pending_load.is_none() {
                    if let Some(pending)=next_load(&jobs) {
                        let name=core::str::from_utf8(&pending.request.name[..pending.request.length]).unwrap();
                        worker.start_load(name,pending.request.volume);
                        pending_load=Some(pending);prefer_load=false;
                        crate::multi_window::status(b"Loading app... Ctrl+Alt+Esc cancels. Other apps keep running.");
                    }
                }
            }
            if !crate::disk_io::busy() {
                for offset in 1..=LIMIT {
                    let index = (io_last + offset) % LIMIT;
                    if let Some(job) = jobs[index].as_ref().filter(|job| job.io.is_some()) {
                        worker.start(job.pid, job.io.unwrap());
                        io_last = index;
                        prefer_load=true;
                        break;
                    }
                }
            }
            if crate::disk_io::loading() {
                let pending=pending_load.as_ref().unwrap();
                if pending.request.tty.is_some_and(|id|!crate::pty::launch_pending(id))
                    || matches!(pending.source,LoadSource::Console(pid) if !jobs.iter().flatten().any(|j|j.pid==pid)) {crate::disk_io::cancel_load();}
                let directory=crate::diskfs::directory();crate::diskfs::set_directory(pending.request.directory);
                let result=worker.step();crate::diskfs::set_directory(directory);
                if result.is_none() {
                    let mut label=[b' ';96];let prefix=b"Loading app: ";label[..prefix.len()].copy_from_slice(prefix);
                    let name=&pending.request.name[..pending.request.length];let n=name.len().min(32);
                    label[13..13+n].copy_from_slice(&name[..n]);
                    let at=14+n;let mut blocks=crate::disk_io::blocks();
                    for i in (at..at+10).rev() {label[i]=b'0'+(blocks%10) as u8;blocks/=10;}
                    let tail=b" blocks | Ctrl+Alt+Esc cancels";label[at+10..at+10+tail.len()].copy_from_slice(tail);
                    crate::multi_window::status(&label[..at+10+tail.len()]);
                }
                if result.is_some() {
                    let cancelled=crate::disk_io::cancel_requested();
                    let program=crate::disk_io::take_loaded();
                    finish_load(pending_load.take().unwrap(),&mut jobs,program,cancelled);
                }
            } else if let Some(pid) = crate::disk_io::owner() {
                let index = jobs.iter().position(|job| job.as_ref().is_some_and(|job| job.pid == pid)).unwrap();
                let job = jobs[index].as_mut().unwrap();
                let directory = crate::diskfs::directory();
                crate::diskfs::set_directory(job.directory);
                crate::pty::set_current(job.tty);
                let result = usermode::io_context(&job.program, &job.space, pid, || worker.step());
                crate::pty::set_current(None);
                job.directory = crate::diskfs::directory();
                unsafe { DIRECTORIES[index] = Some(job.directory); }
                crate::diskfs::set_directory(directory);
                if let Some(result) = result {
                    job.cpu.eax = result;
                    job.io = None;
                    job.state = State::Ready;
                    crate::multi_window::io_progress(pid, None);
                    if job.kill_after_io { queue_kill(pid); }
                } else {
                    crate::multi_window::io_progress(pid, Some(crate::disk_io::blocks()));
                }
            }
        }
        let now = crate::interrupts::timer_ticks();
        let mut next = None;
        for offset in 1..=LIMIT {
            let index = (last + offset) % LIMIT;
            if let Some(job) = &jobs[index] {
                if job.terminal_owner.is_some() || job.console_pending || job.io.is_some() || job.kill_after_io {
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
            io_idle_slices += 1;
            // Continue useful disk work while apps sleep. Firmware repeatedly
            // resets the PIT, so periodically leave one full IRQ window open
            // to keep sleepers/timers progressing even on the BIOS backend.
            if !crate::disk_io::busy() || io_idle_slices >= 8 {
                io_idle_slices = 0;
                crate::idle::wait_for_pointer();
            }
            continue;
        };
        io_idle_slices = 0;
        last = index;
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
        unsafe {DIRECTORIES[index]=Some(job.directory);}
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
            job.console_pending = true;
            job.console_volume=crate::diskfs::volume_epoch();
            job.state = State::Waiting;
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
                    Some(Pause::Io(request)) => {
                        job.io = Some(request);
                        State::Waiting
                    }
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
            sync_tasks(&mut jobs);
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
            crate::audio::close_owner(pid);
            if matches!(outcome,Some(usermode::Outcome::Fault{..})) {crate::system_sounds::play(crate::system_sounds::Event::Error);}
            crate::pty::close_owner(pid);
            crate::multi_window::remove(pid);
            jobs[index] = None;
            if desktop {
                sync_tasks(&mut jobs);
                crate::multi_window::refresh_catalog();
            }
        }
    }
    unsafe {
        ACTIVE = false;
        DESKTOP = false;
        REQUEST = None;
        RUNNING = 0;
        DIRECTORIES=[None;LIMIT];
        PROCESSES = [None; LIMIT];
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

pub fn directory_in_use(path:&crate::fs_path::Path)->bool {unsafe {(&*core::ptr::addr_of!(DIRECTORIES)).iter().flatten().any(|dir|path.contains(dir))}}

const _:()=assert!(core::mem::size_of::<Job>()<=1024);
