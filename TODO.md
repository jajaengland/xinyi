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

### Initial Verified Baseline — 25 October 2026 (Before Storage Work)

- `cargo +nightly build -Z build-std=core` succeeds on the installed Windows nightly toolchain with NASM on `PATH`; it reports 43 warnings.
- `python tools/smoke.py` boots the debug kernel in QEMU with 128 MiB RAM and a disposable IDE fixture. It verifies memory-map reporting, paging, frame alloc/free balance, keyboard diagnostics, shell editing/history/completion, basic file creation/reading, task-record creation, disk sector-0 reads, increasing uptime, breakpoint/panic diagnostics, and ring 3 `getpid` followed by a GPF with CS `0x001b`.
- These checks establish a baseline, not completeness of the prototypes below. No persistent filesystem, executable launch, context switching, or bootable installation has been verified. Existing untracked bootloader/disk artifacts were left untouched.

### Storage Milestone — 25 October 2026

- FAT16 root-file creation, listing, reading, replacement, and clearing are connected to `mount`, `unmount`, `touch`, `ls`, `cat`, and `write FILE [TEXT]`. Names are case-insensitive ASCII 8.3 names; files are limited to 1024 bytes. RAM remains a separate fallback backend.
- ATA now validates LBA28 capacity, bounds polling, checks errors, waits for write completion, and supports cache flushes. The RAM block device now actually writes.
- `tools/storage_qemu.py` verified host-to-guest and guest-to-host file contents, persistence across guest reset and fresh QEMU processes, unchanged boot/partition sectors, identical FAT copies, and no leaked clusters after replacement. Both whole-volume and primary MBR FAT16 layouts pass; missing-disk operations return to a usable shell.
- `tools/storage_tests.rs` covers boundaries, full disks/directories, corrupt chains, mirror mismatches, read-only files, invalid geometry, and injected write/flush failures. `tools/mkfat16.py` creates new data images without overwriting existing files.
- The storage debug build succeeds with 23 warnings, down from the initial 43. Existing untracked disk/bootloader artifacts remain untouched.
- Added `format` / `format confirm` for a destructive primary-IDE whole-disk quick format: one FAT16 LBA partition, mounted-disk refusal, immediate one-use confirmation, metadata readback/flushes, and an explicit warning that existing partitions and bootability are removed. Large disks use at most just under 2 GiB with the rest unallocated.
- Thirteen host storage tests pass, including formatter geometry and injected failures. `tools/format_qemu.py` verifies mounted refusal, confirmation/cancellation without writes, formatting blank and existing disks, independent host readback, and persistence after a fresh boot. All destructive tests use private disposable images.

### Bootable Installation Milestone — 25 October 2026

- `install` / `install confirm` now build a BIOS-bootable primary IDE system with MBR/stage-2 loaders, a pristine initialized kernel image, and a separate FAT16 data partition. Compatible partitioned FAT16 data is preserved; incompatible/blank disks get a destructive fresh layout only after the preview is confirmed.
- `tools/install_qemu.py` boots only from the installed IDE disk (no `-kernel`, CD, or floppy), verifies E820 memory, allocator/paging/shell operation and persistence at 32/128 MiB RAM, and reinstalls from the running installed system while preserving data. It independently compares installed bytes against ELF load segments, including after deliberate runtime mutations.
- The loader validates header bounds, available RAM, and a payload checksum. Corrupt headers/payloads and insufficient RAM are tested failure paths. Preview/cancellation, one-use confirmation, missing disks, incompatible layouts, changed targets, and write failures are covered by host/QEMU checks. Seventeen host storage/installer tests pass.
- The current debug build succeeds with 22 warnings. Original untracked disk/bootloader artifacts remain untouched; the verified standalone emulator image is `target/xinyi-installed.img` when generated with `--output`.

Current work to prioritize:

