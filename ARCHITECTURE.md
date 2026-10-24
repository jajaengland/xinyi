# Xinyi Architecture

> **Documentation notice:** This document was AI-assisted and may not have been written entirely by a human. Verify technical details against the source code.

This document is intended to help AI assistants understand the code.

## Overview

Xinyi is an experimental 32-bit x86 bare-metal kernel written primarily in Rust, with NASM assembly used for bootstrapping and interrupt handling.

The kernel runs without the Rust standard library and currently provides VGA text output, an interactive shell, hardware interrupt handling, timer-based uptime tracking, keyboard input, CPU exception handling, and identity-mapped paging.

## Boot Process

1. The boot assembly entry point disables interrupts.
2. It establishes a stack for the kernel.
3. It passes the multiboot magic value and information pointer to the Rust entry point, `rust_main`.
4. The kernel validates and preserves the boot information, installs its own flat GDT, and reloads all segment registers.
5. The kernel enables identity-mapped paging with 4 MiB pages.
6. The kernel initializes its interrupt handling and hardware timer.
7. Interrupts are enabled.
8. The VGA display is initialized and the interactive shell starts.

## Build System

The project uses a custom target specification, `i686-unknown-none.json`, for 32-bit x86 bare-metal compilation.

The build uses Rust nightly and the `core` library, built with:

`cargo +nightly build -Z build-std=core`

`build.rs` assembles the NASM source files and links their output into the kernel.

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

`src/gdt.rs` defines the descriptors and TSS. `gdt_flush` in `src/boot.asm` loads the GDTR, reloads `DS`, `ES`, `FS`, `GS`, and `SS` with `0x10`, and performs a far jump to reload `CS` with `0x08`. `tss_flush` loads the TSS with `LTR`. `SS0` and `ESP0` are set to the kernel data selector and boot stack so ring 3->ring 0 transitions via interrupts/syscalls can switch stacks safely. User-mode descriptors use RPL 3 and DPL 3. `enter_user_mode` in `src/boot.asm` builds the `IRET` frame (`SS`, `ESP`, `EFLAGS`, `CS`, `EIP`) and switches to ring 3. There are still no per-process address spaces or page-level protections; the identity map marks all pages user-accessible to allow the ring 3 test, so it is not an isolation boundary.

## Boot Information and Memory Map

`src/boot.asm` preserves the multiboot values from `EAX` and `EBX` when calling `rust_main(magic, info_addr)`. `src/multiboot.rs` accepts the information only when `magic == 0x2BADB002` and the pointer is nonzero. That prevents the kernel from dereferencing untrusted data when started by a different loader.

The memory summary comes from the `mem_lower` and `mem_upper` fields. The memory map comes from the `mmap_length`/`mmap_addr` fields when the corresponding information flag is set. Entries are read with unaligned loads, each entry's size field is checked, truncated entries are rejected, and the total map length is capped at 64 KiB before iteration. The results are currently diagnostic only; no allocator consumes them yet.

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

## Memory Management Plan

The eventual physical-memory manager should follow these constraints:

- Allocate only from regions marked available (type 1).
- Never hand out memory below `0x00100000`; real-mode firmware data and the VGA window live there.
- Treat the kernel image, boot stack, IDT/GDT, VGA buffer, multiboot information structure, and loaded modules as reserved, regardless of what the map says.
- Page-align allocations and validate requests against the detected map rather than assuming a fixed amount of RAM.
- Keep the active flat identity mapping while adding a physical allocator. The current mapping covers the full 32-bit space, but the allocator must still restrict itself to available regions.
- Before adding user-mode address spaces, introduce 4 KiB page tables and per-process page directories. Identity-map the kernel and VGA memory in every address space, then map user pages separately.
- Keep the first allocator deliberately simple (for example, a bump or free-list allocator over available physical frames) and test it against the `memory` report before adding virtual-memory features.

## Physical Memory Allocation

`src/phys.rs` tracks 4 KiB frames using a 4 MiB bitmap covering the full 32-bit physical space. On startup:

1. Every frame is marked reserved.
2. All multiboot memory-map regions with type 1 are marked free; if no map is present, the allocator falls back to `mem_lower`/`mem_upper`.
3. The kernel reserves `0x00000000`–`0x00100000`, the kernel image (`_kernel_start`..`_kernel_end`), the page directory, the frame bitmap, and the multiboot information and memory-map structures.
4. `init()` reports the number of available frames.

`alloc_frame()` performs a linear scan starting from the last allocated frame, returns an identity-mapped physical address, and marks the frame used. `free_frame()` validates alignment and the frame state, clears it, and adjusts the search hint downward. There is currently no ownership tracking, so `free_frame()` must only be called with addresses returned by `alloc_frame()`. The allocator provides physical frames only; a separate heap allocator will sit on top of it later.

The `frames` command reports available/allocated/free counts and runs a quick alloc/free self-test.

## Paging

`src/paging.rs` enables paging during boot with 4 MiB pages using Page Size Extension (PSE). Because the entire 32-bit physical address space fits in a single page directory:

