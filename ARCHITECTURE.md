# Xinyi Architecture

> **Documentation notice:** This document was AI-assisted and may not have been written entirely by a human. Verify technical details against the source code.

This document is intended to help AI assistants understand the code.

## Overview

`canvas.rs` provides hardware-independent clipped pixel/rectangle/line drawing, filled or outlined triangles and bitmap text into a checked-stride 32-bit RAM surface. `graphics.rs` selects QEMU standard VGA or validated BIOS VESA. The QEMU fast path uses [QEMU standard VGA's documented PCI BAR and Bochs display registers](https://www.qemu.org/docs/master/specs/standard-vga.html). It requires the 1234:1111 adapter, enabled I/O/memory decoding, a bounded framebuffer address, sufficient video memory and the expected colour-text/font state. Other adapters require a supported BIOS VESA framebuffer; unsupported configurations are refused without speculative device programming.

The diagnostic `graphics` command saves legacy VGA registers/DAC and the first 256 KiB of VRAM, extracts the existing BIOS 8x16 font into an owned frame, and allocates an 800x600x32 RAM back buffer before switching mode. Drawing is presented to supervisor-only framebuffer memory; no user mappings or application interfaces change. The demo draws the static scene once and restores only the old box footprint from that cached background before painting its new position (3840 pixel writes per move instead of 480000). It tracks extended arrow make/break state, treats a new press immediately and repeats held movement on 20 ms PIT intervals, independent of keyboard typematic. It uses the existing interrupt-safe input wait when idle. Text restoration disables the extended mode, restores VRAM/fonts/palette/registers and drops all owned buffers. Panic/fatal exception reporting attempts the same restoration. QEMU tests verify actual pixels, input, repeated font recovery, frame balance, injected fatal NMI diagnostics, unsupported hardware refusal and low-memory failure. BIOS VESA, protected client surfaces, PS/2 mouse input and the scheduled window manager are implemented below; native GPU acceleration remains future work.

`terminal.rs` provides foreground program services through syscalls 50..56: keyboard events, 100 Hz ticks, bounded sleep, console controls and positioned row drawing. Calls require an active CPL3 program and matching CR3; drawing validates and copies a bounded user buffer before any VGA write. Input waits drain the existing IRQ queue, including modifier/release bytes, and use `sti; hlt; cli` only when it is empty. Foreground sleep waits similarly allow PIT/keyboard IRQs. Scheduled jobs instead save a sleep/input-wait state and return to the host dispatcher. IRQ handlers never touch the console. Program completion restores the shell's colour and bottom-row cursor after console use. The experimental ABI and limitations are documented in `sdk/README.md`.

Xinyi is an experimental 32-bit x86 bare-metal kernel written primarily in Rust, with NASM assembly used for bootstrapping and interrupt handling.

The kernel runs without the Rust standard library and currently provides VGA text output, an interactive shell, hardware interrupt handling, timer-based uptime tracking, keyboard input, CPU exception handling, and identity-mapped paging.

## Current execution paths

- `shell.rs` provides one command editor/dispatcher for the physical console and PTY sessions.
- `multitask.rs` owns eight protected job slots, with four taskbar roots and their attached text children. Timer preemption applies to user code; kernel services run to completion.
- `multi_window.rs` owns four client surfaces, focus/capture and desktop chrome. Minimizing hides a surface without terminating its job. Taskbar activation restores it and clamps it into view.
- A desktop app's first console request pauses and rewinds that syscall. A Terminal root takes its taskbar slot; the existing app attaches when its PTY opens, preserving earlier side effects and CPU/FPU/memory state.
- `window_api.rs` and `windowtest` retain the synchronous diagnostic/physical-console launch path. Their restrictions should not be confused with the scheduled desktop.
- Programs are flat filesystem files in `/APPS`, not kernel-bundled applications. Text-file and clipboard limits remain 1024 bytes; executable images are bounded to 64 KiB. ELF, writable data and a user heap remain planned work.

## Boot Process

1. The boot assembly entry point disables interrupts.
2. It saves a pristine copy of the initialized kernel in linker-reserved memory, then establishes a stack for the kernel. This copy supports installation without copying modified runtime globals.
3. It passes the multiboot magic value and information pointer to the Rust entry point, `rust_main`.
4. The kernel validates and preserves the boot information, installs its own flat GDT, and reloads all segment registers.
5. The kernel enables identity-mapped paging with 4 MiB pages.
6. The kernel initializes the physical frame allocator from boot information, clears task and in-memory file records, and initializes interrupt handling and the hardware timer.
7. Interrupts are enabled.
8. The VGA display is initialized and the interactive shell starts.

`speaker.rs` owns the legacy PC-speaker tone service (syscall 56). It validates frequency/duration, programs PIT channel 2 without changing the channel-0 clock, waits with interrupt-safe `sti; hlt; cli`, and turns off the speaker before returning. Terminal exit/fault cleanup also stops sound. Snake calls this once per eaten dot. QEMU needs a configured PC-speaker audio backend; the automated Snake tests capture and check WAV samples.

Triangle fills use winding-independent edge tests with wide arithmetic and a clipped bounding box. Degenerate triangles become line segments. Lines are clipped before Bresenham rasterization so extreme off-screen inputs remain bounded and cannot touch stride padding. Host raster tests cover winding, interior/outline separation, clipping and degenerate cases; QEMU checks actual triangle-colour pixels.

## Build System

`programs/apps.txt` lists the standard application sources. `build.rs` runs `tools/build_apps.py`, compiling separate `.bin` files into Cargo's output directory through the existing offline user-image builder and its separate target cache. No application registry or app image is linked into the kernel. Python 3 is required (`PYTHON` selects its executable). The host release packager compiles/export files and seeds a real FAT16 `/APPS` directory using `tools/seed_apps.py`; it refuses conflicting existing contents.

The shell searches the current directory and then `/APPS` for bare program names, adding `.BIN` when needed. `apps` lists real directory entries. `edit FILE` loads `/APPS/EDIT.BIN` through this same path. Deleting/replacing an app changes what runs; no embedded fallback exists. Arguments of at most 128 bytes are copied into the active process and exposed by syscall 59.

`files_api.rs` implements read/save syscalls 57/58 for mounted drives. Paths and inputs use complete readable-range validation and kernel copies. Read outputs must fit entirely in the writable user stack; read-only images, guards, overflow and kernel addresses are rejected. Read copies only the bytes actually read. Filesystem I/O enables timer/keyboard IRQs and disables them again before returning through the saved syscall frame. Save uses the existing replacement/flush path and refuses operation if the disk has detached. The 1024-byte text-file cap remains; larger app files use the separate 64 KiB loader/installer writer.

Drive discovery caches up to 16 identified IDE targets with model, capacity and FAT16/application-volume status, deduplicating aliased channels. An installed target is mounted automatically without substituting another disk if it fails. An uninstalled boot chooses a unique application volume or sole FAT16 volume; ambiguous cases request a numbered choice. `use NUMBER` verifies the candidate before flushing/detaching the current filesystem. `install NUMBER` stages at most 16 `/APPS` files in owned frames, keeps confirmation one-use, then installs the kernel and copies real files. Matching destination apps are replaced; copying failures explicitly report that kernel installation already occurred. There is no cross-file transaction or power-loss journal.

`tools/build_release.py` builds the kernel/apps and runs `tools/system_qemu.py` before packaging a fresh BIOS image, separate app files, manifest, usage notes, launcher and checksums. The tests exercise deleting apps, actual editor saves and cold-boot persistence, numbered drive failures/cancellation and source-to-destination installation.

The project uses a custom target specification, `i686-unknown-none.json`, for 32-bit x86 bare-metal compilation.

The build uses Rust nightly and the `core` library, built with:

`cargo +nightly build -Z build-std=core`

`build.rs` assembles the NASM source files and links their output into the kernel.

It also builds a 512-byte BIOS MBR and a padded 16 KiB stage-2 loader as flat binaries; `installer.rs` embeds these. The linker fills initialized section padding deterministically and exposes `_kernel_file_end` and `_kernel_file_size`. A separate `.bootcopy` NOBITS region reserves the initialized image's size inside `_kernel_start`..`_kernel_end`; `_start` copies the initialized bytes there before calling Rust. The frame allocator therefore reserves the snapshot automatically. A link-time assertion limits the initialized image to 992 KiB, fitting before the data partition.

## Global Descriptor Table

The multiboot loader enters the kernel in 32-bit protected mode with a flat GDT, but that descriptor table belongs to the boot environment rather than to Xinyi. The kernel now installs its own GDT before installing the IDT or enabling interrupts.

| Selector | Purpose | Access | Layout |
| --- | --- | --- | --- |
| `0x00` | Null descriptor | - | Required unused entry |
| `0x08` | Ring 0 code | `0x9A` | Base 0, 4 GiB limit, 4 KiB granularity, 32-bit |
| `0x10` | Ring 0 data | `0x92` | Base 0, 4 GiB limit, 4 KiB granularity |
| `0x18` | Ring 3 code | `0xFA` | Base 0, 4 GiB limit, 4 KiB granularity, 32-bit |
| `0x20` | Ring 3 data | `0xF2` | Base 0, 4 GiB limit, 4 KiB granularity |
| `0x28` | TSS (available 32-bit) | `0x89` | Base and limit of the kernel TSS, byte granularity |

`src/gdt.rs` defines the descriptors and TSS. `gdt_flush` loads the GDTR, kernel data selectors and kernel CS; `tss_flush` loads the TSS with LTR. TSS SS0/ESP0 initially use the kernel data selector and boot stack. The ring-3 demonstration replaces ESP0 with its separately allocated syscall stack. `enter_user_mode` builds the IRET frame with user CS/SS, a private virtual stack and EFLAGS `0x202` (IF set, IOPL/NT/VM/DF clear). Flat user segments remain in use; page permissions enforce the memory boundary.

## Boot Information and Memory Map

`src/boot.asm` preserves the multiboot values from `EAX` and `EBX` when calling `rust_main(magic, info_addr)`. `src/multiboot.rs` accepts the information only when `magic == 0x2BADB002` and the pointer is nonzero. That prevents the kernel from dereferencing untrusted data when started by a different loader.

The memory summary comes from the `mem_lower` and `mem_upper` fields. The memory map comes from the `mmap_length`/`mmap_addr` fields when the corresponding information flag is set. Entries are read with unaligned loads, each entry's size field is checked, truncated entries are rejected, and the total map length is capped at 64 KiB before iteration. The physical frame allocator consumes available regions during initialization; the shell also exposes the map for diagnostics.

With QEMU's default 128 MiB machine, the `memory` command reports:

| Range | Type |
| --- | --- |
| `0x00000000`-`0x0009fc00` | Available |
| `0x0009fc00`-`0x000a0000` | Reserved |
| `0x000f0000`-`0x00100000` | Reserved |
| `0x00100000`-`0x07fe0000` | Available |
| `0x07fe0000`-`0x08000000` | Reserved |
| `0xfffc0000`-`0x100000000` | Reserved |

`mem_lower` is 639 KiB and `mem_upper` is 129920 KiB, consistent with the map.

## Memory Management Constraints

The physical allocator and future virtual-memory work follow these constraints:

- Allocate only from regions marked available (type 1).
- Never hand out memory below `0x00100000`; real-mode firmware data and the VGA window live there.
- Treat the kernel image, boot stack, IDT/GDT, VGA buffer, multiboot information structure, and loaded modules as reserved, regardless of what the map says.
- Page-align allocations and validate requests against the detected map rather than assuming a fixed amount of RAM.
- Keep the active flat identity mapping while adding a physical allocator. The current mapping covers the full 32-bit space, but the allocator must still restrict itself to available regions.
- Before adding user-mode address spaces, introduce 4 KiB page tables and per-process page directories. Keep required kernel mappings supervisor-only, then map user pages separately.
- Keep frame allocation separate from a future heap and verify allocator counts against the `memory` report before adding virtual-memory features.

## Physical Memory Allocation

`src/phys.rs` tracks 4 KiB frames using a 128 KiB bitmap covering the full 32-bit physical space (1,048,576 bits). On startup:

1. Every frame is marked reserved.
2. All multiboot memory-map regions with type 1 are marked free; if no map is present, the allocator falls back to `mem_lower`/`mem_upper`.
3. The kernel reserves `0x00000000`–`0x00100000`, the kernel image (`_kernel_start`..`_kernel_end`), the page directory, the frame bitmap, and the multiboot information and memory-map structures.
4. `init()` reports the number of available frames.

`alloc_frames(count)` finds and claims a contiguous run, starting at the hint and falling back to a full scan. Failed requests leave the bitmap and counts unchanged. `alloc_frame()` requests one frame. `free_frame()` validates alignment and frame state, clears it, and adjusts the hint downward. The allocator itself cannot distinguish reserved frames from allocated frames, so callers must release only their own allocations. `loader::Frames` provides ownership and automatic release for program images and stacks, zeroes newly allocated storage, and rejects allocations whose stack-top address cannot fit in `u32`. Allocation remains serialized in shell context; IRQ handlers do not allocate. There is no heap allocator.

The `frames` command reports available/allocated/free counts and runs a quick alloc/free self-test.

## Paging

`src/paging.rs` enables paging during boot with 4 MiB pages using Page Size Extension (PSE). Because the entire 32-bit physical address space fits in a single page directory:

- 1024 page-directory entries cover 0 through 4 GiB.
- Every kernel entry has present, writable, and 4 MiB page (`PS`) set, with the user bit clear.
- The page directory is 4 KiB aligned and lives in `.bss`.
- `CR4.PSE` is set before `CR0.PG`; `CR0.WP` is also enabled and `CR3` receives the page-directory physical address.
- `CPUID` leaf 1 checks for PSE first. If PSE is unavailable, paging is left disabled and the kernel prints a warning after VGA initialization instead of faulting.

The kernel keeps supervisor identity mappings (`virtual == physical`) for kernel code/data, stacks, descriptor tables, VGA and RAM. Each `paging::AddressSpace` owns a zeroed 4 KiB directory and page table, copies those supervisor mappings, and replaces PDE 256 with a private user table. Only the image pages at `0x40000000` (up to 64 KiB, user/read-only) and the 16 KiB stack at `0x403fc000..0x40400000` (user/read-write) are present. The remaining window, including the guard page at `0x403fb000`, is unmapped. The allocator reserves the physical `0x40000000..0x40400000` range so replacing that identity window cannot obscure an allocated kernel alias. User access must pass both directory and table permissions, following the [Intel paging protection rules](https://cdrdv2-public.intel.com/825758/253668-sdm-vol-3a.pdf), chapter 4.

Kernel physical aliases of user frames, page tables, syscall stacks and VGA remain supervisor-only. The user-entry assembly installs CR3 and keeps the tables/backing allocations alive until its recovery trampoline restores the previous CR3. Dropping active tables is refused, and task-record removal rejects an active directory. User setup refuses unavailable protected paging. These legacy non-PAE mappings have no NX bit, so the user stack is executable. Scheduling is not implemented; execution of the protected probe is synchronous, with exit/fault recovery to the shell.

The `paging` command reports whether paging is enabled, the page size and mapped range, and the current `CR3`, `CR0`, and `CR4` values.

## VGA Text Output

`src/vga.rs` implements text-mode output using the VGA text buffer at physical address `0xB8000`.

The writer supports:

- Character and formatted text output
- Text colours
- Scrolling
- Backspace handling
- Hardware cursor updates

The current implementation uses a single global mutable writer. Output is produced from shell/kernel context, syscalls and CPU exceptions. User faults occur while the shell is suspended and recover only after the Rust handler has returned; kernel faults remain fatal. Keyboard/timer IRQs do not write to VGA. Future concurrent output still requires serialization or a separate debug-console path.

## Interrupt Handling

Interrupt-related code is split between assembly and Rust.

- `src/interrupts.asm` contains exception and hardware IRQ entry stubs.
- `src/interrupts.rs` defines and installs the Interrupt Descriptor Table (IDT), configures the Programmable Interrupt Controller (PIC), and handles timer and keyboard interrupts.

The assembly stubs preserve the required processor state and transfer control to the Rust interrupt handler.

Rust IDT initialization converts the assembly ISR and IRQ symbols to explicit function pointers before converting their addresses to the 32-bit values stored in IDT entries.

The PIC is remapped so hardware IRQs use vectors 32 through 47. IRQ0 is used for the timer, and IRQ1 is used for the keyboard.

## Timer and Uptime

The Programmable Interval Timer (PIT) is configured to generate approximately 100 interrupts per second.

The timer interrupt increments an atomic tick counter. The `uptime` shell command converts the elapsed ticks into hours, minutes, and seconds.

At an empty input queue, the shell calls `idle::wait_for_input`: disable interrupts, recheck the scancode queue, then execute adjacent `sti; hlt` if it remains empty. A queued event instead enables interrupts and returns immediately. Compiler memory barriers keep the queue check inside this sequence, and STI's interrupt shadow prevents a lost wakeup before HLT. Timer and keyboard IRQs wake the halted CPU. ATA/RTC command polling remains bounded polling; this change applies to shell idle time.

## Keyboard Input

`src/keyboard.rs` handles PS/2 keyboard input.

The keyboard IRQ handler reads scancodes from the keyboard data port and places them into a ring buffer. The shell consumes scancodes from that buffer and translates them into key events.

The keyboard implementation supports features including Shift, Caps Lock, Backspace, Enter, Tab, and arrow-key navigation.

The ring buffer separates interrupt-time input collection from normal shell processing.

Keyboard decoding and state tracking happen outside the IRQ handler. The IRQ handler is intentionally kept small and only collects incoming scancode bytes.

Each IRQ drains up to `MAX_BYTES_PER_IRQ` (32) pending bytes so a burst is not left waiting for the next interrupt. If the 128-byte ring buffer is full, the byte is dropped and a drop counter is incremented. After any drop, the decoder resynchronizes by clearing the left/right Shift state, the extended-prefix state, and the Caps Lock pressed latch; the Caps Lock toggled state is preserved because it is a mode rather than a held modifier. This prevents a lost Shift release from leaving letters uppercase indefinitely.

Caps Lock toggles only on the press edge. Hardware auto-repeat sends repeated make codes while a key is held, and toggling on every make code would flip Caps Lock on and off rapidly. Ordinary character keys still process every make code, so auto-repeat continues to work normally.

The `keyboard` shell command exposes the current Shift and Caps Lock state plus buffered and dropped scancode counts, making this behavior observable without extra debug output in the IRQ path.

QEMU's emulated PS/2 controller queue is only 16 bytes and may silently discard bytes before the kernel can read them during large synthetic input bursts. That is an emulator limitation; the kernel drop counter only covers its own ring buffer.

## Real-Time Clock

`src/rtc.rs` reads the CMOS real-time clock and handles BCD or binary values and 12-hour or 24-hour modes.

It waits for updates to finish and samples the clock more than once to reduce the risk of reading inconsistent values. The `date` command displays the date and time.

## Shell

`src/main.rs` contains the kernel entry point and interactive shell.

The shell supports commands including `help`, `about`, `clear`, `echo`, `version`, `uname`, `date`, `uptime`, `keyboard`, `breakpoint`, `panic`, and `reboot`.

A single `COMMANDS` table in `src/main.rs` supplies both the `help` listing and the Tab-completion candidate list, so the two stay consistent. Adding a command still requires a matching arm in `run_command`.

Input is trimmed, then split at the first whitespace character into a command and an argument; the argument is trimmed again. Unknown commands are quoted and, if exactly one command matches by prefix in either direction, the shell prints a case-insensitive "Did you mean" suggestion. Ambiguous prefixes produce no suggestion.

It also provides command history, line editing, cursor movement, and Tab completion. History keeps the eight most recent non-empty commands and suppresses consecutive duplicates.

## CPU Exceptions

The kernel installs handlers for CPU exception vectors 0 through 31.

The assembly exception stubs normalize the CPU's interrupt frame by pushing a dummy error code when the CPU does not supply one, followed by the vector. EIP, CS, and EFLAGS are already in that frame. The stub then passes vector, error code, EIP, CS, and EFLAGS to the Rust handler. The handler prints the exception name, vector, error code, EIP, CS, and EFLAGS, and for page faults (vector 14) also reads and prints CR2. It then mirrors the essential fields to the QEMU debug console at I/O port `0xE9` via `src/debug.rs`, so a fault can be diagnosed from a headless or automated run even when the VGA screen is not visible.

The `breakpoint` command triggers the breakpoint exception, allowing the exception-handling path to be tested. Exception handlers display diagnostic information and halt execution when appropriate.

The panic handler prints the panic message and the source file, line, and column to VGA, mirrors the same information to the debug console, and then halts. The `panic` command deliberately triggers a panic so this path can be exercised like the breakpoint path. A panic is therefore distinguishable from a silent hang in both interactive and automated runs.

## Module and Interface Summary

| Module | Purpose | Key public interfaces |
| --- | --- | --- |
| `main.rs` | Kernel entry point, panic handler, and interactive shell. Uses a single `COMMANDS` table for `help` and Tab completion. | `rust_main`, `print!/println!`, shell dispatch (`run_command`), panic diagnostics with debug-console mirroring. |
| `vga.rs` | VGA text-mode output and cursor management. Uses a single global mutable writer. | `Writer`, `clear_screen`, `move_cursor`, `cursor_position`, `scroll_count`, `_print`. Global wrappers exist for convenience in the current codebase. IRQ-time VGA output is not used today; if added, it must be serialized or moved to `debug`. |
| `gdt.rs` | Kernel-owned flat GDT with ring-0 and ring-3 code/data descriptors plus a TSS. `gdt_flush` and `tss_flush` in `boot.asm` load the table and TSS. | `init`, `set_kernel_stack`, `KERNEL_CODE_SELECTOR`, `KERNEL_DATA_SELECTOR`, `USER_CODE_SELECTOR`, `USER_DATA_SELECTOR`, `TSS_SELECTOR`. |
| `usermode.rs` | Protected synchronous probe runner with exit/fault outcomes and recovery to the shell. | `enter`, `request_exit`, `request_fault`. |
| `syscall_frame.rs` | C-layout saved syscall registers, with compile-time size/offset assertions matching the assembly entry stub. | `SyscallFrame`. |
| `multiboot.rs` | Validated access to the multiboot memory summary and memory map. Reads physical addresses through the current identity map. | `init`, `memory_summary`, `for_each_region`, `Region`, `region_kind`, `info_address`, `mmap_range`. Rejects a bad magic value, missing flags, oversized maps, and truncated entries. |
| `paging.rs` | Supervisor 4 MiB kernel map and owned private 4 KiB user mappings. | `init`, `status`, `AddressSpace`, `isolation_available`, user layout constants. |
| `phys.rs` | 4 KiB physical frame bitmap allocator seeded from the multiboot map with reserved regions. | `init`, `alloc_frame`, `free_frame`, `stats`, `Stats`. Tracks free frames only by bitmap state; ownership is not enforced. |
| `keyboard.rs` | PS/2 scancode collection and translation. IRQ producer, shell consumer via a ring buffer. | `handle_irq`, `poll_key`, `poll_char`, `Key`, `left_shift_down`, `right_shift_down`, `caps_lock_on`, `queued_scancodes`, `dropped_scancodes`. Caps Lock toggles only on the press edge. Drops trigger modifier resynchronization while preserving Caps Lock's toggled state. |
| `interrupts.rs` | IDT setup, PIC remapping, PIT timer, exception and IRQ dispatch, syscall gate. | `init`, `remap_pic`, `init_pit`, `timer_ticks`, `uptime_seconds`, `xinyi_exception_handler`, `xinyi_irq_handler`, `xinyi_syscall_handler`. Exception frames pass vector, error code, EIP, CS, and EFLAGS to the handler. |
| `interrupts.asm` | NASM entry stubs for ISRs and IRQs, and calling convention normalization. Syscall stub for `int 0x80`. | `isr0..isr31`, `irq0..irq15`, `isr128`. |
| `rtc.rs` | CMOS real-time clock reading with BCD/binary and 12/24-hour handling. | `read`, `DateTime`. |
| `task.rs` | Task records own program and stack allocations; round-robin state selection has no CPU context switch. | `init`, `create_task`, `describe`, `remove`, `current_task_id`, `task_count`, `schedule`. |
| `loader.rs` | Zeroed owned contiguous frames, raw-binary copying and stack allocation in the identity map. | `Frames`, `LoadedProgram`, `load_binary`. |
| `vfs.rs` | Volatile, fixed-size file slots with case-insensitive short names. | `init`, `create`, `read`, `write`, `list` (visitor with names/sizes). |
| `blockdev.rs` / `ata.rs` | Block-device trait, RAM disk, and primary IDE PIO implementation. | `BlockDevice`, `RamDisk`, `AtaDisk`. |
| `pci.rs` / `ata_poll.rs` | Read-only PCI storage discovery, IDE channel BAR decoding, and elapsed-tick ATA deadlines. | `for_each_storage`, `primary_channel`, `ide_channel`, `IdeChannel`, `Budget`. |
| `fat16.rs` | FAT16 files and nested directories, mirrored FATs, bounded chains and disk formatting. | `create`, `read`, `write`, `delete`, `list_entries`, `mkdir`, `rmdir`, `is_directory`, formatting. |
| `fs_path.rs` | Bounded ASCII 8.3 path normalization and rendering shared by storage backends. | `Path`, `MAX_DEPTH`, `MAX_PATH`. |
| `diskfs.rs` | Explicit filesystem mount/RAM dispatch, formatter confirmation, and disk diagnostics. | `mount`, `unmount`, `is_mounted`, `prepare_format`, `confirm_format`, `cancel_format`, file operations, `test`. |
| `installer.rs` / `bootdisk.rs` | Installation confirmation, pristine kernel snapshot access, disk layout planning, verified bootloader/payload writes. | `prepare`, `confirm`, `cancel`, `Kernel`, `InstallPlan`, `plan`, `install`. |
| `disk_mbr.asm` / `disk_stage2.asm` | BIOS/EDD disk-only boot, E820 memory discovery, image verification, and 32-bit kernel handoff. | MBR at LBA 0, stage 2 at LBAs 1..32, header at LBA 63, kernel at LBA 64. |
| `debug.rs` | Minimal I/O to QEMU debug console port `0xE9` for headless diagnostics. | `write_byte`, `write_str`, `write_fmt`, `write_hex8`, `write_u32`, `write_hex32`. |

Recovery paths fall back to explicit safe states; ring-buffer drops resynchronize modifiers, filesystem I/O errors detach the disk backend, and synchronous user faults terminate the active probe. Kernel faults and panics print diagnostics and halt.

## Tasks, Loader, and System Calls

`src/task.rs` holds up to 16 records, each owning an image, user/kernel stacks and a private directory/table. `run [demo|FILE]` creates a record and synchronously consumes it into `usermode::execute`; the runner restores the shell and releases all owners after exit/fault. A record is removed from the table while executing, and its ID is stored in the active run for getpid. `spawn demo` creates a paused diagnostic record; `kill PID` releases it. PIDs advance only after successful creation. `ps` shows the current paused record and count. `schedule()` is still only a state-selection prototype, not a CPU context switch.

`load_binary` copies a nonempty slice up to 64 KiB into owned zeroed image/stack frames. Entry is the first byte at `0x40000000`; the entire image is user/read-only. The built-in hello image comes from `programs/demo.asm`, assembled by Cargo. FAT16 program reads validate up to 128 clusters and 64 KiB; RAM programs retain the 1024-byte RAM-file limit. The shell stages disk programs in 16 owned frames, loads the actual image, then drops staging before task creation, avoiding a large kernel-stack allocation. Empty/missing/oversized files fail without creating a task. A one-page program owns eleven frames including both stacks and tables, reclaimed after execution or paused-record removal.

The separate `usermode` demonstration copies a position-independent blob into owned user pages and allocates private tables plus a 64 KiB syscall stack. It reports ABI success through a zero-byte fd-1 write. Plain `usermode` then executes CLI; protection modes fault, and `exit` calls syscall 1 with code 42. Invalid-op, null-segment and set-DF exit probes exercise recovery. Allocation failure drops partial owners before entry. `task_memory_qemu.py` checks ownership/permissions; `isolation_qemu.py` checks page faults/recovery; `phys_tests.rs` exercises contiguous allocation.

Before entry, Rust records the old TSS stack and flags and arms a single active-run state. `enter_user_mode` saves C callee-saved registers, data segments and flags on the original shell stack, saves that ESP and CR3 in supervisor BSS, then installs the user CR3 and IRET frame. A valid ring-3 exit records its code. Recoverable synchronous exceptions record vector/error/EIP/CR2 only when saved CS is ring 3 and a run is active. NMI, double fault, machine check, reserved vectors and all kernel-mode faults remain fatal. Reentrant user runs are refused.

Rust handlers finish and return normally before the assembly stubs branch to `user_mode_return`; no Rust frames are bypassed while they hold references or resources. The trampoline disables interrupts, loads trusted kernel data segments, restores the saved CR3/ESP, segments, flags and callee-saved registers, then returns to the original Rust caller. Rust restores TSS ESP0, consumes/clears the active outcome, and only then drops syscall stack, tables and user backing memory. It reports the result on the shell. All exception/IRQ C calls now load kernel segments and align the call stack; IRQ entries additionally preserve/restore the user's segment selectors. This permits timer interrupts and exit even when user DS/ES/FS/GS are null. `user_recovery_qemu.py` checks repeated exits/faults in each build, independent CR3/TSS readback, flags, frame balance, continued shell operation and a subsequent fatal kernel breakpoint.

The DPL-3 `int 0x80` gate switches to the TSS kernel stack. The syscall number is EAX; three arguments are EBX, ECX, EDX; the result is EAX and other general registers are preserved. `isr128` saves registers with PUSHA and passes a pointer to `syscall_frame::SyscallFrame` as an explicit C argument. This fixed 52-byte prefix ends at saved EFLAGS; optional user ESP/SS are outside the Rust structure so ring-0 entries do not imply nonexistent fields. Compile-time assertions check layout offsets. The stub saves DS/ES/FS/GS, loads kernel data selectors, clears DF, aligns the C call stack to 16 bytes, writes the result into saved EAX, restores segments/registers and returns via IRETD, restoring the caller's flags.

`usermode_syscall_probe` repeats getpid, an invalid-descriptor write and unknown calls with distinct arguments, checking returns, preserved registers/segments/stack and DF/IF. A zero-byte fd-1 write marks success. Syscall 1 exits the active run; syscall 20 returns its process ID (diagnostic probes use ID 1). Syscall 4 is `write(fd, pointer, length)`: console fds 1/2, at most 1024 bytes, byte-count return or negative error (`-9` descriptor, `-14` range, `-22` length, `-1` unavailable/unknown). Kernel-origin writes and inactive/mismatched CR3 contexts are refused.

`user_buffer::readable` checks overflow and complete containment within initialized image bytes or the mapped user stack; guards, page padding, kernel/physical aliases and other unmapped regions are rejected. Page tables are immutable for the active run, user execution is suspended during syscalls, and IRQs do not switch mappings. The handler therefore copies a validated buffer to a bounded kernel array before console output. Zero length avoids pointer access. `vga::write_bytes` sends the bytes through the existing text writer. `programs_qemu.py` checks both profiles with real Rust programs, including stack/constant buffers, stderr, image/stack boundary crossings, overflow, bad descriptors and kernel pointers, with no kernel exceptions or disk writes.

`sdk/xinyi.rs` supplies freestanding Rust entry, syscall wrappers and panic-exit. `tools/build_program.py` uses offline Cargo, the existing target JSON and `sdk/user.ld` to compile an ELF and extract its single read-only flat image. Mutable globals/BSS are rejected; stack locals and immutable strings work. The kernel does not parse ELF yet. `tools/program_disk.py` creates a fresh FAT16 data image with named small binaries. See `sdk/README.md` for the workflow and ABI limits.

## Filesystems and Disk Installation

`src/vfs.rs` provides 32 shared file/folder slots. Each stores a parent index and directory flag; file data remains limited to 1024 bytes. Names are ASCII 8.3 and unique case-insensitively within a parent. `fs_path::Path` lexically normalizes slash/backslash separators, dot and parent components into at most eight short-name components, clamping parent traversal at root. `diskfs` resolves shell paths against a current directory and dispatches file/folder operations to RAM or the mounted FAT16 volume. Successful fresh mount, unmount and disk-error detachment reset CWD to root; failed `cd` otherwise preserves CWD. Removal of root/current/ancestor directories is refused. Backends remain separate; mounting never copies files. All state is shell-only and must be serialized before concurrent filesystem callers are added.

`src/blockdev.rs` defines sector I/O, capacity, and cache-flush operations. Its RAM disk uses interior mutability and performs real bounded writes. `src/ata.rs` supports IDE LBA28/LBA48 PIO, validates IDENTIFY capacity and logical sector size, disables ATA IRQs while polling, and checks absent-device, ERR, DF, and ATAPI signatures. Writes wait for completion and the filesystem issues cache flushes at commit boundaries. `disk` identifies the target and reads sector 0 without modifying media; failures retain status/error/signature diagnostics and successful detection reports the model and addressing mode.

`BlockDevice::block_count` is the accessible range; `physical_block_count` separately describes the full device, or returns `None` when unknown. `ata_capacity::decode` validates IDENTIFY words 60/61 and, when word 83 indicates valid LBA48 support, words 100..103. Saturated legacy capacity alone cannot establish the physical end. Valid extended capacity enables READ SECTORS EXT (`0x24`), WRITE SECTORS EXT (`0x34`) and FLUSH CACHE EXT (`0xea`); otherwise the existing LBA28 commands remain in use. Extended task-file bytes are written high first, low second, with a one-sector transfer count and zero address bits 32..47. This follows the [ATA 48-bit task-file protocol](https://courses.cs.washington.edu/courses/cse451/16au/readings/ata.pdf), section 6.20.

The public block/MBR geometry still uses a `u32` sector count, so accessible capacity is capped at `u32::MAX` sectors while full capacity remains `u64`. `fat16::require_full_disk` rejects unknown or truncated capacity before format preview and before any formatter writes; fresh installer planning uses the same guard. Installation preserving compatible partitions wholly within the accessible range remains possible. Consequently the formatter's backup-GPT cleanup uses the actual device end. The FAT16 volume cap remains just under 2 GiB even on larger devices.

`src/pci.rs` enumerates storage-class PCI functions with configuration mechanism #1. It reads the first IDE-class controller's programming-interface flags and I/O-enable bit. Compatibility-mode channels use `1f0/3f6` and `170/376`; native channels use BAR0/BAR2 for command registers and BAR1/BAR3 plus 2 for control. Invalid, disabled, or memory BARs are not used. With no PCI storage controller the old legacy-primary fallback is retained. AHCI/RAID are reported, not driven or switched. No PCI configuration writes are made. The register model is documented in the [Intel ICH9 datasheet](https://www.intel.ru/content/dam/doc/datasheet/io-controller-hub-9-datasheet.pdf) and HP's [SATA implementation guide](https://h20331.www2.hp.com/Hpsub/downloads/implementing_SATA.pdf).

`AtaDisk::new(0)` still means the primary master of the first IDE controller, now at the discovered addresses. `disk scan` refuses a mounted filesystem, displays PCI IDs/interface/BAR/mapping values, and issues IDENTIFY to master/slave on both usable channels of each IDE controller. It does not redirect the mount/format/install target or write disk sectors. This keeps diagnostic discovery from silently selecting another physical drive for destructive commands.

`ata_poll::Budget` enforces elapsed PIT deadlines (500 ticks for IDENTIFY, 200 for ordinary waits, 1000 for cache flush) and separately bounds ten million iterations with no tick advance. Unlike the former one-million-total-iterations cap, regular tick progress resets the stalled counter. Host tests cover native BAR decoding, bad/disabled BARs, wraparound, and prolonged polling. QEMU exercises compatibility-mode PCI IDE scans at three drive positions and unsupported AHCI reporting; HP dc7800 and actual native-mode operation still require hardware evidence.

`src/fat16.rs` validates BPB geometry, cluster counts, device bounds and reserved FAT entries. It accepts whole-volume images or primary MBR FAT16 types `04`, `06`, `0e`, 512-byte sectors, one/two FATs and clusters up to 32 KiB. Files remain bounded to 1024 bytes/two clusters. Directory views cache up to 32 validated cluster IDs; traversal rejects cycles, reserved/free/bad/out-of-range links, mirror disagreement and malformed directory/dot entries. The root uses its fixed BPB entry area. Subdirectory entry lookup maps a slot through its cached chain; entries retain their physical sector so updates target the correct directory. `list_entries` returns a directory flag and omits dot/LFN/volume-label entries. Unsupported OEM names display a placeholder rather than panicking.

`mkdir` reserves a parent slot, zeroes the new cluster, initializes `.`/`..` (parent zero for root), and flushes data before publishing mirrored FAT allocation and then the parent entry. Timestamps use the existing valid 1980 baseline and directory size is zero. When a non-root directory fills, a zeroed extension cluster is allocated/flushed before linking the old tail; root directories cannot grow. `rmdir` validates the chain, refuses nonempty/read-only directories, flushes the deleted parent entry before releasing all directory clusters, then flushes again. Parent mutation respects its read-only flag. These operations are not journaled: interrupted writes can leave leaked clusters, an extra empty directory cluster or split mirrors; I/O/corruption errors detach the disk. Global cross-link audits and recovery remain future work.

`tools/folder_tests.rs` extends the host storage suite with nested paths, standard dot links, growth/reclamation, corruption/depth limits, read-only folders and injected failures. `folders_qemu.py` independently reads complete directory trees and FAT allocation after cold boots on both layouts, verifies host-created folders and nested program loading, and checks RAM navigation, busy/nonempty removal, error fallback and OEM display. Existing root-file QEMU checks still pass.

Replacement selects an entirely new chain, writes data and mirrored FAT links, flushes, updates the directory entry and flushes, then releases the old chain and flushes again. This preserves the old file on an early data-write failure and avoids silently truncating a full-disk write. It requires free space for a second copy while replacing a nonempty file. It is not a journal: failures during metadata writes can leak allocations or split mirrors, and there is no crash recovery or full-volume cross-link audit. I/O and chain-corruption errors detach the mounted filesystem and visibly fall back to RAM. New entries use the valid baseline DOS date 1980-01-01; real timestamp support remains future work.

`delete FILE` uses the active `diskfs` backend. FAT16 deletion checks the short name, refuses directories/read-only files, and validates the complete bounded chain before writing. It marks the short directory entry deleted (`0xe5`) and flushes before clearing its clusters in all FAT copies and flushing again. The tombstone preserves visibility of following entries and allows slot reuse. This ordering avoids freeing clusters while the entry is still published, but failure can leave an already removed file with leaked space or inconsistent FAT mirrors. Data sectors are not erased. Files over the current 1 KiB limit are refused. RAM deletion clears the record and releases its slot. Host tests cover reclamation, empty files, refusal without writes, and injected write/flush failures; QEMU checks both FAT16 layouts, cold boots, host FAT/cluster readback and RAM fallback.

BPB fields, short directory entries, and the cluster-count distinction between FAT12/FAT16/FAT32 follow the [Microsoft FAT specification](https://www.scs.stanford.edu/~zyedidia/docs/_other/fat.pdf).

`format` obtains a capacity-based `FormatPlan` for the selected IDE disk and arms a shell-only pending plan, without writing. It refuses a mounted volume. `run_command` cancels that plan on every command except `format confirm`; confirmation consumes it before any operation and re-identifies/revalidates capacity. `format_disk` checks the plan again before writing. It creates a non-bootable MBR with one type-`0e` partition at LBA 2048. The volume is capped at 4,192,256 sectors (just under 2 GiB); remaining disk space is unallocated. Cluster sizes are powers of two from 1 to 64 sectors, chosen to produce 4,101 through 65,508 data clusters and adequate FAT capacity. Too-small disks fail without writes.

The formatter invalidates sector 0 and flushes first, clears primary and backup GPT metadata, initializes a temporarily unsigned BPB, both FATs, and 512 root entries, then flushes before publishing the BPB and MBR signatures. Every metadata write is immediately read back and compared, with flushes at publication boundaries and a final mount-path validation. This is a quick format, not a secure wipe or an atomic operation: a failure can leave the disk partially formatted, always reported as an error. It never auto-mounts after formatting. Reformatting an installed/boot disk removes its bootability; the BIOS handoff identifies the boot source as firmware drive 0, which is refused as a format destination; explicit native IDE aliases remain selectable. The host/QEMU formatter tests use only disposable images.

`install` previews a plan and `install confirm` must immediately follow. Mounted disks are refused. Planning validates kernel bounds, capacity, partition ranges/non-overlap, and the mounted-volume validation path without writing. A compatible MBR with FAT16 and all partitions at or beyond LBA 2048 is preserved; other layouts are destructively replaced through the formatter after explicit confirmation. Confirmation revalidates capacity, the original MBR checksum, and the pristine kernel image before any write.

The boot area is fixed and disjoint from data:

| Disk sectors | Contents |
| --- | --- |
| 0 | BIOS MBR loader; installation preserves bytes 440..509 (disk ID and partition table) on compatible layouts. |
| 1..32 | Padded stage-2 BIOS loader. |
| 33..62 | Reserved gap. |
| 63 | Versioned kernel header: `XINYIK1`, load address, entry, initialized bytes, memory bytes, FNV-1a payload checksum, and a checksummed target record. |
| 64..2047 | Initialized kernel image, padded only in its last sector; at most 992 KiB. |
| 2048 onward | FAT16 data partition on fresh installations; compatible existing partitions may start later. |

Installation invalidates the MBR signature and old header, writes and reads back stage 2, the original initialized image, and its header, then flushes. It publishes the new MBR code/signature and flushes last. This avoids advertising a partially installed kernel, but is not a transactional/redundant boot update. A failure can leave the disk unbootable; retry previews must be reviewed because a damaged layout may no longer qualify for data preservation.

Stage 1 preserves the BIOS boot-drive number and requires EDD (`int 13h` extended reads). Stage 2 enables A20, gathers up to 128 E820 records, and constructs low-memory Multiboot information at `0x4000` with its map at `0x5000`. It validates header ranges and requires an available RAM range covering the entire kernel memory reservation. Single-sector BIOS reads bounce through `0x10000`; bounded unreal-mode copies transfer bytes to `0x00100000`. It checks FNV-1a, enters protected mode, zeroes all non-file memory (BSS, stacks, and snapshot space), and jumps to the kernel with the Multiboot magic and information pointer. This follows the relevant [Multiboot machine-state contract](https://www.gnu.org/software/grub/manual/multiboot/html_node/Machine-state.html); it is a loader for this fixed Xinyi image format, not a general ELF/Multiboot image loader. Invalid headers, corrupt payloads, insufficient memory, or disk read failures print diagnostics and halt.

Filesystem I/O selects either the BIOS boot device or native `diskfs::DISK`; formatting and installation destinations use explicit native IDE targets. `install_qemu.py` verifies actual BIOS disk-only boot at 32/128 MiB, memory allocation, data persistence, reinstalling from an installed session, and negative loader cases. The installed bytes are independently compared against ELF load segments, proving runtime globals were not captured. UEFI/Secure Boot are unsupported; the user tests physical removable-media/hardware boots. The untracked root-level bootloader and disk images are older experiments, outside the current Cargo loader build.

## Current Limitations

- The kernel is still experimental and intended for development and learning.
- Tasks execute synchronously and reclaim their private mappings/backing memory. Scheduling, preemption, ELF loading, writable user globals and richer application services remain unfinished.
- RAM and FAT16 support nested folders, paths, file operations and empty-directory removal. ASCII 8.3 names, 1 KiB files, eight-component paths and bounded directory chains remain current limits; append, long names and crash recovery are unfinished.
- There is no networking stack yet.
- VGA output uses shared mutable state and requires care as interrupt-driven features grow.
- The shell runs at CPL 0. The protected user demonstration cannot access supervisor memory; synchronous user faults return to the shell, while kernel faults remain fatal.
- A physical frame allocator consumes boot memory information; there is no heap allocator or frame ownership tracking.
- Supervisor kernel mappings use 4 MiB pages; private user mappings use 4 KiB pages. Non-PAE paging does not prevent execution from writable user stack pages.
- The kernel ring buffer resynchronizes modifiers after drops, but bytes discarded by QEMU's 16-byte PS/2 queue before the IRQ runs are unrecoverable.

## Explicit IDE Target Selection

The dc7800 photos established that the WDC disk identifies at PCI 00:1f.2 secondary master, while the former default chose 00:03.2 primary master. `disk select BB:DD.F primary|secondary master|slave` (or legacy instead of a PCI address) validates the BDF, PCI identity, resource routing, and successful IDENTIFY before applying a candidate. It issues no sector writes. Failed selections retain the prior target; mounted filesystems refuse selection. Selection commands cancel pending destructive confirmations.

`diskfs` owns a shared ATA device used by mount, format, and installer. A selected PCI identity/channel/drive is resolved against current BARs before device initialization. Previews include the device report. The first IDE controller's primary master remains only the default when no explicit or installed selection exists. Scans never select automatically.

`disk_target::Target` stores a 20-byte XDT1 record at kernel-header offsets 32..51: PCI BDF (or a legacy sentinel), vendor/device ID, channel/drive flags, and FNV-1a record checksum. Stage 2 validates its checksum and field bounds, copies it to 0x4080 after the standard Multiboot fields, and the kernel restores it after boot-information parsing. This is after the pristine-image snapshot, so saved state does not contaminate installed kernel bytes. A saved PCI identity that no longer resolves fails access without falling back to another device. It identifies a controller/location, not a unique disk serial; hot swapping is unsupported.

Twenty-three host tests include path/record and capacity validation, refusal without writes, and preserving compatible partitions on address-limited devices. QEMU tests cover explicit selection, selected-secondary-master installation, target restoration, persistence and idle halting/wakeups. LBA48 tests exercise files beyond 128 GiB, unchanged low-address aliases, preserving and fresh installation on sparse 160 GiB images, actual-end GPT cleanup, and disk-only boot. A sparse 3 TiB disk still refuses whole-disk formatting without guest-data allocation. Automated testing uses QEMU; the user handles physical tests when needed. Physical LBA48 operation remains unverified.

`tools/graphics_bench.py` measures key-to-pixel latency with private QEMU runs. The debug/TCG test on this host measured roughly 294 ms before caching and 4.5 ms after; these are host-specific input-to-framebuffer observations, not display frame-rate guarantees.

### BIOS boot device and VESA bridge

The stage-2 loader appends an XBI1 handoff at Multiboot-info offsets 148/152 containing the original BIOS boot-drive number. Only our low-memory BIOS loader enables the bridge. `bios_thunk.asm` is copied to reserved physical 0x7000, with context at 0x6000, disk packet at 0x6100, real stack below 0xf000, sector bounce at 0x10000 and firmware query buffers at 0x18000. All are below the allocator's 1 MiB boundary. Calls serialize, preserve general registers, kernel stack/GDT/IDT/CR0/CR3/CR4, temporarily disable paging/protection, and invoke INT 10h or INT 13h in real mode. PIC vectors/masks and PIT rate temporarily match BIOS operation; NMI is masked during mode transitions. Return restores kernel PIC/PIT and interrupt enable state, drains pending PS/2 bytes to avoid lost keyboard IRQ edges, and leaves user mappings unchanged. This is synchronous firmware access; OS tick accounting pauses during firmware calls.

`bios_disk` validates EDD, 512-byte sectors and u32-addressable capacity. AH42/AH43 transfer one sector using a low-memory DAP/bounce buffer. Writes are read back; `flush` checks completion status, without claiming a portable hardware cache flush or crash consistency. `diskfs` automatically mounts the BIOS boot filesystem as numbered drive 0 and reports failures without choosing another disk. Other IDE devices remain numbered from 1; format/install reject firmware drive 0. `install NUMBER` stages ordinary application files from the USB filesystem, installs to the selected native IDE destination, then copies them. No native USB controller/hotplug support is implied.

The existing Bochs/QEMU backend remains. Other BIOS-booted displays query VBE 2/3 controller and mode records, validate supported linear direct-colour 800x600 modes (RGB565, 24/32-bit BGR memory), pitch, channel masks, alignment and address bounds, then select the highest supported colour depth. VBE3 uses its linear stride/masks. Firmware supplies an 8x16 font. The renderer retains its RGB32 RAM scene and dirty rectangles, translating writes to the selected pixel depth/stride. Exit calls BIOS text mode 3 and restores saved text/cursor; this restores the standard BIOS font, not an arbitrary custom VGA font. Frame allocations are reclaimed on success/failure. Direct Multiboot boots cannot call this firmware path. The user confirmed USB apps, VESA graphics and reboot persistence on an HP laptop; dc7800 verification remains separate; QEMU USB/Cirrus exercises the generic 24-bit VESA path, independently from Bochs VGA.

### PS/2 mouse and graphical interaction test

`ps2::drain` serializes IRQ1/IRQ12 reads from the shared 8042 FIFO and dispatches each byte by AUX status; parity/timeout errors reset the corresponding input state. Firmware returns drain the same FIFO, with BIOS keyboard/mouse IRQ handlers masked during firmware calls. `mouse::init` runs on first use: bounded controller waits, keyboard preserved during transactions, AUX port enabled, F5/F6/F4 acknowledged (with bounded resend), then IRQ12 and the master cascade unmasked. Failure restores keyboard operation and reports unavailability. This selects standard three-byte packets rather than negotiating wheel/extra buttons.

`mouse_packet` decodes header synchronization, signed nine-bit movement, screen-Y inversion and per-axis overflow. IRQ work is bounded; events enter a separate 128-slot queue. Incomplete packets time out, and queue/input corruption cancels UI capture with a current-button snapshot. The shell ignores queued pointer events when deciding to halt; the mouse test rechecks both input queues atomically before STI/HLT.

`mouse_ui` is pure state/hit testing: button press/release capture, no click-through on menu dismissal, bounded menu positioning and saturated counters. `graphics::mouse_demo` uses the existing graphics session/recovery paths, a cursor-free RGB32 scene buffer and a transparent 16x24 black/white arrow overlay. Movement copies the old footprint then draws the new cursor; button/menu changes present only affected regions. Escape and fatal exceptions restore text and reclaim graphics frames. QEMU tests cover standard VGA and BIOS/Cirrus VESA, including file-backed app loading after mouse initialization and subsequent pointer operation. The user has not yet tested the PS/2 mouse path on hardware.

### Initial window manager

`desktop_ui` maintains two fixed windows, visibility, bottom-to-top order, optional focus, pointer capture and drag offsets without hardware access or allocation. Hit testing traverses visible windows front to back: the first opaque client blocks lower windows. Mouse-down raises/focuses that window; title capture moves it with a stable grab offset, clamped to the desktop area, including movement on the release packet. Close/control activation requires matching press/release targets, while input loss cancels capture. Closing chooses the next visible focus; a topmost right-click menu can restore both windows, retaining button counters. An outside menu click is consumed.

`graphics::desktop_demo` shares the graphics session, pointer overlay and fatal/normal text recovery. State changes recompose all visible windows into the cursor-free RGB32 RAM scene in z order. Presentation repairs moved/hidden window bounds (including shadows), newly raised bounds, changed title bars/controls/counters and old/new menu bounds. This preserves the existing clipped pixel conversion/stride handling for QEMU and VESA. No full-frame VRAM copy is used for ordinary dragging; pure pointer motion only restores/draws the cursor. This remains a fixed-window diagnostic, without program-owned surfaces, resizing or multitasking. Host state tests and QEMU framebuffer-to-RAM comparisons cover occlusion and damage repair, with repeated frame reclamation and fatal recovery checks.

### Simple GUI bitmap font

`ui_font::FONT` is a const-built 4096-byte font table using original five-column/seven-row glyph definitions for printable ASCII. Symmetric column expansion and fixed row expansion produce 7x11 glyphs with padding in an 8x16 cell. The mouse/desktop renderers pass this table to the existing clipped Canvas text rasterizer; no font parser, filesystem lookup or additional allocation is required. Fixed advance/line height preserve control geometry and damage regions. BIOS font capture remains available to the original graphics diagnostic and console recovery path.

### Foreground window ABI v1 — 27 October 2026

Syscalls 60 open (ASCII title pointer/length), 61 event (handle/writable Event pointer/wait flag), 62 draw (handle/Draw pointer/count), 63 present and 64 close provide one program-owned foreground window. `window_protocol` shares repr(C) POD records with the SDK: Event is five 32-bit fields (kind, code, client x/y, buttons); Draw is eight fields (kind, x/y, width/height, RGB24, text pointer/length). Events are key, client pointer, close request and input reset. A close request keeps the surface alive so an application can ask about unsaved changes. Handles are monotonic and owner checked after validating current ring-3 CS/CR3. No client memory or video pages are exposed to the program.

`window_api` owns an allocated 640x400 RGB32 surface. A batch has at most 16 commands; all geometry, colours, input ranges and text bytes are copied/validated before touching pixels. Event output is checked against writable stack pages before consuming events. Draws update retained client memory and dirty 16-pixel strips; present recomposes the scene and transfers those strips, with cursor repair. The application should present before waiting for events; moving/repainting chrome also recomposes retained client content. IRQs stay enabled during rendering, with IF=0 restored before syscall IRET. Blocking event waits atomically check queues before STI/HLT. Text-console writes/key reads/drawing return busy while GUI is active.

`app_window_ui` handles bounded dragging, focus, client capture and cancellable close requests. `input_order` assigns wrapping capture sequence numbers to keyboard bytes and complete mouse packets, and the GUI consumes the oldest queued input first. This prevents keys overtaking earlier clicks while an application redraws. Loss resets capture conservatively. Only IRQ producers touch ordering slots; they publish them with the corresponding queue head.

Exit and recoverable user faults release the window and restore text before shell reporting. OOM/unsupported-display failures drop partially allocated surfaces. Filesystem calls retain the existing range checks and persistence semantics. `programs/notepad/main.rs` reuses the editor document model and save/unsaved-change logic, adds a mouse toolbar/caret and cached graphical rows, and ships as a normal FAT16 application. Tests cover ring-3 invalid addresses/commands, no partial invalid-batch output, handle reuse rejection, fault and allocation-failure frame balance, graphical interactions and saved content after a fresh VM. This is cooperative foreground execution, without per-program windows under multitasking.

### Cursor-preserving drag transfers

`present_rect` now copies spans using REP MOVSD (saving/restoring LLVM's reserved ESI) with volatile byte tails. Packed VESA formats are converted into a bounded RAM row buffer before transfer. `move_pointer` first restores/draws the new cursor footprint, then repairs the old one while excluding the new footprint. `present_guarded` splits intersecting spans around that footprint; window, menu and application-client repairs cannot erase the visible cursor during a long transfer. Scene RAM stays cursor-free and existing 16/24/32-bit conversion/clipping rules remain.

Mouse, desktop and application-window loops combine queued motion only while buttons are unchanged, before the next queued keyboard byte, and without crossing reset/loss boundaries. They apply each packet to the model to preserve intermediate clamps/capture, then compose once. Press/release events stay separate. The existing full framebuffer composition checks and window/file API tests run with the new blitter; `window_drag_qemu.py` adds burst motion and cursor visibility measurements.

### Synchronous desktop launcher — 27 October 2026

`app_catalog` builds a bounded, sorted list of 16 regular ASCII `.BIN` entries from `/APPS`. Labels and explicit paths are derived from short filenames; programs are never embedded in the menu. The taskbar/popup use the desktop model's existing press/release capture and opaque overlay rules, with no click-through on dismissal. Missing/empty catalogs remain usable. Each desktop session loads the catalog before entering graphics and refreshes it after an application returns.

Selecting an entry ends the desktop graphics session before calling the existing protected program loader with the absolute application path. The desktop model stays owned on the caller stack. On return, graphics are re-entered and the retained positions/order/focus/counters are rendered again; input capture and popups are reset. No window/framebuffer ownership is shared with the foreground application. Kernel recovery handles user faults as before, while loader failures leave a desktop status message and shell diagnostics. This is synchronous execution, not scheduling or simultaneous application windows. Launcher tests exercise an application that creates another application file, graphical/text launches, CWD shadow prevention, saving in the current directory, faults, empty programs and full frame reclamation.

### Timer-driven user scheduling — 27 October 2026

`multitask` loads two independent copies of `/APPS/COUNTER.BIN` before entering graphics. Each job owns its image, private CR3, user stack/guard, 64 KiB kernel interrupt stack, saved segment/GPR/IRET context and x87/MMX state. SSE/AVX remain disabled for this ABI. The host supervisor stack is now 64 KiB as well: the initial 16 KiB was insufficient for the debug dispatcher/compositor call chain. Bottom sentinels are checked after quanta and input pumping. The original synchronous task-record prototype remains separate.

IRQ0 completes its EOI, then captures a context only when the interrupted CS is ring 3. After the C handler returns, assembly restores the host CR3/stack/callee state using the existing recovery ABI. `usermode::quantum` returns normally to the dispatcher; Rust kernel frames and mutable surface borrows are never suspended across a switch. The dispatcher pumps input/composes damage, selects a ready job round-robin, restores its FPU state and resumes its saved context with IRETD. Kernel IRQs do not switch jobs. Sleeping deadlines use wrapping PIT ticks; when no job is runnable the supervisor halts until input/timer interrupts.

`multi_window` owns four bounded owner-checked slots with independent RGB32 surfaces and 32-event queues. It routes keyboard input to focus and mouse input to the topmost hit/captured window, preserves close requests, and composites bottom-to-top with cursor-protected damage presentation. Exit/fault/forced termination remove only that owner's surface, then release the job after restoring host mappings. Escape exits this demonstration group; Ctrl+Alt+K force-stops its focused job. Syscall 65 adds a sized window (width in low 16 bits, height in high 16), bounded to 160..640 by 80..400 with 8/16-pixel alignment; call 60 retains 640x400 defaults. Call 66 voluntarily yields. Blocking call 61 captures an input-wait state and rewinds the two-byte INT instruction to revalidate/retry on wake; no user pointer survives as a borrowed kernel reference.

Text-console, file and speaker calls return busy in this first concurrent group. Files are loaded before scheduling, avoiding firmware/filesystem reentrancy. General spawn/wait/process listing, concurrent file access, writable globals, resizing, arbitrary app launching and integration with the desktop launcher remain unfinished. The real ring-3 Counter regression holds distinct x87 values/control words, sentinel GPRs and DF through a syscall-free loop; both jobs must be timer-preempted and exit successfully. Additional QEMU modes verify a non-yielding task cannot starve its neighbour or input processing, and that wait, close and user-fault cleanup reclaim all frames.

The taskbar is currently at the top in desktop, foreground-app and multitasking views; Apps opens downward. Window dragging keeps chrome below the header and allows the former bottom taskbar space to be used.

### Concurrent desktop application launcher — 27 October 2026

`desktop` now enters `multitask::desktop`; the old fixed-window/synchronous launcher remains `windowtest` for regression diagnostics. The dispatcher owns eight optional jobs, limited to four root application windows and their terminal children and loads a selected explicit catalog path into a free slot before running it. Each job has its own code, arguments, user/kernel stacks, page tables, CPU/FPU state and PID. Closing/faulting/force-stopping releases only that job, and an empty desktop stays active until Exit or Escape. Allocation/load failures leave existing jobs intact.

`desktop_shell` is a pure model for the top Apps menu, running-task buttons and Exit. Press/release capture consumes menu dismissal through the matching release. `multi_window` routes chrome first, then the frontmost/captured app; dragging retains capture across the taskbar. Running buttons select PIDs, including tasks that have not opened a window, so Ctrl+Alt+K can stop windowless CPU hogs. Window focus/order and taskbar focus are updated together. Escape goes to the focused app unless dismissing a menu or leaving an empty desktop; Exit with running jobs only shows a status message.

The desktop permits existing file calls 57/58: their input/output buffers are validated/copied as before, and kernel calls complete before the scheduler can run another job. BIOS disk operations may briefly delay input/redraw, but cannot overlap another filesystem mutation. There are no retained filesystem borrows across quanta. Working directory is inherited from the shell and remains shared/unchanged during the desktop. Multiple editors may save the same path; there is no conflict detection or document merge.

No executable metadata or built-in name registry is required. A windowless desktop task's first console request captures a `Console(number)` pause and rewinds INT 80. The dispatcher preserves that job and creates a Terminal root using its taskbar slot and working directory. Once the terminal opens its PTY, the original job attaches and retries the console call. Programs retain their CPU/memory state and earlier side effects. Missing Terminal or allocation/open failures stop the waiting child cleanly. Direct physical-shell launches and the legacy `windowtest` retain synchronous foreground behavior.

The release test launches two Notepads and counters, checks separate file contents after task switching and cold boots, rejects a fifth launch, retains unsaved documents on Exit/Escape, handles empty/faulting/windowless-hog apps, refreshes a newly created executable, and returns from Hello/Snake without losing other jobs. All frames must be reclaimed after leaving. Kernel work remains non-preemptible; writable globals/ELF, asynchronous I/O, general process APIs and desktop resizing remain future work.

### Graphical Terminal, shared clipboard and selection — 27 October 2026

`clipboard` owns a fixed 1024-byte ASCII text value. Calls 67/68 validate current CS/CR3, ranges, capacity and reserved fields before copying. Set stages and validates the whole input before mutation; get never partially copies. IRQs never access the clipboard and a syscall cannot be preempted by another job. It is session memory and resets with kernel BSS on boot.

Notepad keeps an optional selection anchor and a drag state around the existing Document cursor. Keyboard movement extends/collapses that range, mouse client capture extends it through release, and input reset cancels dragging. The Document model provides atomic range replacement with a single undo checkpoint, bounded normalization and per-row selection spans including selected newlines. Rendering caches both text and selection spans, recolouring changed spans while retaining the existing caret and dirty presentation paths. Copy/cut validate clipboard success before deleting; paste normalizes into bounded staging buffers before checking replacement capacity.

`programs/terminal_gui` now owns a kernel PTY session rather than a second command interpreter. `shell.rs` shares the physical console's line editor, eight-command history, completion and `run_command` dispatcher. `pty_model.rs` provides 80×25 coloured cells, a 64-row history ring and basic ANSI cursor/erase/SGR parsing. VGA helpers route console operations to the current session; the physical console remains the default. Each session owns eight physical pages and bounded keyboard, selection and render-cache storage.

Host-side shell service runs between user quanta. A launch copies the session directory and arguments into a scheduled child, attaches its console/key calls to that session, and suspends command input until completion. Waiting for a terminal key saves context and retries INT 80 after session input arrives. Text games and EDIT therefore run concurrently with GUI applications. Opening a GUI window detaches the child into a free root task slot and resumes the shell prompt. Ctrl+C queues foreground termination; closing an occupied terminal requires confirmation and reaps its child. Owner close/fault releases session storage and pending work. No kernel Rust frame is suspended by user preemption.

Calls 72..77 open/control, present, enqueue input, paste, select and copy a PTY. Ownership is checked against PID/active CR3, input and output ranges are validated, and presentation targets only an owned 640×400 window. Changed cell rows are rendered from the captured BIOS CP437 font with the VGA palette. Clipboard data remains bounded to 1024 ASCII bytes. Call 69 is retired (-38); call 70 reports cwd and 71 queues a copied desktop launch. Each job and session restores its directory around execution, and graphical children inherit the launch directory.

`directory_protocol.rs` defines a zero-initialized 776-byte listing: 32 entries (12 name bytes, name length, size and directory flag) plus count/total. Calls 78..80 list a page, change the caller's directory and create/delete entries through checked `diskfs` operations. Listing offsets permit large directories without an unbounded user/kernel buffer. Validation completes before filesystem work; interrupts are enabled for synchronous I/O and disabled before returning from the syscall. Errors retain existing files and nonempty directories.

`programs/files` is an ordinary protected filesystem app. It lists 18 entries per view, navigates folders, confirms deletion, creates folders and opens text files in Notepad. Executables launch Terminal with an initial `run` command. Window clamping uses y=24, protecting the top taskbar while allowing desktop headings to be covered. Taskbar labels contain program names without PID suffixes.

QEMU coverage includes the shared shell, windowed Snake/Tetris/EDIT, interrupt and forced terminal close, GUI detachment/cwd inheritance, clipboard across apps, directory ABI errors/pagination, Files create/open/save/delete, persistent files and complete frame reclamation. Pipes, scripting, POSIX process/TTY semantics, native USB hotplug and larger text files remain future work. Kernel I/O can briefly delay other jobs.


Desktop windows can be dragged partly outside the left/right/bottom edges while retaining a reachable title strip and y>=24. Client blits clip screen and source coordinates before indexing either surface. Minimized windows retain their surfaces and scheduled processes, receive no focused input, and do not repaint hidden client updates. Taskbar activation restores and clamps the window fully into view. Minimize uses captured press/release semantics; dragging out cancels it and another visible window takes focus.
