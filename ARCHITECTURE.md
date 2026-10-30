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

`desktop_shell.rs` models Apps/taskbar input. `app_catalog.rs` discovers up to 16 application bundles or legacy loose `.BIN`/`.ELF` files in `/APPS`. `multi_window.rs` owns four RGB32 surfaces, event queues, z-order, focus/capture and damage. `app_window_ui.rs` models dragging, chrome buttons and resize proposals.

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

`app_install.rs` provides trusted-only syscall 102 using the existing 220-byte Transfer request (operation 0 install, 1 remove, no destination). It validates user memory, expected volume and paths before filesystem access; `disk_io.rs` runs it on the serialized yielding worker. `app_tree.rs` snapshots at most 64 entries, 1 MiB/file and 8 MiB aggregate, with bounded paths and explicit deepest-first removal. Installation validates matching metadata and executable format, refuses collisions or a full catalog, copies assets to `_STAGE`, then renames within `/APPS` to publish. Rollback scans the actual partial tree. Removal refuses matching running-process IDs and busy working directories, unpublishes to `_TRASH`, then removes the tree. Reserved folders are ignored by launcher/associations and never overwritten automatically. Files adds confirmed install/remove actions; shell equivalents run synchronously. Updates, signatures, downloads and cross-volume bundles remain future work.

## Desktop shell

`desktop_shell.rs` owns the bottom taskbar, two-column searchable launcher, three catalog-backed desktop shortcuts and captured sound popup. Search is bounded to 24 ASCII bytes, matches raw/friendly names, and cancels captures when results or the catalog change. Outside clicks dismiss popups and consume the matching release. The compositor determines whether the pointer is over exposed wallpaper before enabling shortcut hits, so an application covering an icon receives its own input. Task buttons retain the existing compact PID order and keyboard switching. App names/descriptions and code-drawn icons are presentation hints; executable discovery and launch permissions still use the filesystem catalog and kernel policy.

Desktop windows receive a 800x544 work area above the 56-pixel taskbar. Maximise requests a 792x512 client with four-pixel side/bottom borders; existing standalone and multitask diagnostics retain their legacy work area. Placement remains deferred until the app accepts the resize. The clock polls RTC at most once per monotonic second through a bounded snapshot routine; unreadable RTCs retain the previous display. Quick volume uses the existing system-sound volume and does not add hardware drivers. `desktop_paint.rs` draws flat chrome/icons from Canvas primitives and shared font metrics, with no image decoding or allocations in composition.

## Shared graphical text

`canvas::text` / SDK `text` compile the same allocation-free `text_layout.rs` source. The proportional Ui face derives ink bounds from the original native-grid ASCII bitmap and uses one-pixel letter spacing, four-pixel spaces and seven-pixel tabular digits. Mono retains eight-pixel advances. Width, fitting, ellipsis, hit testing and word-wrapped line iteration share those metrics. Explicit LF and overlong words always advance; all layout is bounded by the supplied bytes and pixel box. No font file parsing, filesystem work or allocation occurs during drawing.

Window draw kinds 2/3 add label/paragraph boxes while retaining the old command size and kind 1 semantics. Both foreground and scheduled paths validate every command and copy every bounded string before mutating surfaces. Damage uses the supplied box. The Canvas rasterizer clips once per glyph/row, preserves stride padding and uses a specialised fully visible path for fixed cells. Titles reserve control space and permission suffixes; I/O progress has its own bounded area. Host properties cover overflow, wrapping progress, neighbouring pixels and exact legacy glyph compatibility. QEMU compares actual user-window surfaces against host-rendered references and exercises reflow/maximise and cleanup on both display paths.

## Sampled audio

