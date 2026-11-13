# Architecture

Xinyi is a freestanding 32-bit x86 kernel written in Rust and NASM. It has no standard library or general kernel heap. Protected applications can use a bounded private heap.

## Build and boot

`build.rs` assembles boot/interrupt code and BIOS loaders, then builds the applications listed in `programs/apps.txt`. Applications are separate FAT16 `/APPS/ID` bundles. `tools/build_release.py` builds and verifies an installed image in QEMU; `tools/build_usb.py` packages and checks its BIOS USB boot path.

Fresh installations reserve the first 2048 sectors for boot code and the kernel:

| Location | Contents |
| --- | --- |
| LBA 0 | MBR and partition table |
| LBA 1-32 | Stage-2 loader |
| LBA 63 | Kernel header and checksum |
| LBA 64 onward | Initialized kernel payload |
| LBA 2048 onward | FAT16 partition |

Stage 1 first reads stage 2 in one BIOS EDD transfer. If the bulk read fails after three attempts, it reloads all 32 sectors individually from LBA 1. Both stages reset the boot drive between failed read attempts and restore the requested transfer count before retrying. Stage 1 reports missing EDD support separately from a stage-2 read failure.

Before loading the kernel above 1 MiB, stage 2 verifies A20 and tries the BIOS service and fast gate if needed. It accepts legacy and extended E820 entries, ignores disabled or empty ranges, and bounds the number of calls. A BIOS that signals the end with carry set after returning records is accepted if the resulting map still covers the kernel.

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

`desktop_shell.rs` models Apps/taskbar input. `app_catalog.rs` discovers up to 64 application bundles or legacy loose `.BIN`/`.ELF` files in `/APPS`. `multi_window.rs` owns four RGB32 surfaces, event queues, z-order, focus/capture and damage. `app_window_ui.rs` models dragging, chrome buttons and resize proposals.

Root task slots compact in their existing order when a process exits. Remaining task buttons shift left without moving their windows or changing PIDs; new roots append at the right. Captured taskbar clicks are canceled if the button mapping changes.

Global keyboard shortcuts are handled before application key queues. Alt+Tab and Shift+Alt+Tab cycle in stable taskbar order, restore minimized windows and cancel pointer capture. Alt+F4 queues CLOSE rather than forcing termination. Ctrl+Esc and Apps menu navigation consume their keys without forwarding them to the focused app.

Windows use the work area above the bottom taskbar and may move partly off-screen. Composition clips to screen/source bounds. Minimized apps retain their state and continue executing; hidden updates do not present. Taskbar activation restores visibility. Close is an application request so editors can prompt for unsaved changes.

Resizable apps advertise limits through call 85. Grip dragging previews an outline; release proposes RESIZE. Call 86 accepts only the latest matching size and allocates/copies before replacing the surface. Failure preserves the old geometry. Maximize/restore uses the same handshake, remembers the normal rectangle and fills the frame above the taskbar, centering a client within the app's layout limits. At the default resolution, maximized resizable apps use up to 792x512 clients with four-pixel side/bottom borders and a compact title bar. Higher-resolution frames respect each app's advertised layout limits instead of surrounding a capped client with large borders. Same-size placement changes reuse the surface. Minimize preserves maximized state; title/grip dragging is disabled while maximized. Legacy and synchronous foreground windows remain fixed-size.

`canvas.rs` draws into RAM; `graphics.rs` presents the selected desktop scene (800x600, 1024x768, 1280x720 or 1280x1024) through QEMU standard VGA or validated BIOS VESA RGB565/24/32-bit framebuffers. Damage-based span copies and cursor protection avoid unnecessary full-screen redraws. GUI text uses `ui_font.rs`; Terminal uses the captured BIOS font. There is no GPU acceleration. Leaving graphics restores text/video state and owned frames.

## Storage and applications

`diskfs.rs` provides mounted-drive selection and canonical per-job paths. `fat16.rs` checks volume geometry and bounded chains. Paths use ASCII 8.3 names with up to eight components; volumes remain below 2 GiB. `vfs.rs` supplies a small RAM backend for console use; application file calls require a mounted drive.

`ata.rs` and `pci.rs` discover compatible IDE channels and perform bounded PIO polling with LBA28/LBA48. BIOS EDD access through `bios_disk.rs`/`bios_thunk.asm` supports the boot drive, including boot USB storage, without native USB hotplug or non-boot USB support.

`ahci_check.rs` provides native SATA DMA reads, writes and cache flushes for supported disks with 512-byte logical sectors. It borrows an idle firmware-configured port, uses kernel-owned DMA buffers and restores the firmware's registers after each operation. The installer records the controller and port in the boot header. On a subsequent BIOS boot, the kernel checks the native disk against the firmware boot disk before switching the mounted volume to AHCI. `tools/ahci_install_qemu.py` exercises installation from USB, cancellation, SATA-only boot and native file persistence.

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


## Positional file I/O

