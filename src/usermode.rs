// Ring-3 ABI and memory-protection probes, executing a copied user image.
use crate::{
    gdt,
    loader::{self, Frames},
    paging, println,
};
unsafe extern "C" {
    fn enter_user_mode(
        entry: u32,
        user_stack: u32,
        mode: u32,
        kernel_stack: u32,
        directory: u32,
        image: u32,
    );
    static user_probe_start: u8;
    static user_probe_end: u8;
}

#[derive(Clone, Copy)]
pub(crate) enum Outcome {
    Exit(u32),
    Fault {
        vector: u32,
        error: u32,
        eip: u32,
        address: u32,
    },
}
struct RunState {
    active: bool,
    outcome: Option<Outcome>,
    pid: u32,
    image_size: u32,
    directory: u32,
    space: *const paging::AddressSpace,
    arguments: [u8; 128],
    argument_length: usize,
}
static mut RUN: RunState = RunState {
    active: false,
    outcome: None,
    pid: 0,
    image_size: 0,
    directory: 0,
    space: core::ptr::null(),
    arguments: [0; 128],
    argument_length: 0,
};
fn kernel_flags() -> u32 {
    let flags: u32;
    unsafe {
        core::arch::asm!("pushfd", "pop {0}", out(reg) flags, options(preserves_flags));
    }
    flags
}

pub fn request_exit(cs: u32, code: u32) -> bool {
    unsafe {
        let state = &mut *core::ptr::addr_of_mut!(RUN);
        if !state.active || cs & 3 != 3 || state.outcome.is_some() {
            return false;
        }
        state.outcome = Some(Outcome::Exit(code));
    }
    true
}
pub fn request_fault(cs: u32, vector: u32, error: u32, eip: u32, address: u32) -> bool {
    // NMI, double fault, machine check and reserved vectors stay fatal.
    if !matches!(vector, 0 | 1 | 3..=7 | 10..=14 | 16 | 17 | 19) {
        return false;
    }
    unsafe {
        let state = &mut *core::ptr::addr_of_mut!(RUN);
        if !state.active || cs & 3 != 3 || state.outcome.is_some() {
            return false;
        }
        state.outcome = Some(Outcome::Fault {
            vector,
            error,
            eip,
            address,
        });
    }
    true
}
#[no_mangle]
pub extern "C" fn xinyi_user_should_return() -> u32 {
    unsafe {
        let state = &*core::ptr::addr_of!(RUN);
        (state.active && state.outcome.is_some()) as u32
    }
}
pub fn getpid() -> u32 {
    unsafe {
        let state = &*core::ptr::addr_of!(RUN);
        if state.active {
            state.pid
        } else {
            0
        }
    }
}
pub fn write(cs: u32, fd: u32, pointer: u32, length: u32) -> u32 {
    let state = unsafe { &*core::ptr::addr_of!(RUN) };
    if !state.active || cs & 3 != 3 || paging::status().page_directory != state.directory {
        return u32::MAX;
    }
    if fd != 1 && fd != 2 {
        return (-9i32) as u32;
    }
    if length > crate::user_memory::MAX_WRITE {
        return (-22i32) as u32;
    }
    if !crate::user_memory::readable(pointer, length, state.image_size) {
        return (-14i32) as u32;
    }
    if length == 0 {
        return 0;
    }
    // The immutable page tables and current CR3 establish that every byte
    // is mapped user memory. User code is suspended during this copy.
    let mut copy = [0; crate::user_memory::MAX_WRITE as usize];
    unsafe {
        core::ptr::copy_nonoverlapping(pointer as *const u8, copy.as_mut_ptr(), length as usize);
    }
    crate::vga::write_bytes(&copy[..length as usize]);
    length
}

