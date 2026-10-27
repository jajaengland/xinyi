# Architecture

Xinyi is a freestanding 32-bit x86 kernel. Rust supplies kernel services; NASM handles boot entry, interrupts, firmware transitions and user-context entry/return. There is no Rust standard library or kernel heap.

## Build and boot

`build.rs` assembles the boot/interrupt code and BIOS loaders, then builds the applications listed in `programs/apps.txt` as separate files. The application binaries are packaged into FAT16 `/APPS`, not embedded in the kernel. `tools/build_release.py` installs and verifies a bootable image in QEMU.

The BIOS disk layout reserves the first 2048 sectors for boot code and kernel payload:

| Location | Contents |
| --- | --- |
| LBA 0 | MBR and partition table |
| LBA 1–32 | Stage-2 loader |
| LBA 63 | Kernel header and checksum |
| LBA 64 onward | Initialized kernel payload |
| LBA 2048 onward | FAT16 partition on a fresh installation |

Stage 2 obtains the memory map, validates payload bounds/checksum and available RAM, loads the kernel and enters protected mode. It also preserves firmware boot-drive information. Direct Multiboot loading is supported for diagnostics but lacks the BIOS handoff used for firmware storage/video.

`boot.asm` saves a pristine initialized kernel snapshot for installation and establishes the supervisor stack. `multiboot.rs` parses boot memory; `gdt.rs` and `interrupts.rs` install the kernel GDT, TSS, IDT, PIC and PIT. The PIT runs at 100 Hz.

## Memory and program isolation

`phys.rs` allocates owned 4 KiB physical frames from usable boot-map ranges, excluding reserved memory, boot structures and the kernel. Kernel memory uses supervisor identity mappings. There is no general heap allocator.

`paging.rs` builds private user address spaces. Current flat programs have read-only code at `0x40000000` (up to 64 KiB), a writable 16 KiB stack at `0x403fc000..0x40400000`, and an unmapped guard at `0x403fb000`. Kernel memory, hardware mappings and physical aliases remain supervisor-only. Legacy non-PAE stacks are executable; this is not a hardened security boundary.

`loader.rs` owns program backing frames. `usermode.rs` validates active user identity/CR3, enters ring 3 and handles exit/fault recovery. Kernel interrupt stacks and the supervisor stack are 64 KiB with capacity checks. `user_buffer.rs` validates complete user ranges before copying data or applying side effects.

## Scheduling and execution

`multitask.rs` owns eight job slots: up to four root application windows and their attached text children. Each job retains its code, arguments, working directory, private CR3, user/kernel stacks, CPU context and x87/MMX state. `cpu_context.rs` matches the assembly interrupt-frame layout. SSE/AVX and writable image globals are not supported.

PIT interrupts preempt ring-3 code. Sleep, yield and input waits return to the host dispatcher. Kernel Rust frames are never suspended: syscalls and disk operations run to completion. Blocking input saves context and retries the intercepted INT after wakeup, revalidating user buffers.

A desktop app's first console request creates a Terminal root in its taskbar slot. Once that window opens its PTY, the original task attaches and retries its syscall without restarting. A graphical child launched from Terminal detaches into a free root window slot and returns the shell prompt. Failures and owner termination clean up waiting children and owned memory.

Physical-console launches retain a synchronous path through `task.rs`/`window_api.rs`. The old task-record selector is separate from the real timer scheduler; `ps` is not yet a full list of desktop processes. `windowtest` retains the legacy window/launcher diagnostic.

## Console and terminal sessions

`shell.rs` supplies the physical console and PTYs with the same line editor, eight-command history, completion and dispatcher. `vga.rs` routes output, cursor, colour and positioned drawing to the current PTY or the physical VGA console.

`pty.rs` owns bounded session storage, keyboard queues, selection, rendering caches and foreground-child lifetime. `pty_model.rs` stores an 80×25 cell screen, 64 rows of history and basic ANSI cursor/erase/16-colour handling. Terminal is an ordinary protected app in `programs/terminal_gui`.