- [x] Replace the shell's busy idle loop with an interrupt-safe queue recheck and `sti; hlt`. QEMU observed 28/30 halted samples, repeated keyboard wakeups, and advancing PIT uptime.
- [x] Separate full IDENTIFY capacity from driver access. Refuse whole-disk formatting and fresh installation when the physical end is unknown or inaccessible; preserve access to compatible existing partitions within range. Twenty-three host tests pass. `tools/reliability_qemu.py` verifies refusal on a private sparse 3 TiB disk without guest-data allocation.
- Automated tests use QEMU and disposable images; the user handles real-hardware testing when useful. On 26 October the user reported that reconfiguring the dc7800 RAM restored operation and Xinyi boots normally again.

- [x] Diagnose HP dc7800 target mismatch: its photos identify the WDC drive at PCI 00:1f.2 secondary master, while the previous default was 00:03.2 primary master.
- [x] Add explicit read-only IDE selection shared by mount/format/install, with failed/mounted selection refusal and installed-boot target restoration.
- [ ] User-run hardware follow-up when needed; current LBA48 verification is in QEMU.
- [x] Add LBA48 PIO reads/writes and extended cache flushes. Valid IDENTIFY capacity enables full access up to the current `u32` sector-count limit (just under 2 TiB). Legacy-only devices retain LBA28. `tools/lba48_qemu.py` covers high-address FAT16, independent readback, low-address alias protection, persistence, preserving/fresh installation and disk-only boot on private sparse 160 GiB images.
- [ ] Widen block/partition geometry for devices beyond the current `u32` sector-count limit; whole-disk formatting remains refused there.

- [x] Replace live-register syscall extraction with an explicit saved register frame, aligned C call, kernel data segments, and preserved user segments. `tools/syscall_qemu.py` verifies arguments, return values, other registers, stack, segments and DF/IF restoration across repeated calls in debug and release builds. Both pass; the build now reports 20 warnings.
- [x] Replace the kernel-overlapping loader destination with owned contiguous image frames and zeroed stacks. Allocate separate ring-3 and syscall stacks instead of using 7 MiB/the active boot stack. Debug/release builds succeed with 17 warnings.
- [x] Reclaim image and stack frames on task-record creation failure and removal (`kill PID`); verify a full 16-slot table without leaks, removal/reuse and zeroed memory outside the kernel in 8 MiB debug/release QEMU guests. `tools/phys_tests.rs` checks contiguous allocation, fragmentation, exhaustion and release balance.
- [x] Add owned private page directories and 4 KiB user mappings: read-only code, writable stack and unmapped guard, with supervisor kernel aliases and CR0.WP. Execute a copied position-independent ABI probe rather than kernel code. Debug/release QEMU verifies kernel read/write/execute refusal, protected stacks/tables/VGA/physical aliases, code-write and guard faults, distinct per-record tables and reclamation. User setup refuses missing protected paging. Builds report 17 warnings.
- [x] Add synchronous user exit/fault recovery to the shell. Restore kernel CR3/ESP, registers, segments, flags and TSS stack before reclaiming the image, stacks and page tables. Debug/release recovery tests repeat exits/faults, null-segment timer IRQs and set-DF exits with balanced frames; subsequent kernel faults stay fatal. `usermode exit` returns code 42. Builds report 17 warnings.
- [x] Connect `run [demo|FILE]` to real protected task execution and cleanup. Built-in assembly and host-compiled Rust programs print text, obtain their process ID, exit or fault back to the shell. `spawn demo` retains diagnostic paused records. Both kernel profiles pass file-program/PID/console tests on private FAT16 images; running a program leaves disk bytes unchanged.
- [x] Replace numeric write with `write(fd, buffer, length)` for console descriptors 1/2, bounded to 1024 bytes with overflow/range/current-address-space checks and a kernel copy before output. Add a minimal Rust SDK, flat-image builder, examples and data-image packager. Host boundary checks and real ring-3 invalid-buffer tests pass.
- [ ] Implement scheduling/preemption and a richer executable layout (ELF, writable data/BSS). Execution is synchronous and legacy non-PAE user stacks remain executable.
- [ ] Audit mutable-static references and remaining compiler warnings; the earlier reliability review predates the task/storage prototypes.
- [x] Implement actual VFS listing, reject duplicate names, and connect file content operations.
- [x] Add ATA polling timeouts/error handling and verify writes/readback and reboot persistence on disposable images.
- [x] Replace the raw `install` kernel copy with a defined bootloader/image layout and verify BIOS disk-only boot with a separate data partition.
- [ ] Add UEFI boot support and validate real removable-media/hardware boot; current installed-boot evidence is from legacy BIOS QEMU.
- [ ] Add interruption recovery/redundant boot updates. Installation publishes bootability last but a failed update may require recovery or reinstallation.
- [ ] Add the existing `panic` command to the shared help/completion table.

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
  - Keyboard/timer IRQs do not write to VGA. User exception handlers report and return through the recovery trampoline; kernel faults remain fatal. The shell is suspended during user execution.
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
  - Ring-buffer overflow triggers state resynchronization; unknown input/scancodes are ignored. Panics/kernel exceptions print diagnostics and halt; active synchronous ring-3 faults recover to the shell. No heap allocations are used in these paths.
