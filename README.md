# Xinyi

WARNING: Below is not human written

Xinyi is an experimental 32-bit x86 kernel written primarily in Rust, with a small amount of x86 assembly for bootstrapping and interrupt handling.

The project is being developed as a learning project, with the long-term goal of building a small operating system with a shell, applications, and eventually a graphical interface.

## Current Features

- 32-bit x86 bare-metal kernel
- Rust `no_std` environment
- VGA text-mode output with scrolling and hardware cursor support
- Interactive shell
- Command history and command-line editing
- Tab completion for commands
- Real-time clock date and time command
- CPU exception handlers and readable exception messages
- Interrupt Descriptor Table (IDT)
- Programmable Interrupt Controller (PIC) remapping
- Programmable Interval Timer (PIT) at 100 Hz
- Timer-based uptime tracking
- Interrupt-driven PS/2 keyboard input using a ring buffer
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
| `breakpoint` | Test the CPU breakpoint exception handler |
| `reboot` | Attempt to reboot the machine |

The shell also supports command history, cursor movement, Backspace, and Tab completion for uniquely matching commands.

## Project Documentation

- [Architecture and implementation details](ARCHITECTURE.md)
- [Development roadmap and known limitations](TODO.md)

## Development Notes

Xinyi is an experimental kernel and is not intended for production use. Features and internal interfaces may change as development continues.

When making changes, build and test the kernel before committing. Preserve working functionality and update the architecture and roadmap documents when behavior changes.