`stream_protocol.rs` defines calls 91..94: bounded range reads/writes, size and resize. `fat16_stream.rs` shares complete FAT chain validation and uses bounded cluster arrays (2048 entries) instead of a whole-file allocation. Files are capped at 1 MiB; syscall scratch is capped at 16 KiB. Reads copy to userspace only on success. Writes and resizing create a separate chain, preserve unchanged bytes, zero-fill new space, flush before publishing the directory entry, then reclaim the old chain. Each edit copies the entire result and requires its free disk space. This deliberately preserves existing write ordering rather than providing an efficient append implementation or journaling. Requests join the serialized desktop I/O worker and retain pointer, process-space and volume-generation checks. There are no persistent file handles or extra cleanup state on process death. Legacy APIs/whole-file application limits and permissions are unchanged. Viewer provides read-only page navigation through the new API; it does not enforce a security boundary.


## Kernel-owned access profiles

`permissions.rs` defines Trusted, ReadOnly and Isolated launch profiles and a fail-closed allowlist for restricted syscalls. LoadedProgram owns the selected profile; RunState copies it on foreground entry, each scheduled quantum and each worker address-space installation. The syscall dispatcher rejects forbidden calls before multitask dispatch/argument handling and the worker rechecks its owner's profile. This covers both legacy and positional filesystem APIs. Restricted profiles deny shell/PTY control, child launch, shared clipboards, transfers, file mutations and global theme changes; Isolated additionally denies file reading/listing. Basic own-process/window/console and bounded sound/time/memory remain available. Call 95 only queries the capability mask. No raw user input changes a running profile.

The physical/PTY shell parses `run --read-only` or `run --isolated` and passes the profile into process creation without leaking the mode flag into app arguments. Viewer launches conventionally as read-only even through the Apps menu; filename matching can only reduce privilege. Root app launches otherwise retain Trusted compatibility behavior. This is not an executable authenticity mechanism, a per-file capability system or protection from kernel bugs/resource exhaustion. `ps` and kernel-appended window title suffixes expose the selected profile. Restricted applications cannot create PTYs or launch trusted children to escape the policy. Automatically created console wrappers are separate trusted UI processes whose PTY controls remain owner-checked and unavailable to the restricted child.


## Larger ELF images

`program_image.rs` validates executable format and file caps before the loader allocates image backing: flat images remain at 64 KiB and static ELF files may reach 1 MiB. Existing ELF virtual-region, segment-count, page-separation, writable-executable and entry checks remain in `elf.rs`; image/heap/stack address limits do not change. Both foreground and scheduled loaders stage up to 1 MiB and release staging before running. Desktop image reads now yield on the shared worker; physical-console/bootstrap loading remains synchronous.

`fat16::read_image` validates a chain of up to 2048 clusters and copies into the bounded kernel staging buffer. `save_image` reuses copy-on-write range-edit ordering for whole image replacement. These paths serve loaders/installers only; old userspace whole-file calls do not grow. The packager, image seeder and installer accept 1 MiB ELF files; the installer stages source apps before changing destination and preserves the existing confirmation flow. BigDemo supplies a real large asset plus independent writable globals/BSS/heap and defaults to the isolated profile.


## Desktop executable worker

`disk_io::Worker` handles either a user syscall or a kernel image read, never both simultaneously. Kernel loads run with the supervisor address space and no active user RunState; syscall work continues to install its owner's immutable address space/profile. Each suspended loader stack owns its staging Frames and local path. Its completed LoadedProgram is moved through a single result slot, consumed before the next operation starts. No scheduler Job reference is held across a loader switch. Completed images are fully parsed before publication; page-table creation remains bounded synchronous work.

`PendingLoad` owns snapshots for Apps, command and console-wrapper requests. The scheduler rechecks source generation, parent liveness, cancellation and capacity before constructing/publishing the Job. Console wrappers retain their child's root slot only on success. Existing Apps/command queue bounds remain and selection alternates loads with waiting app syscalls. Closing a source Terminal or using Ctrl+Alt+Esc requests cancellation; the next FAT read boundary returns a distinct Cancelled error without detaching the mounted volume. Completed unpublished images are dropped on cancellation as well. Device operations are never abandoned while firmware/controller state is active. Pure `load_model.rs` decisions and throttled QEMU integration exercise success/refusal/lifetime cases.


## Application metadata and associations

`app_manifest.rs` is an allocation-free, 512-byte ASCII parser shared by kernel and SDK. A sidecar binds exactly to its sibling BIN/ELF name. The catalog stores copied display fields and icon IDs instead of hardcoded app names. Missing metadata preserves legacy behavior; invalid present metadata prevents normal execution. `read_executable_resolved` reports the canonical path actually read, so the loader applies the correct sibling policy after current-directory/APPS search. `job_from_program` and physical-console launch intersect the loaded policy rather than replacing it.

Files uses normal queued, volume-checked SDK reads to scan handlers, keeping filesystem work in the existing worker. The selected handler's restrictive profile is carried in its launch command as well as reread at load time. Syscall 101 validates the expected volume while capturing the launch; final publication retains the existing epoch/parent/cancellation checks. Unknown or duplicate handlers do not launch. BIN/ELF opening remains explicit. The association scan bounds executable candidates to 16 and directory entries to 64. Whole-system installation stages up to 64 app-tree entries before changing volumes, then recreates folders and files in parent-first order.