System voices use checked-in canonical WAV assets (16 kHz mono signed i16), prepared from the user's original `sounds/` recordings with trimming, peak normalization, edge fades and band-limited downsampling. `system_sounds.rs` maps the nine event IDs; notification aliases Info. The controller resamples into its existing kernel DMA ring at 48 kHz stereo, filling at most two descriptors per tick without allocation or filesystem calls. System voices have no user PID/handle and do not consume stream nonces. An application open preempts a voice; events while an app owns PCM, another voice is active, during the 750 ms cooldown, or while muted are discarded without queuing. All driver and policy mutations disable interrupts. Shutdown first synchronizes storage, then deliberately takes audio for a bounded drain of the shutdown clip before powering off.

Syscall 100 allows Trusted/Media processes to request events; only Trusted may query/set the global system volume. Settings and the shell can save the volume in `/SOUND.CFG`; a bounded read restores it before the startup voice. Player's volume remains separate. Host tests cover event validation, debounce/tick rollover, sample format/length and peak bounds. The QEMU integration compares all nine actual output waveforms against the prepared recordings and checks mute, notification reuse, persisted settings, no-device operation and complete shutdown playback.

`audio.rs` matches only PCI 8086:2415 multimedia-audio devices with valid I/O BARs, stops all capture/output engines, verifies codec readiness/rate, and enables bus mastering only after installing kernel-owned DMA storage. PCI command writes use the halfword operation so status write-one-to-clear bits are not echoed. The driver uses one descriptor page plus 32 eight-KiB sample slots; user code sees neither physical pointers nor the descriptor list. `audio_model.rs` bounds ownership to 31 slots and validates CIV progression before retiring/reusing buffers. Stable CIV/PICB snapshots and final halt handling cover ring wrap and starvation/restart. FIFO/invalid progression stops the engine. Polling is bounded from IRQ0 and audio syscalls; the controller's own interrupts and capture engines remain disabled.

Calls 96..99 expose availability, exclusive open, copied PCM submission and owner-checked queue/volume/close. Media profiles add FILE_READ|AUDIO without write/launch/shell/shared-state authority. Handles do not wrap/reuse, and exit/fault/forced termination release the owner's DMA stream before private process backing is freed. DMA buffers remain kernel-owned for the device lifetime. The speaker/PIT-channel-2 ABI remains unchanged. Launchers connect AC97 and PC speaker to the audio backend with input voices disabled.

Player preloads at most 512 KiB in range-read chunks, validates bounded RIFF/fmt/data structure and supported PCM format, then performs userspace conversion to stereo i16/48 kHz. Preloading avoids gaps from repeated full-chain validation in current positional reads. UI events remain serviced between load chunks and nonblocking PCM submissions. Audio tests inspect actual QEMU WAV-backend samples, including an 880 Hz resampled source, rather than relying only on syscalls/logs. On this Windows QEMU build the test finalizes zero RIFF/data length placeholders from captured byte count without changing samples.

## Ethernet foundation

`rtl8139.rs` discovers only `10ec:8139` Ethernet devices with a valid I/O BAR, enables I/O/bus mastering without echoing PCI status bits, and keeps device IRQs masked. Five permanently owned kernel pages hold an 8 KiB receive ring with overflow space and four 2 KiB transmit slots. CPU reads/writes DMA memory through volatile operations and observes descriptor ownership before reuse. Input lengths/status are checked before copying. Ring errors trigger a bounded controller reset; a failed reset takes the link offline. Shutdown disables transmit/receive before halt.

`net_model.rs` is an allocation-free protocol state machine shared with host tests. It bounds input to 1536 bytes, output to four frames, ARP cache to four expiring entries, and ping to one four-packet job. It validates Ethernet recipients, IPv4 lengths/checksums, fragment flags, UDP lengths/checksums, DHCP identity/options/state, and ICMP identifiers/sequences/data/checksums. IPv4 fragments, VLANs, DHCP option overload and unrelated traffic are rejected. DHCP learns address/mask/router/DNS/server/lease, renews via unicast after ARP resolution, rebinds via broadcast, and stops using expired leases. Clock comparisons survive tick wrap; exceptionally long leases are conservatively shortened to seven days.

