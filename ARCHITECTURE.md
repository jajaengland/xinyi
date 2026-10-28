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

`multitask.rs` manages eight job slots: up to four root application windows and their attached text children. Jobs retain arguments, cwd, private CR3, stacks, CPU context and x87/MMX state. PIT interrupts preempt user code; sleep, yield and input waits return to the dispatcher. Desktop file calls queue into a serialized worker; other kernel calls run to completion. SSE/AVX and user threads are unsupported.

A windowless desktop app's first console request opens Terminal and attaches the existing process to its PTY. Graphical children launched from Terminal detach into a free root slot. The physical console retains a synchronous launch path through `task.rs`/`window_api.rs`. Legacy `spawn`/`kill` task records are separate from desktop jobs.

Before servicing terminal shell commands, the dispatcher publishes a bounded
copy of the live job list. Terminal `ps` reads this snapshot, including private
frame usage and scheduler states; `kill PID` validates it and queues termination
for the dispatcher. Shell commands never borrow the live job array. Physical
console `ps` / `kill` continue to operate on diagnostic task records.

`shell.rs` supplies the physical console and PTYs with the same command dispatcher, line editing, history and completion. `vga.rs` routes output to the current console. `pty.rs` owns session/input/selection state; `pty_model.rs` maintains an 80x25 text-program screen, 64 history rows and basic ANSI cursor/erase/colour handling. The Terminal app displays 25-34 rows, adding history above the compatible 80-column text screen. Its background expands with the client and the scrollbar stays at the right edge.

`monotonic.rs` derives the 100 Hz elapsed-time API from the free-running ACPI PM timer when firmware exposes a validated System I/O register. Fractional ticks and 24/32-bit wrap are accumulated under a short interrupt guard. Sampling before and after firmware calls preserves elapsed time independently of PIC/PIT reprogramming. Unsupported platforms retain the PIT fallback; the PIT fallback filters its initial programming edge without changing the established BIOS/PIC restore order. Counter displays elapsed time rather than counting scheduled redraws.

`ps2.rs`, `keyboard.rs` and `mouse.rs` handle IRQ1/IRQ12 input through bounded queues. Shared sequence ordering preserves keyboard/mouse capture order; overflow/reset handling clears stale input state. Native USB input is not implemented.

## Queued filesystem work

The BIOS backend keeps four verified read-through sectors to avoid rereading the same FAT sectors for each allocation step. Writes still reach firmware and pass readback before updating this cache; there are no dirty entries. Mount/unmount, firmware errors and native writes invalidate cached sectors, including native access that may alias the boot drive.

`disk_io.rs` owns one 128 KiB worker stack. Desktop calls 57/58, 78-81,
88-90 and Settings saves park the caller with a saved CPU context. The dispatcher
selects waiting requests round-robin and resumes the worker in the owning
process's CR3, cwd and PTY context. Each queued request retains its submission volume generation; a changed volume returns -116 before touching the filesystem. It restores the kernel context before
running another app. User buffers and address spaces remain pinned because
waiting owners cannot execute or be reclaimed.

The ATA and BIOS disk backends checkpoint after complete block transfers (and
firmware write readback), with at most eight blocks per slice. No controller or
BIOS thunk is suspended. A completed operation returns its normal syscall result
and wakes its owner. A forced stop waits for the active operation to finish;
queued work can be discarded before it starts. Filesystem operations never
reenter one another while FAT metadata updates are in flight. A completed block-operation count
in the owning window title reports activity without claiming a byte percentage.

App launches and catalog refreshes wait for the worker to become idle. Terminal
line editing and non-disk commands continue; filesystem commands retain their
order in a deferred command slot. These kernel shell commands and executable
loads remain synchronous when they eventually run. Firmware/controller timeouts
can still stall an individual transfer, and this does not add journaling or
power-loss recovery. Periodic IRQ windows keep sleepers progressing even when
BIOS transfers repeatedly reset the PIT.

## Desktop and rendering

`desktop_shell.rs` models Apps/taskbar input. `app_catalog.rs` discovers up to 16 regular `.BIN`/`.ELF` files in `/APPS`. `multi_window.rs` owns four RGB32 surfaces, event queues, z-order, focus/capture and damage. `app_window_ui.rs` models dragging, chrome buttons and resize proposals.

Root task slots compact in their existing order when a process exits. Remaining task buttons shift left without moving their windows or changing PIDs; new roots append at the right. Captured taskbar clicks are canceled if the button mapping changes.

Global keyboard shortcuts are handled before application key queues. Alt+Tab and Shift+Alt+Tab cycle in stable taskbar order, restore minimized windows and cancel pointer capture. Alt+F4 queues CLOSE rather than forcing termination. Ctrl+Esc and Apps menu navigation consume their keys without forwarding them to the focused app.

Windows stay below the top taskbar but may move partly off-screen. Composition clips to screen/source bounds. Minimized apps retain their state and continue executing; hidden updates do not present. Taskbar activation restores visibility. Close is an application request so editors can prompt for unsaved changes.

Resizable apps advertise limits through call 85. Grip dragging previews an outline; release proposes RESIZE. Call 86 accepts only the latest matching size and allocates/copies before replacing the surface. Failure preserves the old geometry. Maximize/restore uses the same handshake, remembers the normal rectangle and fills the frame below the taskbar, centering a client within the app's layout limits. Standard maximized apps use 792x544 clients with four-pixel side/bottom borders and a compact title bar. Same-size placement changes reuse the surface. Minimize preserves maximized state; title/grip dragging is disabled while maximized. Legacy and synchronous foreground windows remain fixed-size.

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

## FAT16 audit and recovery

`fat16_check.rs` shares the normal geometry validation but opens damaged FAT headers for inspection. The physical-console `fsck` scans both FAT copies and walks the complete reachable directory tree with a 384 KiB frame-backed workspace. Cluster ownership detects shared/cyclic chains; file lengths, directory dot entries, mirror entries and reserved headers are checked. Traversal is bounded to eight directory levels, 32 clusters per directory and 1,048,576 entries. A bound/read failure prevents cleanup. Scans never call block writes or flushes.

Orphan repair requires an unmounted volume, a preview and immediate one-shot confirmation. Device identity, volume epoch and a fingerprint of boot/partition metadata, both FAT copies and visited directory sectors are revalidated before writing. Only unreferenced allocated entries are zeroed. Zero-versus-allocated differences in those entries are safely resumable after interrupted cleanup; live or ambiguous differences are refused. Writes touch only changed FAT sectors, are read back and flushed, then the volume is scanned again. Bad/reserved markers and all live chain pointers, directories and data remain unchanged. This is conservative cleanup, not a journal or reconstruction of damaged files. BIOS firmware still cannot provide a portable durability guarantee.