The shared clipboard (`clipboard.rs`) holds up to 1024 ASCII bytes. Set/get validate the entire range; insufficient capacity does not produce a partial copy. Selection in Notepad is maintained by the document model; edits replace ranges with a single undo checkpoint.

## Desktop, input and rendering

`ps2.rs`, `keyboard.rs` and `mouse.rs` route IRQ1/IRQ12 input into bounded queues. Keyboard/mouse events share capture sequence ordering. Overflow/reset handling clears stale modifier and pointer-capture state. Native USB input is not implemented.

`desktop_shell.rs` models Apps/taskbar input. `app_catalog.rs` lists up to 16 regular `.BIN` files from `/APPS`, sorted by name. `multi_window.rs` owns four RGB32 client surfaces and event queues, window order, focus/capture and damage regions. `app_window_ui.rs` handles title dragging, close requests and minimizing.

Windows stay below y=24 but can extend partly beyond the other screen edges. Surface copies clip both screen and source coordinates. Minimized windows retain their surfaces/processes, lose focused input and do not present hidden updates. Taskbar activation restores the window fully into view. Close remains an application request so editors can check unsaved changes.

`canvas.rs` draws into RAM; `graphics.rs` presents an 800×600 scene using either QEMU standard VGA or validated BIOS VESA linear RGB565/24/32-bit modes. VESA mode records, masks, pitch and address ranges are checked before use. Presentation copies spans, converts packed formats when needed and protects the cursor footprint. There is no native GPU acceleration.

Text mode, font/palette/register state and owned frame allocations are restored when leaving graphics or reporting fatal diagnostics. Application GUI text uses `ui_font.rs`; terminal cells use the captured BIOS bitmap font.

## Filesystems and disks

`diskfs.rs` provides canonical per-job paths, mounted-drive selection and shared filesystem calls. Paths use ASCII 8.3 names, up to eight components. `fat16.rs` validates volume geometry and bounded directory/file chains. `vfs.rs` provides a small RAM backend for console use. Program file calls require a mounted drive.

Ordinary text-file reads/writes are limited to 1024 bytes; the separate executable reader supports 64 KiB. Replacement allocates a new chain, writes data and publishes metadata, then releases the old chain. FAT mirrors and readback checks detect failures, but there is no journal or power-loss rollback.

`ata.rs`, `pci.rs` and the ATA helper modules discover and validate compatible IDE channels, perform bounded PIO polling and support LBA28/LBA48. Full disk capacity is kept separately from the u32 accessible-sector count. FAT16 volumes remain below 2 GiB.

`bios.rs`/`bios_thunk.asm` bridge protected mode to BIOS services. `bios_disk.rs` accesses the boot drive through EDD, allowing boot USB storage without a native USB controller driver. This does not support hotplug or arbitrary non-boot USB devices. Firmware writes are synchronous and read back; firmware cache-flush durability is not guaranteed.

`installer.rs` previews and revalidates the destination, stages source applications, and requires an immediately following confirmation. Compatible FAT16 layouts can be preserved; incompatible layouts are formatted. Bootability is published after the payload is written. `format` uses a separate preview/confirmation flow. Both operations can destroy data; neither is power-loss safe.

## Applications and API

`int 0x80` uses saved-register dispatch. User ranges, handles, ownership and reserved arguments are checked before use. Calls provide console/input/timing, bounded files/arguments, windows, clipboard, PTYs and directory operations. The detailed contract is in [sdk/README.md](sdk/README.md).

Files and Notepad are separate applications. Files lists 18 rows per view using a bounded paginated directory ABI, opens text files in Notepad and executables through Terminal, and confirms deletion. Text files/clipboard and flat-image limits are shared with the rest of the system.

## Verification

Host Rust tests cover geometry/capture, queues, scheduling, document edits, storage and pointer checks. QEMU tests use disposable images and check BIOS/USB boot, installation, persistent files, isolation/fault recovery, terminal handoff, minimize/restore, clipped composition and frame reclamation. Drag benchmarks check cursor visibility and catch-up latency; they are not hardware frame-rate guarantees.

See `tools/build_release.py` for the release integration suite. Hardware support remains dependent on firmware/controller compatibility; emulator results are not a universal hardware guarantee.