`app_install.rs` provides trusted-only syscall 102 using the existing 220-byte Transfer request (operation 0 install, 1 remove, 2 update, 3 rollback, 4 finalize, no destination). It validates user memory, expected volume and paths before filesystem access; `disk_io.rs` runs it on the serialized yielding worker. `app_tree.rs` snapshots at most 64 entries, 1 MiB/file and 8 MiB aggregate, with bounded paths and explicit deepest-first removal. Installation validates matching metadata and executable format, refuses collisions or a full catalog, copies assets to `_STAGE`, then renames within `/APPS` to publish. Rollback scans the actual partial tree. Removal refuses matching running-process IDs and busy working directories, unpublishes to `_TRASH`, then removes the tree. Reserved folders are ignored by launcher/associations and never overwritten automatically. Files adds confirmed install/remove actions; shell equivalents run synchronously. Updates stage and revalidate complete bundles, require increasing numeric versions and no capability expansion, and retain `_BACKUP` with an app-bound `_UPD.ID` record until rollback/finalize. Rollback validates the old image before replacing the current one. This uses ordered renames, not a crash-atomic journal. Signatures, downloads and cross-volume bundles remain future work.

## Desktop shell

`desktop_shell.rs` owns the bottom taskbar, two-column searchable launcher, three catalog-backed desktop shortcuts and captured sound popup. Search is bounded to 24 ASCII bytes, matches raw/friendly names, and cancels captures when results or the catalog change. Outside clicks dismiss popups and consume the matching release. The compositor determines whether the pointer is over exposed wallpaper before enabling shortcut hits, so an application covering an icon receives its own input. Task buttons retain the existing compact PID order and keyboard switching. App names/descriptions and code-drawn icons are presentation hints; executable discovery and launch permissions still use the filesystem catalog and kernel policy.

Desktop geometry comes from `display_layout.rs`. The default work area is 800x544 above a 56-pixel taskbar; higher modes align the work height to 16 pixels. At 1024x768 the work area is 1024x704 and a fully resizable client can reach 1016x672; at 1280x1024 these become 1280x960 and 1272x928. Files and Image advertise the expanded limits; other apps retain their layout caps. Existing standalone and multitask diagnostics retain their legacy work area. Placement remains deferred until the app accepts the resize. The clock polls RTC at most once per monotonic second through a bounded snapshot routine; unreadable RTCs retain the previous display. Quick volume uses the existing system-sound volume and does not add hardware drivers. `desktop_paint.rs` draws flat chrome/icons from Canvas primitives and shared font metrics, with no image decoding or allocations in composition.

## Shared graphical text

`canvas::text` / SDK `text` compile the same allocation-free `text_layout.rs` source. The proportional Ui face derives ink bounds from the original native-grid ASCII bitmap and uses one-pixel letter spacing, four-pixel spaces and seven-pixel tabular digits. Mono retains eight-pixel advances. Width, fitting, ellipsis, hit testing and word-wrapped line iteration share those metrics. Explicit LF and overlong words always advance; all layout is bounded by the supplied bytes and pixel box. No font file parsing, filesystem work or allocation occurs during drawing.

Window draw kinds 2/3 add label/paragraph boxes while retaining the old command size and kind 1 semantics. Both foreground and scheduled paths validate every command and copy every bounded string before mutating surfaces. Damage uses the supplied box. The Canvas rasterizer clips once per glyph/row, preserves stride padding and uses a specialised fully visible path for fixed cells. Titles reserve control space and permission suffixes; I/O progress has its own bounded area. Host properties cover overflow, wrapping progress, neighbouring pixels and exact legacy glyph compatibility. QEMU compares actual user-window surfaces against host-rendered references and exercises reflow/maximise and cleanup on both display paths.

## Sampled audio

System voices use checked-in canonical WAV assets (16 kHz mono signed i16), prepared from the user's original `sounds/` recordings with trimming, peak normalization, edge fades and band-limited downsampling. `system_sounds.rs` maps the nine event IDs; notification aliases Info. The controller resamples into its existing kernel DMA ring at 48 kHz stereo, filling at most two descriptors per tick without allocation or filesystem calls. System voices have no user PID/handle and do not consume stream nonces. An application open preempts a voice; events while an app owns PCM, another voice is active, during the 750 ms cooldown, or while muted are discarded without queuing. All driver and policy mutations disable interrupts. Shutdown first synchronizes storage, then deliberately takes audio for a bounded drain of the shutdown clip before powering off.

Syscall 100 allows Trusted/Media processes to request events; only Trusted may query/set the global system volume. Settings and the shell can save the volume in `/SOUND.CFG`; a bounded read restores it before the startup voice. Player's volume remains separate. Host tests cover event validation, debounce/tick rollover, sample format/length and peak bounds. The QEMU integration compares all nine actual output waveforms against the prepared recordings and checks mute, notification reuse, persisted settings, no-device operation and complete shutdown playback.

