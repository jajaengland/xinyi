# Architecture

Xinyi is a freestanding 32-bit x86 kernel written in Rust and NASM. It has no standard library or general kernel heap. Protected applications can use a bounded private heap.

## Build and boot

`build.rs` assembles boot/interrupt code and BIOS loaders, then builds the applications listed in `programs/apps.txt`. Applications are separate FAT16 `/APPS` files. `tools/build_release.py` builds and verifies an installed image in QEMU; `tools/build_usb.py` packages and checks its BIOS USB boot path.

Fresh installations reserve the first 2048 sectors for boot code and the kernel:

| Location | Contents |
| --- | --- |
| LBA 0 | MBR and partition table |
| LBA 1-32 | Stage-2 loader |
| LBA 63 | Kernel header and checksum |
| LBA 64 onward | Initialized kernel payload |
| LBA 2048 onward | FAT16 partition |

Stage 2 validates the payload and available RAM, collects the memory map and firmware boot-drive information, then enters protected mode. `boot.asm` preserves a pristine kernel snapshot for installation and establishes the supervisor stack. Direct Multiboot loading remains available for diagnostics but lacks the BIOS handoff used by firmware storage/video.

`multiboot.rs`, `gdt.rs` and `interrupts.rs` initialize memory discovery, GDT/TSS, IDT, PIC and the 100 Hz PIT.

## Memory and executable loading

`phys.rs` allocates owned 4 KiB frames from usable boot-map ranges, excluding reserved memory, boot structures and the kernel. Kernel memory uses supervisor identity mappings.

`elf.rs` validates static ELF32/i386 ET_EXEC images. It rejects dynamic linking, interpreters, TLS, overlapping segment pages, RWX declarations, invalid entry points and out-of-range segments. `loader.rs` owns backing frames, copies initialized data and zeros BSS. Raw flat binaries remain supported. File size is capped at 64 KiB.

`paging.rs` constructs private address spaces:

| User region | Purpose |
| --- | --- |
| `0x40000000..0x40100000` | ELF load segments with per-segment write permissions; flat read-only code uses at most the first 64 KiB |
| `0x40100000..0x40200000` | Private heap, initially unmapped |
| `0x403fb000..0x403fc000` | Unmapped stack guard |
| `0x403fc000..0x40400000` | Writable 16 KiB stack |

Each address space owns a metadata page containing its layout, heap break and backing-frame records. Call 87 allocates pages before publishing mappings, rolls back on exhaustion, and unmaps before freeing on shrink. The SDK allocator splits/coalesces free blocks and retains pages until shrink or exit. `user_memory.rs` checks complete syscall buffers against the active layout, permissions and exact heap break. Exit/fault recovery reclaims the process's frames.

Classic non-PAE paging has no NX bit: code is protected from writes, but writable data, heap and stacks remain executable. Kernel interrupt and supervisor stacks are 64 KiB; size guards keep scheduler records bounded.

## Processes, terminals and input

`multitask.rs` manages eight job slots: up to four root application windows and their attached text children. Jobs retain arguments, cwd, private CR3, stacks, CPU context and x87/MMX state. PIT interrupts preempt user code; sleep, yield and input waits return to the dispatcher. Kernel calls run to completion, so disk I/O can pause other applications. SSE/AVX and user threads are unsupported.

A windowless desktop app's first console request opens Terminal and attaches the existing process to its PTY. Graphical children launched from Terminal detach into a free root slot. The physical console retains a synchronous launch path through `task.rs`/`window_api.rs`. Legacy `spawn`/`kill` task records are separate from desktop jobs.

`shell.rs` supplies the physical console and PTYs with the same command dispatcher, line editing, history and completion. `vga.rs` routes output to the current console. `pty.rs` owns session/input/selection state; `pty_model.rs` maintains an 80x25 text-program screen, 64 history rows and basic ANSI cursor/erase/colour handling. The Terminal app displays 25-33 rows, adding history above the compatible text screen, with a scrollbar outside its 80 columns.

`ps2.rs`, `keyboard.rs` and `mouse.rs` handle IRQ1/IRQ12 input through bounded queues. Shared sequence ordering preserves keyboard/mouse capture order; overflow/reset handling clears stale input state. Native USB input is not implemented.

## Desktop and rendering

