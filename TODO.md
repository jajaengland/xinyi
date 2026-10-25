# Xinyi Development Roadmap

> **Documentation notice:** This document was AI-assisted and may not have been written entirely by a human. Verify technical details and priorities against the source code and project goals.

This document is intended to help AI assistants understand the code.

This roadmap tracks the development of Xinyi, an experimental 32-bit x86 kernel written primarily in Rust.

Items are grouped by their current status. A planned feature should only be marked complete after it has been implemented and tested.

## Completed Foundations

- [x] Establish a bootable 32-bit x86 kernel.
- [x] Set up a Rust `no_std` environment.
- [x] Integrate NASM assembly into the build.
- [x] Implement VGA text output, scrolling, colours, and cursor control.
- [x] Build an interactive command shell.
- [x] Add command history and line editing.
- [x] Add Tab completion for commands.
- [x] Read the CMOS real-time clock.
- [x] Install a kernel-owned flat Global Descriptor Table (GDT) with ring-0 code and data selectors.
- [x] Set up the Interrupt Descriptor Table (IDT).
- [x] Add CPU exception handlers.
- [x] Remap the Programmable Interrupt Controller (PIC).
- [x] Configure the Programmable Interval Timer (PIT).
- [x] Track uptime using timer interrupts.
- [x] Implement interrupt-driven PS/2 keyboard input with a ring buffer.
- [x] Add a breakpoint command for testing exception handling.
- [x] Add a reboot command.

## Near-Term Priorities

### 1. Reliability and Debugging

- [x] Review compiler warnings and resolve those that indicate real problems.
- [x] Review interrupt handling and ensure the assembly and Rust calling conventions remain consistent.
- [x] Audit ring-buffer behavior, including full-buffer handling and scancode edge cases.
  - Confirmed normal simultaneous-Shift streams decode correctly. The stuck state came from a lost release event, reproducible through QEMU PS/2 queue overflow.
  - The IRQ now drains up to 32 pending bytes, counts ring-buffer drops, and resets Shift and extended-prefix state after a drop while preserving Caps Lock.
  - Caps Lock now toggles on the press edge only, so auto-repeat make codes cannot flip it repeatedly. Drop recovery also clears its pressed latch while preserving the toggled state.
  - Verified with repeated make codes in QEMU: two Caps Lock makes produce one toggle, and repeated letter makes still produce repeated characters.
  - Remaining caveat: QEMU's 16-byte PS/2 queue can drop bytes before the kernel sees them; the kernel cannot recover events that never arrive.
- [x] Review shared mutable state, especially VGA output and interrupt interactions.
  - Keyboard and timer IRQs do not write to VGA; exceptions print and then halt. The shared writer therefore has no concurrent writer today.
  - Any future IRQ-time output must be serialized or sent to the debug console instead of VGA.
- [x] Improve diagnostic messages for kernel faults.
  - Exceptions now report vector, error code, EIP, CS, EFLAGS, and CR2 for page faults, and mirror a summary to the QEMU debug console.
  - Kernel panics now report the panic message and source location on VGA and the debug console, so they are distinguishable from silent hangs. The `panic` command exercises this path.
- [x] Test shell commands and keyboard behavior after changes to interrupt handling.

### 2. Shell Improvements

- [x] Review command-line editing and history edge cases.
  - Verified empty-history navigation, recall and rerun, walking past both ends, the 8-entry history limit (oldest entry is discarded), Backspace at the start of a line, wrapped input and output, and the 128-byte input limit.
  - Verified ambiguous and unique Tab completion, unknown prefixes, leading spaces, and repeated internal spaces.
- [x] Improve error messages for unknown commands.
  - Unknown commands are quoted and, when exactly one command matches by prefix, the shell prints a case-insensitive "Did you mean" suggestion.
  - Verified unique (`ver`), overlong (`versionn`), mixed-case (`VERSION`), ambiguous (`u`), and unrelated (`frobnicate`) inputs.
- [x] Consider more flexible command parsing.
  - Commands are split at the first whitespace character rather than only a literal space, and the argument is trimmed, so repeated spacing works consistently.
- [x] Add a command for displaying kernel diagnostic information.
  - The `keyboard` command shows Shift and Caps Lock state plus buffered and dropped scancode counts.
- [x] Keep shell behavior consistent as new kernel features are added.
  - Help output and Tab completion now share a single `COMMANDS` table, so they cannot drift apart. New commands still need a matching dispatch arm in `run_command`.

### 3. Kernel Structure

- [x] Separate kernel subsystems into clear modules as the project grows.
  - Subsystems are split into `main`, `vga`, `keyboard`, `interrupts`/`interrupts.asm`, `rtc`, and `debug`. `main` wires them together and owns the shell.
- [x] Document important interfaces between subsystems.
  - `ARCHITECTURE.md` now includes a module/interface summary covering each module and its public functions.