- [x] Improve build and debugging instructions.
  - `README.md` now documents the debug-console/headless workflow, QMP VGA reads, `-no-reboot`, the `keyboard`/`breakpoint`/`panic` commands, and NASM PATH setup for MSYS2/Windows.

## Application Foundations

These tasks prepare the kernel for running programs beyond its built-in shell.

- [x] Investigate Global Descriptor Table (GDT) setup and privilege levels.
  - The multiboot loader supplied a five-entry flat GDT at `0x000cb2b8` with ring-0 code (`0x08`) and data (`0x10`) plus two 16-bit firmware leftovers. The kernel now installs its own three-entry GDT at a kernel address and reloads every segment register.
  - Verified with QMP `info registers`: GDTR points into the kernel, CS/DS/SS remain `0x08`/`0x10`, CPL is 0, and paging is off.
  - User code/data descriptors, a TSS with ring-0 stack, protected user pages and a ring 3 demonstration were subsequently added.
- [x] Design a safe approach to kernel memory management.
  - The boot entry now passes the multiboot magic and information pointer to `rust_main`. `src/multiboot.rs` validates the magic value, checks information flags, bounds the map to 64 KiB, rejects truncated entries, and uses unaligned reads.
  - The `memory` command reports `mem_lower`/`mem_upper` and the full map with region types. QEMU's default 128 MiB machine reports 639 KiB lower, 129920 KiB upper, available RAM from 1 MiB to about 127.9 MiB, and reserved firmware/VGA/high-memory ranges.
  - Rules for the future allocator are documented in `ARCHITECTURE.md`: allocate only type-1 regions, reserve the kernel/stack/IDT/GDT/VGA/boot structures, page-align requests, and identity-map required memory before enabling paging.
- [x] Investigate virtual memory and paging.
  - `src/paging.rs` initially used a user-accessible 4 MiB identity map. Kernel mappings are now supervisor-only; each private directory substitutes a 4 KiB user code/stack table at `0x40000000..0x40400000`.
  - `init` checks CPUID PSE support and leaves paging off with a VGA warning if unavailable. It sets `CR4.PSE` before `CR0.PG` and loads `CR3` with the directory address.
  - Initial QMP baseline: `CR0=0x80000011`, `CR3=0x0012c000`, `CR4=0x00000010`. The current implementation additionally sets CR0.WP and loads each owned user directory for the ring-3 probe.
  - `tools/isolation_qemu.py` verifies real user page faults and correct CR2/error codes for both builds with 8 MiB RAM. `tools/task_memory_qemu.py` checks page-table permissions/ownership and reclamation independently.
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
  - The diagnostic ring-3 entry checks repeated calls and reports success through a zero-byte console write, then executes CLI or a selected protection probe. Normal programs use `run` and actual text writes.
  - Verified: the syscall prints `Syscall: getpid()` and returns to ring 3; the `CLI` raises a General Protection Fault reported with `CS=0x001B` and error code 0, caught by the kernel handler.
  - The earlier `IRET` failure was caused by the inline-asm frame (the CPU faulted with `CS=0x0008`); moving the frame setup to assembly fixed it. User code must not call kernel printing helpers because VGA cursor updates use privileged port I/O.
  - Code is user/read-only; stack is user/read-write with an unmapped guard. Kernel memory, tables, VGA and physical aliases are supervisor-only. User faults now return to the shell and reclaim the active run's memory.