`audio.rs` matches Intel ICH/ICH0/ICH2/ICH3 multimedia-audio devices (8086:2415, 2425, 2445 and 2485) with valid I/O BARs, stops all capture/output engines, verifies codec readiness/rate, and enables bus mastering only after installing kernel-owned DMA storage. PCI command writes use the halfword operation so status write-one-to-clear bits are not echoed. The driver uses one descriptor page plus 32 eight-KiB sample slots; user code sees neither physical pointers nor the descriptor list. `audio_model.rs` bounds ownership to 31 slots and validates CIV progression before retiring/reusing buffers. Stable CIV/PICB snapshots and final halt handling cover ring wrap and starvation/restart. FIFO/invalid progression stops the engine. Polling is bounded from IRQ0 and audio syscalls; the controller's own interrupts and capture engines remain disabled.

If AC97 does not initialize, the PCM service tries Intel HD Audio IDs 8086:2668, 293e and 9d71. It configures an analog speaker or line-out route at boot and reports the codec and chosen route through audio probe. The same exclusive owner and system sound policies apply to both drivers. HD Audio uses a fixed cyclic DMA buffer in kernel-owned frames. IRQ0 copies from the software queue into completed periods, applies volume in software, and accounts for samples staged in DMA until they play. A late or invalid position sample stops the stream instead of refilling uncertain periods. The 32-bit PCI register window, stereo 48 kHz PCM, and dedicated output stream are required.

Calls 96..99 expose availability, exclusive open, copied PCM submission and owner-checked queue/volume/close. Media profiles add FILE_READ|AUDIO without write/launch/shell/shared-state authority. Handles do not wrap/reuse, and exit/fault/forced termination release the owner's DMA stream before private process backing is freed. DMA buffers remain kernel-owned for the device lifetime. The speaker/PIT-channel-2 ABI remains unchanged. Launchers connect AC97 and PC speaker to the audio backend with input voices disabled.

Player embeds the unchanged `assets/player/dndt.wav` recording as its built-in selection; the generated four-note tune has been removed. It uses the same PCM decoding/resampling path for the recording and external WAVs. Player preloads at most 512 KiB in range-read chunks, validates bounded RIFF/fmt/data structure and supported PCM format, then performs userspace conversion to stereo i16/48 kHz. Preloading avoids gaps from repeated full-chain validation in current positional reads. UI events remain serviced between load chunks and nonblocking PCM submissions. Audio tests inspect actual QEMU WAV-backend samples, including an 880 Hz resampled source, rather than relying only on syscalls/logs. On this Windows QEMU build the test finalizes zero RIFF/data length placeholders from captured byte count without changing samples.

## Ethernet foundation

`rtl8139.rs` discovers only `10ec:8139` Ethernet devices with a valid I/O BAR, enables I/O/bus mastering without echoing PCI status bits, and keeps device IRQs masked. Five permanently owned kernel pages hold an 8 KiB receive ring with overflow space and four 2 KiB transmit slots. CPU reads/writes DMA memory through volatile operations and observes descriptor ownership before reuse. Input lengths/status are checked before copying. Ring errors trigger a bounded controller reset; a failed reset takes the link offline. Shutdown disables transmit/receive before halt.

`net_model.rs` is an allocation-free protocol state machine shared with host tests. It bounds input to 1536 bytes, output to four frames, ARP cache to four expiring entries, and ping to one four-packet job. It validates Ethernet recipients, IPv4 lengths/checksums, fragment flags, UDP lengths/checksums, DHCP identity/options/state, and ICMP identifiers/sequences/data/checksums. IPv4 fragments, VLANs, DHCP option overload and unrelated traffic are rejected. DHCP learns address/mask/router/DNS/server/lease, renews via unicast after ARP resolution, rebinds via broadcast, and stops using expired leases. Clock comparisons survive tick wrap; exceptionally long leases are conservatively shortened to seven days.

`network.rs` polls at most once per 100 Hz monotonic tick, receives at most eight packets and drains at most four outgoing frames per call. Physical-shell, scheduler and syscall host contexts service it; no network parsing or printing occurs in IRQ handlers. Background ping results retain the originating PTY ID, check that it still exists, and temporarily route output there. Closing Terminal cannot leave a dangling session pointer. Numeric `ping` and `net` use existing trusted shell controls; no application network syscalls or new access grants are exposed.