- 1024 page-directory entries cover 0 through 4 GiB.
- Every entry has present, writable, and 4 MiB page (`PS`) set.
- The page directory is 4 KiB aligned and lives in `.bss`.
- `CR4.PSE` is set before `CR0.PG`; `CR3` receives the page-directory physical address.
- `CPUID` leaf 1 checks for PSE first. If PSE is unavailable, paging is left disabled and the kernel prints a warning after VGA initialization instead of faulting.

This mapping is deliberately identity-based (`virtual == physical`), so the kernel, stack, IDT/GDT, VGA buffer, multiboot data, and all RAM remain reachable. It is a minimal, testable paging foundation rather than a protection boundary: every mapped page is writable from ring 0. User-mode isolation will require 4 KiB page tables, page-level permissions, and per-process address spaces after a frame allocator exists.

The `paging` command reports whether paging is enabled, the page size and mapped range, and the current `CR3`, `CR0`, and `CR4` values.

## VGA Text Output

`src/vga.rs` implements text-mode output using the VGA text buffer at physical address `0xB8000`.

The writer supports:

- Character and formatted text output
- Text colours
- Scrolling
- Backspace handling
- Hardware cursor updates

The current implementation uses a single global mutable writer. Output is produced only from normal shell/kernel context and from the CPU exception handler, which halts after printing. The keyboard and timer IRQ handlers do not write to VGA, so there is currently no concurrent writer. Care is still required before adding any VGA output to an interrupt handler: such output would need explicit serialization, or should use the debug console instead.

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
| `usermode.rs` | Ring 3 transition test. `boot.asm::enter_user_mode` builds the `IRET` frame. User code writes directly to VGA, calls `int 0x80` (`getpid`), then executes a privileged `CLI`. | `enter`. |
| `multiboot.rs` | Validated access to the multiboot memory summary and memory map. Reads physical addresses through the current identity map. | `init`, `memory_summary`, `for_each_region`, `Region`, `region_kind`, `info_address`, `mmap_range`. Rejects a bad magic value, missing flags, oversized maps, and truncated entries. |
| `paging.rs` | Identity-mapped paging with 4 MiB PSE pages covering 0-4 GiB. | `init`, `status`, `Status`, `PAGE_SIZE`, `MAPPED_BYTES`. `init` validates PSE support and returns an error without touching CPU state when unavailable. |
| `phys.rs` | 4 KiB physical frame bitmap allocator seeded from the multiboot map with reserved regions. | `init`, `alloc_frame`, `free_frame`, `stats`, `Stats`. Tracks free frames only by bitmap state; ownership is not enforced. |
| `keyboard.rs` | PS/2 scancode collection and translation. IRQ producer, shell consumer via a ring buffer. | `handle_irq`, `poll_key`, `poll_char`, `Key`, `left_shift_down`, `right_shift_down`, `caps_lock_on`, `queued_scancodes`, `dropped_scancodes`. Caps Lock toggles only on the press edge. Drops trigger modifier resynchronization while preserving Caps Lock's toggled state. |
| `interrupts.rs` | IDT setup, PIC remapping, PIT timer, exception and IRQ dispatch, syscall gate. | `init`, `remap_pic`, `init_pit`, `timer_ticks`, `uptime_seconds`, `xinyi_exception_handler`, `xinyi_irq_handler`, `xinyi_syscall_handler`. Exception frames pass vector, error code, EIP, CS, and EFLAGS to the handler. |
| `interrupts.asm` | NASM entry stubs for ISRs and IRQs, and calling convention normalization. Syscall stub for `int 0x80`. | `isr0..isr31`, `irq0..irq15`, `isr128`. |
| `rtc.rs` | CMOS real-time clock reading with BCD/binary and 12/24-hour handling. | `read`, `DateTime`. |
| `debug.rs` | Minimal I/O to QEMU debug console port `0xE9` for headless diagnostics. | `write_byte`, `write_str`, `write_fmt`, `write_hex8`, `write_u32`, `write_hex32`. |

Error-handling conventions in the current kernel are simple and explicit: recovery paths fall back to safe defaults (e.g., ignoring unrecognized scancodes), ring-buffer overflow is counted and triggers state resynchronization rather than undefined behavior, and fatal conditions (`panic!` or CPU exceptions) print diagnostics to VGA and the debug console and halt.

## Current Limitations

- The kernel is still experimental and intended for development and learning.
- There is no implemented user-mode process system yet.
- There is no filesystem or networking stack yet.
- VGA output uses shared mutable state and requires care as interrupt-driven features grow.
- The current shell operates directly in kernel mode (CPL 0); user code can run at ring 3 and is currently sandboxed only by the CPU's privilege checks against privileged instructions. Page-level isolation and per-process address spaces are not implemented yet.
- Boot memory information is reported but not consumed by an allocator: there is no physical frame allocator yet.
- Paging identity-maps the full 32-bit address space with 4 MiB pages. It provides no per-process address space or user-mode protection yet.
- The kernel ring buffer resynchronizes modifiers after drops, but bytes discarded by QEMU's 16-byte PS/2 queue before the IRQ runs are unrecoverable.