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

- [ ] Review compiler warnings and resolve those that indicate real problems.
- [ ] Review interrupt handling and ensure the assembly and Rust calling conventions remain consistent.
- [ ] Audit ring-buffer behavior, including full-buffer handling and scancode edge cases.
- [ ] Review shared mutable state, especially VGA output and interrupt interactions.
- [ ] Improve diagnostic messages for kernel faults.
- [ ] Test shell commands and keyboard behavior after changes to interrupt handling.

### 2. Shell Improvements

- [ ] Review command-line editing and history edge cases.
- [ ] Improve error messages for unknown commands.
- [ ] Consider more flexible command parsing.
- [ ] Add a command for displaying kernel diagnostic information.
- [ ] Keep shell behavior consistent as new kernel features are added.

### 3. Kernel Structure

- [ ] Separate kernel subsystems into clear modules as the project grows.
- [ ] Document important interfaces between subsystems.
- [ ] Establish consistent error-handling conventions.
- [ ] Improve build and debugging instructions.

## Application Foundations

These tasks prepare the kernel for running programs beyond its built-in shell.

- [ ] Investigate Global Descriptor Table (GDT) setup and privilege levels.
- [ ] Design a safe approach to kernel memory management.
- [ ] Investigate virtual memory and paging.
- [ ] Design a basic memory allocator when the required foundations are ready.
- [ ] Research process and thread management.
- [ ] Plan how applications will interact with kernel services.

## User-Mode Applications

A longer-term goal is to run applications with less privilege than the kernel.

- [ ] Establish user-mode execution and privilege separation.
- [ ] Design a system-call interface.
- [ ] Define an executable format and loading strategy.
- [ ] Implement basic process creation and termination.
- [ ] Investigate scheduling and multitasking.
- [ ] Provide a way to launch and manage applications from the shell.

## Filesystem and Storage

- [ ] Investigate disk access in the emulator.
- [ ] Design a block-device interface.
- [ ] Choose a simple filesystem format to support first.
- [ ] Implement basic file and directory operations.
- [ ] Add shell commands for working with files.
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
9. Provide commands appropriate for the MSYS2 UCRT64 environment on Windows.
10. Use Notepad for editing instructions when practical; do not assume a terminal editor is preferred.
11. After testing, review `git diff` before committing.
12. Keep code changes and documentation changes in separate commits when practical.

The roadmap is a guide, not a rigid sequence. Priorities can change as the kernel develops and new technical challenges are discovered.