`network.rs` polls at most once per 100 Hz monotonic tick, receives at most eight packets and drains at most four outgoing frames per call. Physical-shell, scheduler and syscall host contexts service it; no network parsing or printing occurs in IRQ handlers. Background ping results retain the originating PTY ID, check that it still exists, and temporarily route output there. Closing Terminal cannot leave a dangling session pointer. Numeric `ping` and `net` use existing trusted shell controls; no application network syscalls or new access grants are exposed.

References: [RTL8139 reference implementation](https://github.com/qemu/qemu/blob/master/hw/net/rtl8139.c), [DHCP RFC 2131](https://www.rfc-editor.org/rfc/rfc2131), [DHCP options RFC 2132](https://www.rfc-editor.org/rfc/rfc2132), [IPv4 RFC 791](https://www.rfc-editor.org/rfc/rfc791), [ICMP RFC 792](https://www.rfc-editor.org/rfc/rfc792), [UDP RFC 768](https://www.rfc-editor.org/rfc/rfc768). TCP, DNS resolver, downloads, socket permissions/quotas and additional hardware remain future work.

Networking is disabled on every boot. The developer-only command `net experimental-enable` enables it for the current boot; `net off` disables it again. These commands are omitted from help/completion and have no Settings control or saved enable flag. Disabled adapters have RX/TX, interrupts and PCI bus mastering off; no DHCP or packet parsing runs until enabled.

## Files drag and drop

Syscall 103 registers a receiving window (operation 0, zero pointer/size), begins a drag (1, 3468-byte FileClipboard input), consumes its payload (2, matching version token in a writable FileClipboard), or finishes delivery (3, zero pointer/size). Beginning requires the caller's registered window to own live left-button capture; the kernel validates and normalizes a snapshot of 1–32 non-overlapping absolute paths with the mounted volume epoch. Only the topmost visible registered Files list area can receive a drop, and only that owner can consume the token, once. No file operations run in the compositor.

Window events 6/7/8 report hover/drop/end using the existing fixed-size Event ABI. Hover events coalesce without replacing drop/end. The compositor intercepts an active drag before chrome/taskbar handling, draws a mode badge and receiving outline, and cancels on Escape, lost input, owner removal or desktop teardown. Alt+F4 and force-close first cancel capture. Files interprets folder rows, checks same-parent no-ops, and passes absolute destinations to the existing queued transfer syscall. Move payloads never update the shared clipboard. Successful completion or cancellation releases delivery and refreshes both windows. Source/target exit cannot leave references into a process address space: payloads are copied kernel values, not user pointers.

Drop operation 4 registers the whole current client as an open-file target; geometry follows resizing. Read-only/Media may use operations 2, 3 and 4, but cannot register a Files list source (0) or begin a drag (1). The SDK consumes a single regular-file payload and checks its epoch before returning it. Notepad carries the dropped path/volume through its existing unsaved-change Action flow and retains the document on failed/missing/stale opens. Player uses its existing volume-checked streaming loader under the Media profile. Open-file targets never perform filesystem moves.

Desktop file icons are bounded snapshots of `/DESKTOP` (20 entries, folders first). Refresh occurs at most once per second, never while the serialized filesystem worker is active or a drag/captured desktop gesture is pending. A volume epoch change clears stale icons. Desktop drag sources use kernel-copied canonical absolute paths and the normal single-use drag token; no clipboard or new write syscall is exposed.

Wallpaper/folder drops enqueue a Files launch with an explicit `--folder` argument. Only that menu launch's resulting PID can receive the deferred payload; load failure/cancellation or receiver exit cancels it. Registration delivers a current-folder DROP event, and Files performs existing epoch-checked transfer/collision handling through the disk worker. Failed folder navigation exits before registration. Opening ordinary desktop documents delegates association lookup to Files, which exits after successfully queuing the associated app. Apps keep their existing launch profiles. No executable auto-run is added.
