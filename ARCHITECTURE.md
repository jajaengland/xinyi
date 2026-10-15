# Xinyi Architecture

> **Documentation notice:** This document was AI-assisted and may not have been written entirely by a human. Verify technical details against the source code.

This document is intended to help AI assistants understand the code.

## Overview

Xinyi is an experimental 32-bit x86 bare-metal kernel written primarily in Rust, with NASM assembly used for bootstrapping and interrupt handling.

The kernel runs without the Rust standard library and currently provides VGA text output, an interactive shell, hardware interrupt handling, timer-based uptime tracking, keyboard input, and CPU exception handling.

## Boot Process

1. The boot assembly entry point disables interrupts.
2. It establishes a stack for the kernel.
3. It calls the Rust entry point, `rust_main`.
4. The kernel initializes its interrupt handling and hardware timer.
5. Interrupts are enabled.
6. The VGA display is initialized and the interactive shell starts.

## Build System

The project uses a custom target specification, `i686-unknown-none.json`, for 32-bit x86 bare-metal compilation.

The build uses Rust nightly and the `core` library, built with:

`cargo +nightly build -Z build-std=core`

`build.rs` assembles the NASM source files and links their output into the kernel.

## VGA Text Output

`src/vga.rs` implements text-mode output using the VGA text buffer at physical address `0xB8000`.

The writer supports:

- Character and formatted text output
- Text colours
- Scrolling
- Backspace handling
- Hardware cursor updates

The current implementation uses a single global mutable writer. Care is required if output is ever shared between normal kernel code and interrupt handlers.

## Interrupt Handling

Interrupt-related code is split between assembly and Rust.

- `src/interrupts.asm` contains exception and hardware IRQ entry stubs.
- `src/interrupts.rs` defines and installs the Interrupt Descriptor Table (IDT), configures the Programmable Interrupt Controller (PIC), and handles timer and keyboard interrupts.

The assembly stubs preserve the required processor state and transfer control to the Rust interrupt handler.

The PIC is remapped so hardware IRQs use vectors 32 through 47. IRQ0 is used for the timer, and IRQ1 is used for the keyboard.

## Timer and Uptime

The Programmable Interval Timer (PIT) is configured to generate approximately 100 interrupts per second.

The timer interrupt increments an atomic tick counter. The `uptime` shell command converts the elapsed ticks into hours, minutes, and seconds.

## Keyboard Input

`src/keyboard.rs` handles PS/2 keyboard input.

The keyboard IRQ handler reads scancodes from the keyboard data port and places them into a ring buffer. The shell consumes scancodes from that buffer and translates them into key events.

The keyboard implementation supports features including Shift, Caps Lock, Backspace, Enter, Tab, and arrow-key navigation.

The ring buffer separates interrupt-time input collection from normal shell processing.

## Real-Time Clock

`src/rtc.rs` reads the CMOS real-time clock and handles BCD or binary values and 12-hour or 24-hour modes.

It waits for updates to finish and samples the clock more than once to reduce the risk of reading inconsistent values. The `date` command displays the date and time.

## Shell

`src/main.rs` contains the kernel entry point and interactive shell.

The shell supports commands including `help`, `about`, `clear`, `echo`, `version`, `uname`, `date`, `uptime`, `breakpoint`, and `reboot`.

It also provides command history, line editing, cursor movement, and Tab completion.

## CPU Exceptions

The kernel installs handlers for CPU exception vectors 0 through 31.

The `breakpoint` command triggers the breakpoint exception, allowing the exception-handling path to be tested. Exception handlers display diagnostic information and halt execution when appropriate.

## Current Limitations

- The kernel is still experimental and intended for development and learning.
- There is no implemented user-mode process system yet.
- There is no filesystem or networking stack yet.
- VGA output uses shared mutable state and requires care as interrupt-driven features grow.
- The current shell operates directly in kernel mode.