- [x] Establish consistent error-handling conventions.
  - Ring-buffer overflow is tracked and triggers conservative state resynchronization; unknown input/scancodes are ignored; panics and CPU exceptions print diagnostics (VGA + debug console) and halt. No heap allocations are used in these paths.
- [x] Improve build and debugging instructions.
  - `README.md` now documents the debug-console/headless workflow, QMP VGA reads, `-no-reboot`, the `keyboard`/`breakpoint`/`panic` commands, and NASM PATH setup for MSYS2/Windows.

## Application Foundations

These tasks prepare the kernel for running programs beyond its built-in shell.

- [x] Investigate Global Descriptor Table (GDT) setup and privilege levels.
  - The multiboot loader supplied a five-entry flat GDT at `0x000cb2b8` with ring-0 code (`0x08`) and data (`0x10`) plus two 16-bit firmware leftovers. The kernel now installs its own three-entry GDT at a kernel address and reloads every segment register.
  - Verified with QMP `info registers`: GDTR points into the kernel, CS/DS/SS remain `0x08`/`0x10`, CPL is 0, and paging is off.
  - Remaining work for privilege separation: user code/data descriptors, a TSS with ring-0 stack, and user-mode execution.
- [x] Design a safe approach to kernel memory management.
  - The boot entry now passes the multiboot magic and information pointer to `rust_main`. `src/multiboot.rs` validates the magic value, checks information flags, bounds the map to 64 KiB, rejects truncated entries, and uses unaligned reads.
  - The `memory` command reports `mem_lower`/`mem_upper` and the full map with region types. QEMU's default 128 MiB machine reports 639 KiB lower, 129920 KiB upper, available RAM from 1 MiB to about 127.9 MiB, and reserved firmware/VGA/high-memory ranges.
  - Rules for the future allocator are documented in `ARCHITECTURE.md`: allocate only type-1 regions, reserve the kernel/stack/IDT/GDT/VGA/boot structures, page-align requests, and identity-map required memory before enabling paging.
- [x] Investigate virtual memory and paging.
  - `src/paging.rs` identity-maps the entire 32-bit physical address space with 4 MiB PSE pages: 1024 page-directory entries, present/writable/user/large, with a 4 KiB-aligned directory.
  - `init` checks CPUID PSE support and leaves paging off with a VGA warning if unavailable. It sets `CR4.PSE` before `CR0.PG` and loads `CR3` with the directory address.
  - Verified with QMP and the `paging` command: `CR0=0x80000011`, `CR3=0x0012c000`, `CR4=0x00000010`, identity range `0x00000000-0x100000000`.
  - The mapping is a foundation, not a protection boundary. User-mode isolation still needs 4 KiB page tables, per-process directories, and a frame allocator.
- [x] Design a basic memory allocator when the required foundations are ready.
  - `src/phys.rs` provides a bitmap allocator for 4 KiB physical frames over the full 32-bit address space.
  - The bitmap is initialized by setting every frame reserved, opening regions with `multiboot` type 1, reserving the first MiB, the kernel image (`_kernel_start`..`_kernel_end`), the page directory, the bitmap itself, and multiboot structures.
  - `alloc_frame()` searches from a hint, returns identity-mapped physical addresses, and `free_frame()` returns them to the pool. The `frames` command reports counts and performs an alloc/free self-test.
  - Paging remains identity-mapped; this allocator manages physical memory only and must remain separate from any future heap.
- [x] Research process and thread management.
  - Basic task structure implemented with kernel stacks, states (Running/Ready/Sleeping/Zombie).
  - Can create tasks and view with `ps` command.
- [ ] Plan how applications will interact with kernel services.

## User-Mode Applications

A longer-term goal is to run applications with less privilege than the kernel.

- [x] Establish user-mode execution and privilege separation.
  - Added ring 3 code (`0x1B`) and data (`0x23`) descriptors and a TSS with `SS0`/`ESP0` loaded via `LTR`.
  - `boot.asm` builds the `IRET` frame in a dedicated `enter_user_mode` routine; the `usermode` command drops to ring 3 with it.
  - The ring-3 entry writes a message straight to the VGA text buffer (avoiding privileged cursor port I/O), issues `int 0x80` for `getpid`, then executes a privileged `CLI`.
  - Verified: the syscall prints `Syscall: getpid()` and returns to ring 3; the `CLI` raises a General Protection Fault reported with `CS=0x001B` and error code 0, caught by the kernel handler.
  - The earlier `IRET` failure was caused by the inline-asm frame (the CPU faulted with `CS=0x0008`); moving the frame setup to assembly fixed it. User code must not call kernel printing helpers because VGA cursor updates use privileged port I/O.
  - Still uses identity-mapped pages that are user-accessible; page-level isolation will come with 4 KiB page tables.