References: [RTL8139 reference implementation](https://github.com/qemu/qemu/blob/master/hw/net/rtl8139.c), [DHCP RFC 2131](https://www.rfc-editor.org/rfc/rfc2131), [DHCP options RFC 2132](https://www.rfc-editor.org/rfc/rfc2132), [IPv4 RFC 791](https://www.rfc-editor.org/rfc/rfc791), [ICMP RFC 792](https://www.rfc-editor.org/rfc/rfc792), [UDP RFC 768](https://www.rfc-editor.org/rfc/rfc768). TCP, DNS resolver, downloads, socket permissions/quotas and additional hardware remain future work.

Networking is disabled on every boot. The developer-only command `net experimental-enable` enables it for the current boot; `net off` disables it again. These commands are omitted from help/completion and have no Settings control or saved enable flag. Disabled adapters have RX/TX, interrupts and PCI bus mastering off; no DHCP or packet parsing runs until enabled.

## Files drag and drop

Syscall 103 registers a receiving window (operation 0, zero pointer/size), begins a drag (1, 3468-byte FileClipboard input), consumes its payload (2, matching version token in a writable FileClipboard), or finishes delivery (3, zero pointer/size). Beginning requires the caller's registered window to own live left-button capture; the kernel validates and normalizes a snapshot of 1–32 non-overlapping absolute paths with the mounted volume epoch. Window drops target the topmost visible registered Files list or open-file client area; only the selected owner can consume the token, once. Wallpaper targets use the deferred Files launch described below. No file operations run in the compositor.

Window events 6/7/8 report hover/drop/end using the existing fixed-size Event ABI. Hover events coalesce without replacing drop/end. The compositor intercepts an active drag before chrome/taskbar handling, draws a mode badge and receiving outline, and cancels on Escape, lost input, owner removal or desktop teardown. Alt+F4 and force-close first cancel capture. Files interprets folder rows, checks same-parent no-ops, and passes absolute destinations to the existing queued transfer syscall. Move payloads never update the shared clipboard. Successful completion or cancellation releases delivery and refreshes both windows. Source/target exit cannot leave references into a process address space: payloads are copied kernel values, not user pointers.

Drop operation 4 registers the whole current client as an open-file target; geometry follows resizing. Read-only/Media may use operations 2, 3 and 4, but cannot register a Files list source (0) or begin a drag (1). The SDK consumes a single regular-file payload and checks its epoch before returning it. Notepad carries the dropped path/volume through its existing unsaved-change Action flow and retains the document on failed/missing/stale opens. Player uses its existing volume-checked streaming loader under the Media profile. Open-file targets never perform filesystem moves.

Desktop file icons are bounded snapshots of `/DESKTOP` (20 entries, folders first). Refresh occurs at most once per second, never while the serialized filesystem worker is active or a drag/captured desktop gesture is pending. A volume epoch change clears stale icons. Desktop drag sources use kernel-copied canonical absolute paths and the normal single-use drag token; no clipboard or new write syscall is exposed.

Wallpaper/folder drops enqueue a Files launch with an explicit `--folder` argument. Only that menu launch's resulting PID can receive the deferred payload; load failure/cancellation or receiver exit cancels it. Registration delivers a current-folder DROP event, and Files performs existing epoch-checked transfer/collision handling through the disk worker. Failed folder navigation exits before registration. Opening ordinary desktop documents delegates association lookup to Files, which exits after successfully queuing the associated app. Apps keep their existing launch profiles. No executable auto-run is added.

Desktop icon placement maps each current name/type to one of 20 unique grid slots. Directory refresh reconciles surviving entries by identity and gives new entries free slots. A desktop-source drop onto wallpaper changes this mapping without launching Files or mutating the filesystem; occupied document slots swap. Other-folder targets retain normal transfer behavior. Layouts persist in `/DESK.CFG`: XDL1 header, bounded entry count, and fixed name/type/slot records. Parsing rejects malformed lengths, duplicate names/slots and out-of-range slots before changing state. Missing/new items reconcile into free cells. The desktop saves dirty layouts at its idle one-second poll or teardown, only with matching volume epoch and no active disk worker/drag/capture. This small copy-on-write save remains synchronous kernel I/O. Failed saves are logged and play an error sound; another arrangement retries. Layouts are loaded once per volume session.

Desktop context menus use bounded three/five-row geometry clamped inside the work area. Opening requires exposed wallpaper; captured menu clicks never reach windows underneath. Directory refresh/save waits while a menu is open so item indices remain stable. Escape, lost input, volume change and task switching dismiss it. Reset marks the default grid layout dirty for the existing saved-layout path.

Desktop multi-selection is a bounded 20-bit mask. Ctrl toggles an already-selected icon on release, preserving it when a drag starts. Shift selects the visual slot range. Drag snapshots place the grabbed icon first, followed by remaining selected absolute paths, for a stable placement anchor. Group placement clamps a shared grid translation to bounds and moves unrelated occupants into vacated cells, retaining unique slots. Selection is cleared on directory identity changes and volume changes. Existing single-file open targets reject multi-item payloads.

A wallpaper/icon press explicitly clears window focus before desktop keyboard navigation. Window activation restores app keyboard routing. Grid navigation uses occupied slots and never forwards handled desktop keys into an unfocused app. Ctrl+arrows preserves the mask; Shift navigation maintains a stable range anchor; Enter queues the existing volume-tagged Files/association launch only for one selected icon. Escape clears a selection before the legacy empty-desktop exit path.

Desktop clipboard staging uses the same kernel canonicalization, volume checks and version allocator as syscall 83. No additional app capability is granted. Desktop Paste enqueues Files with a decimal clipboard version and `/DESKTOP` destination; Files checks that version before its existing paste flow starts. Clipboard moves retain conditional per-item updates and normal collision handling, separate from drag payloads.

## Recycle records

`sdk/trash.rs` composes existing volume-tagged filesystem APIs; it adds no kernel syscall or authority. Files creates a unique bounded `/TRASH/Tddddddd` directory (1024 IDs), saves an XTR1 record with ready=false, transfers the source into `ITEM` using copy-then-remove, and publishes ready=true only after transfer success. The codec bounds original paths to 105 bytes/eight FAT 8.3 components, rejects the TRASH subtree and malformed state/type fields. Allocation scans at most 1024 root entries; mkdir arbitrates racing allocations without overwriting.

The Recycle Bin app reads 16 underlying directory entries per page and requires confirmation to restore. It refuses unready/malformed records and transfers ITEM back only through the existing no-overwrite operation. After success it deletes metadata and the empty record directory; cleanup failure reports a leftover record while leaving restored data in place. Failed transfer/publication preserves the record and any destination data for manual recovery. This is not an atomic transaction or a power-loss journal; source cleanup may already have completed before a ready-record write fails. Existing transfer size/depth/busy-directory limits still apply. Selected-record permanent deletion is described below; Empty Bin remains deferred. Raw shell deletion remains available.

Desktop Recycle snapshots selected absolute paths into the existing single-use delivery transport and binds it to one queued Files PID via its launch token. Files starts in `--recycle` mode, consumes the matching payload, validates each path is an immediate child of its current directory and rechecks the volume, then presents its existing Y/N recycle confirmation. Delivery is finished before confirmation; Copy/Cut clipboard state is never changed. Load cancellation, receiver exit, stale volumes and full app slots leave originals intact.

`trash::purge` preflights a breadth-first 32-node payload plan without recursion or writes, rejects unknown record-root files and type mismatches, and checks the volume. It then commits XTR1 state 2 (purging) before deleting the plan in reverse order, children before parents. Restore only accepts state 1. State-0 unfinished recycle records and malformed metadata are refused; state-2 records may resume deletion, including metadata cleanup after ITEM was already removed. A failure preserves remaining data/marker for retry. Individual operations remain serialized, volume-tagged filesystem calls; this is not a journal or an atomic multi-file transaction. The UI requires Y after a dedicated irreversible-deletion prompt; Enter/repeated Delete does not confirm.

The desktop launcher renders 16 cards per page with full-catalogue search, page buttons, Page Up/Down and selection-following keyboard navigation. Large temporary catalogues use `app_catalog::Storage` backed by owned kernel frames. This avoids nesting full catalogue arrays on the 64 KiB supervisor stack; the stack guard remains enabled.


`pixel_protocol.rs` and `pixel_copy.rs` implement syscall 104 for bounded packed RGB tiles. Both foreground and desktop handlers check window ownership, validate/copy the request and user pixels into owned scratch frames, reject unsupported colour bits atomically, then mark the affected client region dirty. Scratch is freed before return. The foreground dirty-row mask is now u64 so valid 544-pixel clients can mark rows 32/33 without overflowing a u32 mask. `programs/image` is a read-only ELF app; it decodes bounded BI_RGB BMP files in userspace, draws fit/native tiles and receives existing file-open drops. It registers BMP associations and embeds a generated sample. Host and QEMU image tests cover malformed headers, ranges/ownership, frame balance, real framebuffer colours, associations, resizing and both standard VGA/VESA paths. No PNG/JPEG decoder or desktop wallpaper import is provided yet.

`display.rs` validates an exact eight-byte XDP1 record in `/DISPLAY.CFG` and saves only whitelisted dimensions. Preferences apply on desktop entry or reboot, with an 800x600 fallback if the requested mode is unavailable. VBE selection validates actual width, height, pitch and framebuffer extent; standard VGA uses the same shared geometry. Backbuffer allocation, compositing, damage, pointer bounds, taskbar anchors and window placement use the active dimensions. Suspend/resume retains the active mode. UI scaling is implemented as described below.


Desktop UI scaling uses 100%, 125% or 150%. `display_layout.rs` separates physical mode dimensions from logical desktop dimensions and rejects scales leaving less than 800x600 logical pixels. `display.rs` persists an exact five-byte XUS1 record in `/UISCALE.CFG`; malformed records retain 100%. A saved scale that cannot fit the actual/fallback mode temporarily uses 100%. Settings apply on the next desktop entry; diagnostic and foreground graphics use their legacy 100% mode.

Composition and app-owned surfaces remain in logical pixels. `graphics.rs` scales dirty spans at presentation, using ceil boundaries and floor source samples so adjoining regions tile the entire physical framebuffer without gaps, including fractional final cells. The existing fast span-copy path remains for 100%; scaled output packs into a bounded row buffer for RGB565/24/32-bit backends. UI fonts, BIOS Terminal glyphs, chrome, client pixels and the cursor share this mapping. Pointer input carries fractional motion between events before logical hit testing; application event coordinates and resize dimensions remain logical. Suspend/resume retains both physical mode and scale. Bitmap enlargement does not add anti-aliasing or a vector font engine.

The current desktop styling pass keeps geometry in `desktop_shell.rs` and painting in `desktop_paint.rs`. Desktop artwork follows the active logical work area and blends with the saved theme. Stepped panel corners, launcher cards, task icons and active accents use Canvas primitives and shared palette colours; no image decoding or extra composition buffers are required. This pass covers shell/window chrome; app client controls remain a separate follow-up.

Files uses `programs/files/chrome.rs` for shared command paint/hit geometry. Primary commands occupy one row, history/Places and a bounded monospace path occupy the next, while rarer item/app operations use the existing captured context menu through More. The list origin and view geometry remain unchanged; page controls move into the footer. Navigation buttons are disabled when no target exists, and Enter on a disabled menu item does not invoke it. File operations and their confirmations retain the existing backend.

Files app-management actions load bounded sibling metadata before showing a review of the app identity, version, target and service profile. `programs/files/app_review.rs` follows BIN-before-ELF selection, validates the manifest against that executable and checks the selected volume. Install/update re-read metadata before submission and refuse a changed preview. Reviews are advisory, not a new kernel authorization token or proof against a privileged concurrent writer; syscall 102 still independently validates/stages the actual bundle. Removal allows unknown metadata for broken bundles. Modal button input is handled before normal file-list release/drag logic.

`programs/appmgr` is a trusted ELF application with a heap-owned catalogue of up to 64 installed app folders. It reads bounded manifests using the shared Files review helper, presents versions/service profiles, and uses existing volume-bound launch and app-management APIs. Reserved staging folders are omitted; invalid metadata remains inspectable but is not launched by the manager. Removal is confirmed, guards self-removal in the UI and retains kernel running-app/volume checks. Install/update dialogs accept an absolute source folder or `.XAP` file. Folder manifests are reviewed without changing the manager working directory. Package review snapshots at most 768 KiB in its private heap and validates the allocation-free `app_package` reader, including CRC, canonical relative paths, parent order and manifest identity. Confirmation extracts that immutable reviewed snapshot into a newly reserved `/PKGIN/ID`, calls the existing kernel installer, and cleans up only its own extraction records. Existing `/PKGIN` leftovers are refused. No kernel syscall or privilege boundary is added; publisher authentication and power-loss atomicity are not provided. Format, limits and recovery are documented in `APP_PACKAGES.md`. Updates show installed/proposed versions and access; confirmation rereads the source manifest. This advisory review uses existing kernel validation, including version/access checks. Restore/discard controls address the owner recorded in /APPS/_UPD.ID, reflecting the single retained backup per drive. A working message is presented before the blocking management syscall. The manager does not change permissions or replace kernel policy.

The BIOS verified read-through cache now holds 32 sectors (16 KiB of sector data), remains write-through and is invalidated on the same errors/device changes as before. Disk-worker quanta charge cache hits one unit and completed physical operations sixteen units, yielding at 128 units: at most eight physical operations or 128 cache hits per slice. This avoids scheduler/chrome overhead during repeated FAT/mirror scans while retaining bounded checkpoints, write readback and publication ordering. `tools/storage_perf_qemu.py` performs identical private-image writes/listings/read verification and records monotonic guest ticks for before/after comparisons. It is a local benchmark, not a hardware throughput guarantee.

### Per-drive file-type defaults

`src/app_defaults.rs` defines a bounded table of at most 16 file-type-to-bundle-ID preferences. The SDK association resolver loads `/DEFAULTS.CFG` with the caller's volume epoch before scanning manifests. Preferences select only installed bundle IDs whose current valid manifest declares the extension; they cannot store executable paths, arguments or access profiles, and BIN/ELF associations are forbidden. Matching still launches with the manifest's restricted prefix and kernel policy intersection. A missing/incompatible preferred app returns -61 rather than silently choosing another app. With no preference, zero handlers returns -2 and multiple handlers returns -17. Invalid or oversized preference files return -8. These choices are configuration, not a new authority boundary; trusted apps may edit them through existing filesystem services.

Applications exposes Defaults/Ctrl+D for its selected bundle, shows the current saved choice as the extension is edited, checks declared support before saving, and offers Reset type. Escape cancels. Saves use the existing volume-bound verified file-save operation, not direct disk writes. The SDK reloads the table when saving so unrelated recently saved mappings are retained; simultaneous trusted writers have no compare-and-swap guarantee. Preferences persist on that drive; whole-system installation does not migrate them to another drive.

### Saved application restrictions

`app_access.rs` parses at most 64 per-bundle restrictions in `/ACCESS.CFG` (maximum 1024 bytes). It accepts only bounded bundle IDs and R (read-only), M (media) or I (isolated); no trusted grant, paths, launch arguments or arbitrary capability masks are stored. The LF ASCII header is `XINYI-ACCESS 1`, followed by records such as `NOTEPAD=R`. Duplicate/invalid records are rejected. Removing a record restores the other launch policy limits.

The common disk loader reads this table for resolved paths beneath `/APPS/ID/`, including nested helpers, and intersects the saved restriction with name and manifest restrictions. Existing caller-profile intersection still applies. Loader parse/read failures block bundle launch; temporary image frames are released through existing ownership cleanup. Configurations do not affect raw embedded programs or executables copied outside bundle directories. Trusted code can still edit the configuration or copy executables through existing write services; this is not a new isolation boundary or an authentication mechanism.

Applications exposes Permissions/Ctrl+P. Its choices cannot exceed the current manifest's declared mask; Declared removes the user restriction and does not bypass the manifest, name or caller limits. It reloads metadata before saving, retains unrelated records and checks the volume through normal SDK calls. The UI refuses to restrict its own APPMGR bundle so users retain the management interface. Changes affect subsequent launches, not existing process authority. Records persist through updates and removal/reinstallation of the same bundle ID until explicitly reset. Simultaneous trusted writers have no compare-and-swap guarantee.

A malformed `/ACCESS.CFG` blocks disk bundle launches, including Applications. Return to the built-in physical console (close desktop jobs, then leave the desktop) and use `delete /ACCESS.CFG` to reset all saved restrictions. The console's built-in file commands remain available for recovery. This format currently uses LF, not CRLF. Whole-system installation does not migrate this per-drive file to another drive.

### Wheel and shared message dialogs

The PS/2 driver negotiates IntelliMouse sample rates 200/100/80 and identifies four-byte wheel devices (IDs 3/4), retaining a queried three-byte fallback (ID 0). The packet decoder sign-extends the fourth byte's low nibble. Wheel events are never combined with motion, buttons or keyboard ordering boundaries. Window event 9 (`SCROLL`) keeps the existing 20-byte layout; `code` is a signed i32 delta carried in u32 (positive downward), and x/y are client coordinates. The compositor routes it to the top visible client under the pointer without changing focus, or to the open Apps menu for pagination. Captured drags suppress wheel routing. Text editors change the viewport, not the caret.

Shared message syscall 105 takes severity in EBX (0 information, 1 warning, 2 error), a readable text pointer in ECX, and 1..192 bytes in EDX. It validates/copies printable ASCII before selecting only the caller's own window or kernel-owned attached Terminal. It is allowed in every launch profile and grants no additional file, launch or device access. The window stores a bounded notice independently of the app surface; a lower-priority notice cannot displace an active higher-priority one. Rendering is composited after the original client, so dismissing it restores that surface without app repaint dependence. Small clients paginate fitted text. Modal pointer events require captured release on the acknowledgement button; keyboard acknowledgement and dismissal return RESET instead of passing the original input to the app. Other windows remain independent. Resize/reset/drag cleanup remain available; opening a notice cancels related drag state and drops stale queued user input while preserving resize and drag-end events. Foreground windows use the same model.

`gui::report(kind, message)` returns an empty status string after successful publication, so migrated errors and results no longer appear only at the bottom. If no modal service/window is available, it returns the original text for legacy console fallback. Applications, Files, Notepad, Player, Image, Viewer, Settings, Recycle Bin and Terminal use this service for actionable messages; text EDIT can report through its attached Terminal. Normal progress and hints remain inline. Files now uses static ELF format under its unchanged FILES.BIN name to accommodate the expanded UI beyond its old 64 KiB flat-image limit.

Apps power buttons have captured pointer confirmation and Y/Escape keyboard handling. They refuse active queued I/O/loading and use verified disk synchronization before the existing ACPI shutdown or bounded keyboard-controller restart. Confirmation explicitly warns that running applications close and unsaved work is lost; it does not silently save application documents.

Files now renders its recycle, new-file, new-folder, rename and paste-as prompts as centered, app-modal panels with captured Confirm/Cancel buttons. Dialog transitions reset button capture using the actual input state, including keyboard-opened dialogs when the pointer is already over Cancel. Recycle preflight validates the entire selection before confirmation or mutation; `/TRASH` and its contents are refused with a specific warning. Recycle Bin restore/delete confirmations request the warning clip, and actions on an empty bin produce an information dialog.

New shared notifications request the sound clip matching their severity (information/warning/error). Repainting the same active notification or submitting a lower-priority message does not replay its sound. Existing system mute, debounce and audio-owner rules still apply. The private recycle QEMU test uses an AC97 device and checks actual warning/info playback log entries, captured buttons, protected-bin data preservation and empty-bin feedback.

### Image wallpaper cache

`wallpaper_model.rs` defines a 112-byte C request (105 path bytes, u8 mode, u16 length, u32 expected volume) and checked Fit/Fill/Center source mapping. `wallpaper.rs` loads the existing bounded BMP format into temporary owned frames, builds an RGB cache for the current logical desktop work area, and publishes it only after validation and optional preference saving succeed. Image read and conversion failures retain the previous state. The new cache replaces and drops the old frames; staging frames are released on all paths. No filesystem or decoding work occurs in `desktop_paint`.

Syscall 106 supports query (0), preview image (1), save image (2), preview colour (3), and save colour (4). Query is available to every profile; changes require Trusted. It validates active-image user buffers, the exact request size, mode, bounded absolute 8.3 path, matching clear/image operation and volume epoch. Desktop changes use the serialized filesystem worker and existing deferred-owner lifetime rules. Conversion yields every 16 output rows through a CPU checkpoint without falsely counting rows as disk blocks. Final publication rechecks the volume. The existing colour and sound settings remain separate saves, not a multi-file transaction.

The compositor copies cached pixels after the desktop base colour and before shortcuts, folders and windows. The cache is gated on volume epoch and logical dimensions. A clipped Canvas blit handles image copying without exposing the framebuffer. Desktop labels use opaque dark backplates over images. `/WALL.CFG` is loaded at desktop entry and caches are released at desktop exit; missing/invalid sources fall back to the coloured background and leave the saved record intact. Whole-system installation does not migrate this per-drive preference.

`tools/build_apps.py` now lists the bundled Settings BMP in an optional `assets` array in applications.json. `seed_apps.py` expands that list and applies the existing 1 MiB data-file bound while retaining flat executable limits. Source BMP, converter, request bounds, geometry, clipping, persistence, failure recovery and frame ownership are covered by host and private QEMU tests.