`desktop_shell.rs` models Apps/taskbar input. `app_catalog.rs` discovers up to 16 regular `.BIN`/`.ELF` files in `/APPS`. `multi_window.rs` owns four RGB32 surfaces, event queues, z-order, focus/capture and damage. `app_window_ui.rs` models dragging, chrome buttons and resize proposals.

Windows stay below the top taskbar but may move partly off-screen. Composition clips to screen/source bounds. Minimized apps retain their state and continue executing; hidden updates do not present. Taskbar activation restores visibility. Close is an application request so editors can prompt for unsaved changes.

Resizable apps advertise limits through call 85. Grip dragging previews an outline; release proposes RESIZE. Call 86 accepts only the latest matching size and allocates/copies before replacing the surface. Failure preserves the old geometry. Maximize/restore uses the same handshake, remembers the normal rectangle and fills the frame below the taskbar, centering a client within the app's layout limits. Same-size placement changes reuse the surface. Minimize preserves maximized state; title/grip dragging is disabled while maximized. Legacy and synchronous foreground windows remain fixed-size.

`canvas.rs` draws into RAM; `graphics.rs` presents an 800x600 scene through QEMU standard VGA or validated BIOS VESA RGB565/24/32-bit framebuffers. Damage-based span copies and cursor protection avoid unnecessary full-screen redraws. GUI text uses `ui_font.rs`; Terminal uses the captured BIOS font. There is no GPU acceleration. Leaving graphics restores text/video state and owned frames.

## Storage and applications

`diskfs.rs` provides mounted-drive selection and canonical per-job paths. `fat16.rs` checks volume geometry and bounded chains. Paths use ASCII 8.3 names with up to eight components; volumes remain below 2 GiB. `vfs.rs` supplies a small RAM backend for console use; application file calls require a mounted drive.

`ata.rs` and `pci.rs` discover compatible IDE channels and perform bounded PIO polling with LBA28/LBA48. BIOS EDD access through `bios_disk.rs`/`bios_thunk.asm` supports the boot drive, including boot USB storage, without native USB hotplug or non-boot USB support.

Legacy text calls 57/58 and shell text commands retain a 1024-byte limit. Volume-tagged whole-file calls 88-90 support 64 KiB reads/saves/size queries. Kernel frame buffers stage data before copying successful reads to users or writing saves to disk. New saves publish entries after writing data; replacement writes a new chain before publishing metadata and freeing the old chain. FAT mirrors, flushes and readback checks detect failures, but there is no journal or power-loss rollback. Firmware cache-flush durability is not guaranteed.

`installer.rs` previews/revalidates destinations and stages source applications. It publishes bootability after writing the payload, preserving compatible FAT16 layouts or formatting incompatible ones. Installation and `format` require explicit confirmation and are not power-loss safe.

Files supports shared selections through `file_clipboard.rs`, with version/volume checks. Recursive transfers preflight up to 256 entries at depth eight, refuse overwrites and delete move sources only after copying the tree. Errors can leave partial destinations or independent copies. In-use directories cannot be moved/removed.

Notepad is an ELF app stored as `NOTEPAD.BIN` for filename compatibility. Its heap-backed model supports 64 KiB ASCII documents, four undo/redo snapshots, saved-content comparison and indexed visual rows. Open validates a replacement before changing the document; failed opens/saves retain it. EDIT keeps its fixed 1 KiB model. The shared text clipboard is also limited to 1 KiB.

`sdk/widgets.rs`, `gui.rs` and `file_dialog.rs` provide shared controls and Open/Save browsing without changing cwd. `theme.rs` loads validated desktop/title colours from `/GUI.CFG`; Settings can apply or persist them. Detailed syscall layouts and SDK usage belong in the [SDK reference](sdk/README.md).

## Shutdown and verification

`power.rs` validates bounded ACPI RSDT/FADT/DSDT tables and recognizes static legacy S5 values without executing arbitrary AML. Shutdown synchronizes the mounted backend, cancels on flush failure and requires confirmation when desktop jobs are running. Unsupported firmware uses a text-mode halted fallback.

Host Rust tests cover storage, documents, memory validation, queues and UI models. Disposable QEMU tests cover BIOS/USB installation and persistence, process isolation/faults, ELF/heap allocation, terminal handoff, file operations, resizing/maximize and frame reclamation. See `tools/build_release.py` for integration checks. Emulator results do not guarantee compatibility with every firmware/controller.