pub fn active_image(cs: u32) -> Option<u32> {
    let state = unsafe { &*core::ptr::addr_of!(RUN) };
    if state.active
        && state.outcome.is_none()
        && cs & 3 == 3
        && paging::status().page_directory == state.directory
    {
        Some(state.image_size)
    } else {
        None
    }
}
pub fn arguments() -> ([u8; 128], usize) {
    let state = unsafe { &*core::ptr::addr_of!(RUN) };
    (state.arguments, state.argument_length)
}
pub fn enter(argument: &str) {
    if unsafe { (*core::ptr::addr_of!(RUN)).active } {
        println!("A user program is already active.");
        return;
    }
    if !paging::isolation_available() {
        println!("User isolation unavailable: protected paging is disabled.");
        return;
    }
    let mode = match argument {
        "" => 0,
        "kernel-read" => 1,
        "kernel-write" => 2,
        "kernel-stack" => 3,
        "page-table" => 4,
        "code-write" => 5,
        "stack-guard" => 6,
        "vga" => 7,
        "kernel-exec" => 8,
        "image-alias" => 9,
        "exit" => 10,
        "invalid-op" => 11,
        "null-segments" => 12,
        "exit-df" => 13,
        _ => {
            println!("Usage: usermode [exit|invalid-op|null-segments|exit-df|kernel-read|kernel-write|kernel-stack|page-table|code-write|stack-guard|vga|kernel-exec|image-alias]");
            return;
        }
    };
    let bytes = unsafe {
        let start = core::ptr::addr_of!(user_probe_start);
        let size = core::ptr::addr_of!(user_probe_end) as usize - start as usize;
        core::slice::from_raw_parts(start, size)
    };
    let Some(program) = loader::load_binary(bytes) else {
        println!("Out of memory: user image/stack");
        return;
    };
    let Some(space) = paging::AddressSpace::new(&program) else {
        println!("Out of memory: user page tables");
        return;
    };
    let Some(kernel_stack) = Frames::new(loader::KERNEL_STACK_PAGES) else {
        println!("Out of memory: syscall stack");
        return;
    };
    execute(program, space, kernel_stack, 1, mode);
}

pub fn execute(
    program: loader::LoadedProgram,
    space: paging::AddressSpace,
    kernel_stack: Frames,
    pid: u32,
    mode: u32,
) {
    assert!(!unsafe { (*core::ptr::addr_of!(RUN)).active });
    crate::debug::write_fmt(format_args!(
        "User stacks: {:08x}/{:08x} kernel {:08x}/{:08x}\n",
        program.stack.base(),
        program.stack.top(),
        kernel_stack.base(),
        kernel_stack.top()
    ));
    crate::debug::write_fmt(format_args!(
        "User space: cr3={:08x} image={:08x} entry={:08x} stack={:08x} mode={}\n",
        space.directory(),
        program.image.base(),
        program.layout.entry,
        paging::USER_STACK_TOP - 4,
        mode
    ));
    crate::debug::write_fmt(format_args!(
        "User guard: {:08x}\n",
        paging::USER_STACK_GUARD
    ));
    // Paint unused syscall-stack storage to check capacity after returning.
    unsafe {
        core::slice::from_raw_parts_mut(
            kernel_stack.base() as *mut u32,
            ((kernel_stack.top() - kernel_stack.base()) / 4) as usize,
        )
        .fill(0x58475354);
    }
    let old_stack = gdt::kernel_stack();
    let old_directory = paging::status().page_directory;
    let old_flags = kernel_flags();
    let baseline = crate::phys::stats().free_frames;
    let started = crate::interrupts::timer_ticks();
    gdt::set_kernel_stack(kernel_stack.top());
    println!("  Jumping to ring 3 via IRET...");
    println!(
        "  User CS: {:#06x}, User DS: {:#06x}",
        gdt::USER_CODE_SELECTOR,
        gdt::USER_DATA_SELECTOR
    );

    crate::terminal::begin();
    unsafe {
        (*core::ptr::addr_of_mut!(RUN)).active = true;
        (*core::ptr::addr_of_mut!(RUN)).pid = pid;
        (*core::ptr::addr_of_mut!(RUN)).image_size = program.size;
        (*core::ptr::addr_of_mut!(RUN)).space = &space;
        (*core::ptr::addr_of_mut!(RUN)).directory = space.directory();
        (*core::ptr::addr_of_mut!(RUN)).arguments = program.arguments;
        (*core::ptr::addr_of_mut!(RUN)).argument_length = program.argument_length;
        enter_user_mode(
            program.layout.entry,
            paging::USER_STACK_TOP - 4,
            mode,
            kernel_stack.base(),
            space.directory(),
            program.image.base(),
        );
    }
    let unused = unsafe {
        core::slice::from_raw_parts(
            kernel_stack.base() as *const u32,
            ((kernel_stack.top() - kernel_stack.base()) / 4) as usize,
        )
    }
    .iter()
    .take_while(|&&word| word == 0x58475354)
    .count()
        * 4;
    assert!(unused >= 64, "foreground syscall stack overflow");
    crate::debug::write_fmt(format_args!(
        "User syscall stack: unused={} bytes\n",
        unused
    ));
    let restored_flags = kernel_flags();
    assert!(restored_flags & 0x7600 == old_flags & 0x7600);
    gdt::set_kernel_stack(old_stack);
    assert!(paging::status().page_directory == old_directory);
    let outcome = unsafe {
        let state = &mut *core::ptr::addr_of_mut!(RUN);
        state.active = false;
        state
            .outcome
            .take()
            .expect("user return without an outcome")
    };
    crate::window_api::finish();
    drop(kernel_stack);
    drop(space);
    drop(program);
    crate::debug::write_fmt(format_args!(
        "User recovery: cr3={:08x} tss={:08x} reclaimed={} ticks={} flags={:08x}\n",
        old_directory,
        gdt::kernel_stack(),
        crate::phys::stats().free_frames - baseline,
        crate::interrupts::timer_ticks().wrapping_sub(started),
        restored_flags
    ));
    crate::terminal::finish();
    match outcome {
        Outcome::Exit(code) => println!("User program exited with code {}.", code),
        Outcome::Fault {
            vector,
            error,
            eip,
            address,
        } => println!(
            "User program faulted: vector={} error={:#x} eip={:#x} address={:#x}",
            vector, error, eip, address
        ),
    }
}