- [x] Design a system-call interface.
  - Added `int 0x80` handler with DPL 3 gate; `isr128` saves/restores registers and calls `xinyi_syscall_handler`.
  - Implemented basic syscall dispatch (sys_exit, sys_write, sys_getpid) reading args from registers (EAX/EBX/ECX/EDX).
  - Verified `getpid` from ring 3 in the `usermode` command; the handler runs on the TSS kernel stack and returns to user mode.

- [x] Define an executable format and loading strategy.
  - Added simple binary loader for flat executables at fixed addresses.
  - Created `loader` module with program loading and user stack allocation.
  - `run` command now uses the loader.
- [x] Implement basic process creation and termination.
  - Added task/process structure with PID, state tracking, stack management.
  - Task creation allocates kernel stack from frame allocator, tracks tasks in array.
  - Added `ps` command to view tasks.
  - Kernel-level task management in place; user process loading and full scheduling will follow.
- [x] Investigate scheduling and multitasking.
  - Added cooperative task scheduler with round-robin scheduling.
  - Implemented task context structure and scheduler logic.
  - Timer interrupt can trigger preemptive context switching.
- [x] Provide a way to launch and manage applications from the shell.
  - Added `run` command to launch applications.
  - Added `ps` command for process management.
  - Basic process creation integrated with shell.

## Filesystem and Storage

- [x] Investigate disk access in the emulator.
  - Implemented ATA/IDE PIO driver for primary controller.
  - Added disk read/write operations.
  - Integrated with block device interface.
- [x] Design a block-device interface.
  - Implemented BlockDevice trait.
  - Created RamDisk and AtaDisk implementations.
- [x] Choose a simple filesystem format to support first.
  - Implemented FAT16 support with block device abstraction.
  - Created block device interface and RAM disk implementation.
  - Added FAT16 driver with boot sector parsing and root directory listing.
- [ ] Implement basic file and directory operations.
- [x] Add shell commands for working with files.
  - Added `ls`, `touch`, `cat` commands.
  - In-memory VFS foundation in place.
- [ ] Test persistence across kernel restarts.

## Graphics and User Interface

The long-term goal includes a graphical interface inspired by classic desktop systems.

- [ ] Research graphics modes and framebuffer access.
- [ ] Design a graphics abstraction separate from VGA text output.
- [ ] Implement basic pixel and drawing operations.
- [ ] Add bitmap font rendering.
- [ ] Develop a simple window or panel system.
- [ ] Design a basic desktop and application-launching interface.
- [ ] Preserve a text-mode or diagnostic path for debugging.

## Networking

- [ ] Research virtual network devices supported by QEMU.
- [ ] Design a network-device abstraction.
- [ ] Implement the foundations of Ethernet and packet handling.
- [ ] Investigate ARP and IP support.
- [ ] Consider UDP before attempting more complex protocols.
- [ ] Add networking tests and diagnostics.

## Audio

- [ ] Research audio hardware emulation available in QEMU.
- [ ] Choose an initial audio device or output interface.
- [ ] Implement basic audio output.
- [ ] Investigate sound effects and application audio.

## Testing and Development Workflow

- [ ] Build and boot the kernel after meaningful changes.
- [ ] Test keyboard input, shell commands, uptime, and exception handling.
- [ ] Keep commits focused and descriptive.
- [ ] Update `README.md` when user-visible features change.
- [ ] Update `ARCHITECTURE.md` when implementation details change.
- [ ] Record unfinished work and known limitations here.
- [ ] Preserve working functionality when introducing new features.

## Future Ideas

These are exploratory ideas rather than commitments:

- A more complete command shell.
- Standalone applications.
- A graphical desktop with a retro aesthetic.
- Persistent files and user configuration.
- Networking utilities.
- Audio playback.

## Guidance for Future AI Sessions

When continuing development with an AI assistant:

1. Read `README.md`, `ARCHITECTURE.md`, and this file first.
2. Inspect the relevant source files before proposing replacements.
3. Ask for current code or command output when the implementation is unclear.
4. Prefer small, incremental changes over large rewrites.
5. Explain what a change does and why it is needed.
6. Preserve existing working features unless a change explicitly requires modifying them.
7. Do not mark a task complete until the implementation has been built and tested.
8. Clearly distinguish confirmed behavior from assumptions or future plans.
9. Provide commands appropriate for the MSYS2 UCRT64 environment on Windows when giving terminal instructions.
10. Edit files directly using the available coding tools. Do not assume a GUI text editor is required.
11. After testing, review `git diff` before committing.
12. Keep code changes and documentation changes in separate commits when practical.
13. Read current uncommitted changes before assuming the repository matches the last documented milestone.
14. Do not assume a currently modified subsystem is stable merely because it builds.
15. When investigating hardware behavior, prefer temporary diagnostics and observed data over speculative rewrites.

The roadmap is a guide, not a rigid sequence. Priorities can change as the kernel develops and new technical challenges are discovered.