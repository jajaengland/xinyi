# Xinyi

WARNING: Below is not human written

Xinyi is an experimental 32-bit x86 kernel written primarily in Rust, with a small amount of x86 assembly for bootstrapping and interrupt handling.

The project is being developed as a learning project, with the long-term goal of building a small operating system with a shell, applications, and eventually a graphical interface.

## Current Features

- 32-bit x86 bare-metal kernel
- Rust `no_std` environment
- Kernel-owned flat Global Descriptor Table (GDT)
- VGA text-mode output with scrolling and hardware cursor support
- Interactive shell
- Command history and command-line editing
- Tab completion for commands
- Real-time clock date and time command
- CPU exception handlers with fault details (vector, error code, EIP, CS, EFLAGS, and CR2 for page faults)
- Kernel panic reporting with message, source location, and debug-console output
- Interrupt Descriptor Table (IDT)
- Programmable Interrupt Controller (PIC) remapping
- Programmable Interval Timer (PIT) at 100 Hz
- Timer-based uptime tracking
- Interrupt-driven PS/2 keyboard input using a ring buffer
- Bootloader memory-summary and memory-map reporting
- Identity-mapped paging using 4 MiB PSE pages
- Ring 3 user-mode entry with a TSS-backed kernel stack
- `int 0x80` system-call interface
- Basic in-memory process table and filesystem commands
- Reboot command

## Requirements

Development currently uses MSYS2 UCRT64 on Windows.

Required tools include:

- Rust nightly toolchain
- Rust source component (`rust-src`)
- NASM assembler
- QEMU system emulator for x86
- Cargo and the Rust build tools

The project uses a custom target specification: `i686-unknown-none.json`.

## Build

Run this from the repository root:

`cargo +nightly build -Z build-std=core`

## Run in QEMU

After a successful build, run:

`qemu-system-i386 -kernel target/i686-unknown-none/debug/rust-os`

## Debugging

Fault and panic details are mirrored to QEMU's debug console at I/O port `0xE9`. To capture them without opening a display:

`qemu-system-i386 -kernel target/i686-unknown-none/debug/rust-os -display none -no-reboot -debugcon file:debug.log -global isa-debugcon.iobase=0xe9`

`-no-reboot` makes QEMU exit instead of restarting after a triple fault. The `keyboard` command reports modifier state and scancode statistics. The `breakpoint` and `panic` commands exercise the fault-reporting paths.

With `-display none`, the 80x25 VGA text buffer can still be read through QEMU's QMP interface with `pmemsave` at guest physical address `0xB8000`, size `80 * 25 * 2`.

If the build cannot find NASM on Windows, add MSYS2's tools to `PATH` for the session:

`$env:PATH = "C:\msys64\usr\bin;$env:PATH"`

## Shell Commands

| Command | Description |
| --- | --- |
| `help` | Show available commands |
| `about` | Show information about Xinyi |
| `clear` | Clear the screen |
| `echo TEXT` | Print text |
| `version` | Show kernel version |
| `date` | Display date and time from the RTC |
| `uname` | Show architecture and environment |
| `uptime` | Show time since PIT initialization |
| `keyboard` | Show keyboard modifier state and scancode buffer diagnostics |
| `memory` | Show the multiboot memory summary and memory map |
| `paging` | Show paging configuration and control-register state |
| `frames` | Show physical frame allocation statistics and run a self-test |
| `usermode` | Test ring 3 user-mode execution |
| `ps` | Show running processes |
| `run [prog]` | Run a user program |
| `ls` | List files |
| `touch FILE` | Create a file |
| `cat FILE` | Display file contents |
| `breakpoint` | Test the CPU breakpoint exception handler |
| `panic` | Test the kernel panic handler |
| `reboot` | Attempt to reboot the machine |

The shell also supports command history, cursor movement, Backspace, and Tab completion for uniquely matching commands.

Unknown commands are reported with a quoted name. When exactly one known command matches by prefix, the shell also prints a case-insensitive "Did you mean" suggestion. Commands are separated from their arguments by whitespace, and repeated spaces are ignored.

## Project Documentation

- [Architecture and implementation details](ARCHITECTURE.md)
- [Development roadmap and known limitations](TODO.md)

## Development Notes

Xinyi is an experimental kernel and is not intended for production use. Features and internal interfaces may change as development continues.

The kernel currently builds with a small number of dead-code warnings for helper functions that are intentionally being kept available during development.

The keyboard IRQ drains all pending PS/2 bytes on each interrupt. If the kernel's scancode ring buffer overflows, it counts the dropped bytes and resets Shift and extended-prefix state so a lost release cannot leave a modifier stuck. Caps Lock toggles only on the press edge, so holding the key or auto-repeat cannot flip it repeatedly. The `keyboard` command reports this state and the drop count.

QEMU's emulated PS/2 controller queue is small (16 bytes) and can discard bytes before the kernel sees them during large synthetic input bursts. No kernel-side recovery can restore events that never arrive; normal typing is unaffected.

When making changes, build and test the kernel before committing. Preserve working functionality and update the architecture and roadmap documents when behavior changes.