unsafe extern "C" {
    fn enter_scheduled_user(cpu: *const crate::cpu_context::Cpu, directory: u32);
}
pub(crate) fn quantum(
    program: &loader::LoadedProgram,
    space: &paging::AddressSpace,
    stack: &Frames,
    pid: u32,
    cpu: &crate::cpu_context::Cpu,
) -> Option<Outcome> {
    let old_stack = gdt::kernel_stack();
    let old_directory = paging::status().page_directory;
    let old_flags = kernel_flags();
    gdt::set_kernel_stack(stack.top());
    unsafe {
        let run = &mut *core::ptr::addr_of_mut!(RUN);
        run.active = true;
        run.outcome = None;
        run.pid = pid;
        run.image_size = program.size;
        run.directory = space.directory();
        run.space = space;
        run.arguments = program.arguments;
        run.argument_length = program.argument_length;
        enter_scheduled_user(cpu, space.directory());
    }
    gdt::set_kernel_stack(old_stack);
    assert!(paging::status().page_directory == old_directory);
    assert!(kernel_flags() & 0x7600 == old_flags & 0x7600);
    unsafe {
        let run = &mut *core::ptr::addr_of_mut!(RUN);
        run.active = false;
        run.outcome.take()
    }
}

pub(crate) fn current_space() -> Option<&'static paging::AddressSpace> {
    let run = unsafe { &*core::ptr::addr_of!(RUN) };
    if !run.active
        || run.outcome.is_some()
        || paging::status().page_directory != run.directory
        || run.space.is_null()
    {
        return None;
    }
    Some(unsafe { &*run.space })
}

// A paused owner cannot run or change its mappings while its filesystem call
// uses these buffers. No live reference to RUN crosses the worker stack switch.
pub(crate) fn io_context<T>(
    program: &loader::LoadedProgram, space: &paging::AddressSpace, pid: u32,
    operation: impl FnOnce() -> T,
) -> T {
    let directory = paging::status().page_directory;
    let flags = kernel_flags();
    unsafe {
        core::arch::asm!("cli", options(nostack));
        assert!(!(*core::ptr::addr_of!(RUN)).active);
        core::ptr::addr_of_mut!(RUN).write(RunState {
            active: true, outcome: None, pid, image_size: program.size,
            directory: space.directory(), space,
            arguments: program.arguments, argument_length: program.argument_length,
        });
        core::arch::asm!("mov cr3, {0:e}", in(reg) space.directory(), options(nostack));
    }
    let result = operation();
    unsafe {
        core::arch::asm!("cli", options(nostack));
        (*core::ptr::addr_of_mut!(RUN)).active = false;
        core::arch::asm!("mov cr3, {0:e}", in(reg) directory, options(nostack));
        if flags & 0x200 != 0 { core::arch::asm!("sti", options(nostack)); }
    }
    result
}