- [x] Design a system-call interface.
  - Added `int 0x80` handler with DPL 3 gate; `isr128` saves/restores registers and calls `xinyi_syscall_handler`.
  - Saved-frame dispatch supports validated console writes, actual running PIDs and synchronous exit/fault cleanup. The experimental ABI is documented in `sdk/README.md`.
  - Verified `getpid` from ring 3 in the `usermode` command; the handler runs on the TSS kernel stack and returns to user mode.

- [ ] Define and integrate a safe executable format and loading strategy.
  - Flat images linked at `0x40000000` now execute in protected mappings. The host builder extracts code/read-only data from a compiled ELF and rejects writable globals. ELF parsing in the kernel and a writable-data layout remain unfinished; disk programs are still limited to 1024 bytes.
- [x] Implement basic synchronous process creation and termination.
  - `run` creates and consumes a task record, executing its owned image and reclaiming it after exit/fault. `getpid` returns that process's ID. `spawn`/`kill` manage paused diagnostic records; concurrent processes are not yet scheduled.
  - `ps` reports the current ID and total records, not a full process list.
- [ ] Implement and verify scheduling and multitasking.
  - The context structure and round-robin state-selection prototype exist. `schedule()` does not switch CPU state and is never called by the timer IRQ.
- [x] Launch simple flat applications from the shell.
  - `run demo` prints hello world; `run FILE` reads an image from the active RAM/FAT16 backend and waits for exit/fault. `programs/hello/main.rs` and the SDK guide show the host build workflow. Arguments, input and filesystem syscalls remain future work.

## Filesystem and Storage

FAT16 remains the current filesystem. FAT32 and volumes larger than the current roughly 2 GiB cap are deferred until applications need the extra space; the current text-file workload does not justify that work yet.

- [ ] Deferred: add FAT32 and larger-volume creation/migration when needed, retaining FAT16 support.
- [x] Investigate disk access in the emulator.
  - Implemented ATA/IDE PIO driver for primary controller.
  - Added disk read/write operations.
  - Integrated with block device interface.
- [x] Design a block-device interface.
  - Implemented BlockDevice trait.
  - Created RamDisk and AtaDisk implementations.
- [x] Choose a simple filesystem format to support first.
  - FAT16 root-directory regular-file access is connected to the shell, including mirrored FAT writes and bounded fragmented-chain reads.
- [x] Implement root-file creation, listing, reading, and replacement/clearing.
- [x] Add `delete FILE` for RAM and FAT16 root files, including reusable directory slots, mirrored FAT cluster reclamation, read-only/directory refusal and validation before writes. Twenty-six host storage tests pass, including delete write/flush failures and reclamation. QEMU storage checks cover deletion, reuse and cold-boot persistence in both volume layouts.
- [x] Add nested folders on RAM/FAT16: `mkdir`, `cd`, `pwd`, `ls [PATH]`, empty-only `rmdir`, and paths for file commands/program launch. FAT16 directories use validated/growing cluster chains and standard dot entries; navigation resets on backend switches/errors. Thirty-one host storage tests pass, including multi-sector directory growth, corruption, read-only folders and write failures. QEMU verifies both volume layouts, cold boots, host-created folders, nested program execution, recursive host readback, RAM fallback and OEM-name display. Both builds report 17 warnings.
- [ ] Add append, long filenames, and larger files. Current paths allow eight components, FAT16 directory chains 32 clusters, and RAM 32 shared file/folder slots.
- [ ] Add real FAT timestamps (new entries currently use 1980-01-01).
- [ ] Add metadata rollback/recovery and whole-volume cross-link checks. Current replacement needs space for a new copy; interrupted metadata writes can leak clusters or split FAT mirrors.
- [x] Add shell commands for working with files.
  - `touch`, `write`, `cat`, `delete` and `ls` operate on either RAM or an explicitly mounted FAT16 volume. `mount`/`unmount` switch backends without copying files.
- [x] Test persistence across kernel restarts on disposable QEMU images.

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
