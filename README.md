# Xinyi

Xinyi is an experimental 32-bit x86 operating system written in Rust and x86 assembly. It includes a text shell, a small multitasking desktop, FAT16 storage and protected filesystem applications.

## Build and run

Requirements: Rust nightly with `rust-src`, Cargo, NASM, Python 3 and `qemu-system-i386` on `PATH`. The build scripts are primarily tested on Windows with MSYS2; some tools include Windows-specific QEMU defaults.

From the repository root:

```powershell
rustup component add rust-src --toolchain nightly
cargo +nightly build --release -Z build-std=core
python tools/build_release.py
```

The release builder runs QEMU checks and creates a new folder under `target/releases/` containing a bootable `xinyi.img`, the kernel, separate `/APPS` files, usage notes and checksums. `target/latest-release.json` records its location. Run the generated `Launch-Xinyi.cmd` on Windows, or use:

```text
qemu-system-i386 -m 128M -drive file=PATH/TO/xinyi.img,format=raw,if=ide,index=0 -boot c -vga std
```

Use release builds for interactive graphics. A direct kernel boot is useful for diagnostics but does not include the application filesystem:

```text
qemu-system-i386 -kernel target/i686-unknown-none/release/rust-os
```

## Desktop and applications

Apps use one folder per application: `/APPS/NOTEPAD/NOTEPAD.BIN` and `/APPS/NOTEPAD/NOTEPAD.APP`, with any assets alongside them. To install a new app, put its folder on the mounted FAT16 volume (for example `/INBOX/SAMPLE`), select it in Files, choose **Install** or right-click **Install app**, then confirm with **Y**. It appears in Apps immediately on reopening the launcher. To uninstall, select its folder inside `/APPS`, choose **Remove**, and confirm. Running apps must be closed first. Removal deletes the app folder and bundled files; documents elsewhere stay intact. **N** or Escape cancels either prompt.

Bundles use an ASCII app ID of 1–8 letters/digits/underscores beginning with a letter. The folder, executable stem, and manifest stem must match; use either `.BIN` or `.ELF` plus `.APP`. For example, `SAMPLE/SAMPLE.BIN` and `SAMPLE/SAMPLE.APP`. Installation validates metadata and executable format, preserves the source, and refuses an existing installation. Updates currently require removing the old version first. Only trusted programs may install or remove apps. The physical shell also supports `app install /INBOX/SAMPLE` and `app remove /APPS/SAMPLE`.

The initial limits are 16 launcher apps, 64 entries per app tree, 1 MiB per file, 8 MiB total bundle data, and eight path components. Installation uses a reserved `/APPS/_STAGE` folder and publishes the final name only after copying succeeds. Removal first renames the app to `/APPS/_TRASH`. After an interrupted operation these reserved folders stay hidden from Apps and are preserved for manual recovery; a later operation refuses to overwrite them. This first version installs from the currently mounted volume; it does not download apps or access a second USB volume.

The desktop has a bottom taskbar, clock, system-sound controls, and single-click Files, Terminal, Settings and Desktop shortcuts. Apps opens a searchable two-column launcher with code-drawn icons and short descriptions. Type part of an app name, use arrows/Home/End, and press Enter. Escape dismisses the launcher without reaching an app underneath. Sound popup changes are immediate; Settings Save or `sounds save` keeps them on disk. Desktop shortcuts respond only where another window is not covering them.

Type `desktop` at the shell. Apps discovers installed app folders in `/APPS`, as well as legacy loose `.BIN` and `.ELF` files. Up to four root application windows and their attached text processes can run together.

App titles, descriptions, icon choices and restrictive access profiles come from sibling `/APPS/NAME/NAME.APP` manifests, documented in [programs/manifests/README.md](programs/manifests/README.md). Missing manifests retain filename/generic-icon discovery; invalid present manifests refuse execution. Files resolves TXT/LOG/MD/CFG/INI/CSV/APP and extensionless documents to Notepad, WAV to Player, and DAT/RAW/HEX to Viewer. Unknown types report no handler; conflicting handlers report ambiguity. BIN/ELF opening keeps the explicit executable path through Terminal. The whole-system installer preserves app folders, manifests, and assets.

- **Files:** browse folders, open documents in Notepad, run programs through Terminal, create folders and confirm recycling. F5 refreshes; PageUp/PageDown changes pages. Click empty space to clear selection; arrow keys select an item again. F2 renames a file or unused folder. Ctrl+click toggles items; Shift+click selects a range; Ctrl+A selects up to 32 items. Right-click for actions. Copy/Move (Ctrl+C/X) shares the selection across Files windows, even after closing the source window; open a destination and Paste (Ctrl+V). Folders transfer recursively. Existing names offer Paste as rather than overwrite. Delete/Recycle moves selected files or folders to Recycle Bin after confirmation.
- **Settings:** choose desktop and active title-bar colours. Apply changes the current session; Save writes `/GUI.CFG` on the mounted drive for the next boot.
- **Notepad:** edit ASCII documents up to 64 KiB, with scrolling, four undo steps and unsaved-change prompts. Drag or Shift+arrows selects text; Ctrl+A/C/X/V selects all, copies, cuts and pastes. Ctrl+S saves, Ctrl+Shift+S opens Save As, Ctrl+O opens a file, Ctrl+N starts a new document, Ctrl+Z/Y undoes/redoes and Ctrl+Q quits. Open/Save As browses folders without changing the working directory.
- **Terminal:** uses the same shell as the physical console. Text apps launched from Apps automatically get a terminal window. Ctrl+C interrupts a program; Ctrl+Shift+C copies selected output; Ctrl+V pastes. Drag the right-hand scrollbar, click its arrows/track, or use Shift+PageUp/PageDown for up to 64 rows of scrollback. The text area retains all 80 columns.
- **EDIT:** text-mode file editor, also usable inside Terminal. Ctrl+S saves; Ctrl+Q quits.
- **Snake / Tetris:** text games with pause/restart; Escape quits. Snake beeps when food is collected.
- **Counter / Hello:** small application and scheduling examples.
- **MEMDEMO:** an ELF application demonstrating writable globals, zeroed BSS, and heap-backed Rust `Vec`/`Box`. Run it from Apps or with `run memdemo`.

Use the maximize/restore title-bar button or double-click the title bar in Files, Notepad and Terminal. Maximized frames fill the work area above the taskbar, respect app layout limits and restore their previous size/position. Minimize and taskbar restore preserve maximized state.

Drag the lower-right grip to resize Files or Notepad; Notepad reflows text and Files shows more rows. Terminal expands its background and right-edge scrollbar horizontally and grows vertically to display up to 32 rows when maximised, while preserving the 80-column text-program layout. Apps that do not opt in remain fixed-size.

Closing an app shifts the remaining taskbar buttons left; new apps append on the right.

Use **Alt+Tab** to cycle apps in taskbar order and **Shift+Alt+Tab** to cycle backwards, including minimized apps. **Alt+F4** requests a normal close so editors can prompt to save. **Ctrl+Esc** opens Apps; use arrows, Home/End and Enter to launch, or Escape to cancel.

During desktop file operations, the working app's window title shows `I/O` and a completed block-operation count. Other apps remain usable. Ctrl+Alt+K waits for an active filesystem operation to finish before reclaiming that process. Terminal typing and non-disk commands remain live; submitted filesystem commands wait for the worker.

Drag normal title bars to move windows, including partly off-screen. The bottom taskbar remains accessible. The **_** button minimizes without closing or pausing the program; its taskbar button restores it into view. Ctrl+Alt+K stops the focused app. Close running apps before selecting Shell to return to the console.

## Shell and storage

`help` lists commands. Common commands include:

Inside a desktop Terminal, `ps` lists live desktop processes with PID, name,
scheduler state and allocated private memory in KiB (including the kernel stack
and page tables, excluding GUI/PTY buffers). `kill PID` forcibly stops that
process; unsaved application changes are lost. Closing a Terminal also stops its
attached text program. On the physical console these commands retain their
diagnostic task-record behavior.

| Purpose | Commands |
| --- | --- |
| Browse | `pwd`, `ls [PATH]`, `cd PATH`, `mkdir PATH`, `rmdir PATH` |
| Text files | `touch FILE`, `cat FILE`, `write FILE TEXT`, `delete FILE` |
| Applications | `apps`, `run NAME [ARGS]`, `edit [FILE]`, `notepad [FILE]`, `terminal`, `settings`, `run viewer [FILE]` |
| Drives | `disks`, `use NUMBER`, `mount`, `unmount` |
| Recovery | `fsck`, `fsck repair-orphans`, `fsck confirm` |
| System | `version`, `uptime`, `date`, `reboot`, `shutdown` |

The system mounts its boot filesystem automatically. BIOS boot-drive access supports boot USB media; native disks use ATA/IDE PIO, including compatible IDE emulation. BIOS boot media appears as drive 0, with native IDE targets numbered separately. Use `disks` before selecting a target.

`install NUMBER` previews a bootable installation onto a native IDE destination and stages `/APPS` from the mounted source. Only `install confirm` immediately after the preview applies it. Compatible FAT16 layouts can be preserved; incompatible layouts are replaced after confirmation. `format` previews a whole-disk quick format; `format confirm` applies it and removes existing partitions and bootability. **Both operations can destroy data: inspect the target and preview carefully.**

`run --read-only APP [ARGS]` allows reads from the mounted volume but denies file mutations, child launches, shared clipboard access, changing desktop colours and terminal-control services that can execute shell commands. `run --isolated APP [ARGS]` also denies file reads/listing. These modes work from the console and Terminal. Viewer uses read-only access automatically; its title shows `[RO]`. `ps` reports access profiles. BigDemo uses isolated access automatically. Player uses a media profile that allows reading and PCM playback. Other unflagged applications retain trusted access for compatibility, so choose a restricted mode for unfamiliar programs. Read-only access covers the entire mounted volume, not just a selected file. These initial profiles do not provide resource quotas or protection from kernel bugs.

`fsck` checks the selected FAT16 volume from the physical console without writing or flushing disk sectors. It reports mirror differences, broken/cyclic/shared chains, file-size errors and lost allocations. A damaged mounted volume is detached to RAM. Close the desktop before scanning. To reclaim lost clusters, run `unmount`, then `fsck repair-orphans` to inspect the target and preview, followed immediately by `fsck confirm`. Any intervening command cancels confirmation. Cleanup discards data in unreferenced clusters; it refuses incomplete scans, live-chain damage and ambiguous mirrors. It preserves live file pointers, directories, boot sectors and bad-cluster markers.

`shutdown` flushes the selected disk before attempting ACPI soft-off. From the desktop terminal, save your work and use `shutdown confirm` to close the running applications. If power-off is unsupported, Xinyi restores text mode, reports that it is safe to turn off the computer, and halts. Flush failures cancel shutdown.

## USB images

```powershell
python tools/build_usb.py
```

This packages and tests the latest verified release. `target/latest-usb.json` records the raw image and checksum. Flash the image in raw/DD mode, then boot in legacy BIOS mode. Flashing replaces the selected USB drive's contents.

USB access uses BIOS services for the boot drive only. Unmount before removal. Firmware writes are read back, but there is no portable firmware cache-flush guarantee.

## Current limits

- Legacy BIOS boot; no UEFI or native USB/AHCI drivers.
- FAT16 volumes below 2 GiB, ASCII 8.3 names and paths up to eight components.
- Positional file APIs support 1 MiB files in chunks up to 16 KiB. The Viewer app displays hex/ASCII pages without loading the whole file. Copy-on-write range edits require free space for the complete replacement and become slower as files grow. Existing recursive copy/move remains limited to 64 KiB per file.
- Notepad documents: 64 KiB of ASCII text, with a scrollbar and four undo steps. Shell text commands, EDIT and the shared clipboard retain their 1024-byte limit. ELF executable files: 1 MiB; flat binaries: 64 KiB.
- Flat binaries remain supported. Static ELF32/i386 apps can use writable data/BSS (up to a 1 MiB image span) and a private heap capped at 1 MiB. ELF files can be up to 1 MiB; flat binaries retain their 64 KiB cap. No dynamic linking, TLS, user threads or Linux ABI compatibility.
- Bounded opt-in window resizing, shared GUI controls, basic bitmap fonts and 800Ã—600 framebuffer graphics; no GPU acceleration.
- Desktop app file calls are queued and yield between completed disk blocks; other apps keep running during saves and copies. Desktop executable reads share the yielding worker; individual firmware/controller calls and kernel shell file commands still run synchronously.
- Folder transfers: up to 256 entries per tree, eight path components and 64 KiB per file. Batch operations stop on failure; completed items and partial destination trees can remain.
- ACPI shutdown supports simple static legacy S5 tables; firmware needing AML methods, hardware-reduced ACPI or unsupported table layouts uses the safe halt fallback.
- No filesystem journal or general power-loss recovery. The checker can reclaim unreferenced clusters and resume interrupted orphan cleanup; it cannot reconstruct broken live files or interrupted installations. This is a learning OS, not a hardened system for important data or untrusted programs.

## Development and tests

Automated storage tests use disposable QEMU images. Host storage tests also cut writes and flushes at each operation boundary, including write-through and write-back disk models. `python tools/fsck_qemu.py --disk PATH` verifies scans and recovery on private copies of a bootable image. `python tools/build_release.py` runs the release integration suite. Individual checks include:

```powershell
python tools/smoke.py --kernel target/i686-unknown-none/release/rust-os --qemu qemu-system-i386
python tools/window_lifecycle_qemu.py --disk PATH/TO/xinyi.img --vga cirrus --usb
python tools/maximize_qemu.py --disk PATH/TO/xinyi.img --vga cirrus --usb
python tools/large_files_qemu.py --disk PATH/TO/xinyi.img --vga cirrus --usb
rustc --edition=2021 --test tools/window_tests.rs -o target/window-tests.exe
.\target\window-tests.exe
```

`graphics`, `mouse`, `windowtest` and `multitask` provide interactive diagnostics. Deliberate fault/panic probes are test tools, not normal applications.

- [Architecture](ARCHITECTURE.md)
- [Application SDK](sdk/README.md)
- [Roadmap](TODO.md)


`run bigdemo` (or BIGDEMO in Apps) demonstrates a real larger ELF application with a 128 KiB file-backed palette asset, private initialized data/BSS and a 96 KiB heap. Left/Right changes palette pages. The app runs isolated by default. Builds, foreground/desktop loading, image packaging and installation all accept ELF files up to 1 MiB. The 1 MiB virtual image span, 1 MiB heap and 16 KiB stack limits are separate and unchanged. Desktop image reads yield between completed disk operations and temporarily use a bounded 1 MiB staging buffer; physical-console/bootstrap loading remains synchronous; legacy whole-file APIs and Files copy/move keep their earlier limits.


Desktop app loading, including automatic Terminal wrappers, shares the serialized disk worker with app file requests. Existing windows keep receiving input and drawing while an image is read. Loading messages appear in the Apps launcher; Ctrl+Alt+Esc cancels the active load at a safe read boundary. No process starts until the complete image validates and its source volume, parent and available slots are rechecked. Closing a launching Terminal cancels its pending load. Only one pending Apps-menu request and one pending command request are retained; additional requests report that a launch is already queued. Individual firmware/controller calls still block, and final bounded image/page-table setup is synchronous.


## Graphics and fonts

Interface labels now use a proportional 16-pixel bitmap face with narrower letters and spaces, tabular digits, pixel-based measurement, word wrapping and clipped ellipsis. Window titles, taskbar entries, Apps labels and shared GUI buttons use it. Kernel and applications share the layout code. Terminal cells, text inputs and Notepad's editing grid retain their fixed 8x16 layout for accurate selection and cursor positioning. The rasterizer clips glyphs once and uses an in-bounds row fast path; framebuffer transfers continue to use the existing bounded span copies.

Open **FONTS** from Apps or `run fonts` to compare fixed/proportional widths, adjust a wrapping box with +/- or Space, and test maximise/restore. The current faces cover printable ASCII at native size; scalable fonts, shaping and Unicode remain future work. `tools/text_qemu.py` checks actual foreground/desktop surface pixels, invalid buffers and batches, reflow, maximise/restore and memory reclamation on standard VGA and BIOS VESA.

## Audio

System events use the voice recordings in `sounds/`. Their prepared 16 kHz mono PCM copies in `assets/sounds/` total about 142 KB (down from 1.06 MB), with excess silence trimmed, peaks balanced and short edge fades. Originals are retained; run `python tools/prepare_sounds.py` with ffmpeg available to regenerate them, then rebuild the kernel. No compressed-audio decoder is needed.

Startup/shutdown, errors, confirmation warnings, save/copy completion, empty folders and explicit filesystem mount/unmount events use the matching clips. Information and notifications share `info.wav`. Settings has system sound volume, mute and Test voice controls; Save persists the volume in `/SOUND.CFG`. The same controls are available through `sounds`, `sounds test error`, `sounds test notification`, `sounds volume 60`, `sounds mute`, `sounds unmute` and `sounds save` in Terminal/the console. The volume is separate from Player's. Voices never stack: requests during another voice, within 750 ms of the last accepted event, or while an app owns PCM are dropped. Opening app audio stops any current system voice. There is no deferred chorus after playback ends.

PLAYER in Apps provides actual sampled sound through an Intel 82801AA AC'97 device, separate from the PC speaker. Use **dndt.wav** / T for the bundled recording, **Open WAV** / Ctrl+O for a file, Space to restart, S to stop, and +/- for volume. The first player supports uncompressed 8/16-bit mono/stereo PCM WAV at 8-48 kHz, preloaded up to 512 KiB, with nearest-neighbour conversion to 48 kHz stereo. Files load in bounded chunks before playback; this avoids relying on the current range-read speed for continuous streaming. No MP3/OGG decoder or software mixer is included yet.

`run --media APP` grants file-read and PCM playback access without file writes, child launches, clipboard access or shell controls. Player gets this profile automatically. A single process owns the PCM stream; other applications receive a busy result. Closing, faulting or forcibly terminating its owner stops DMA and releases the stream. The kernel copies samples into its own bounded DMA buffers, never pointing the device at process memory. Playback and the desktop run concurrently.

`Launch-Xinyi.cmd` configures QEMU AC97 output and retains PC-speaker sound for Snake/Tetris. Audio input voices are disabled. Other hardware, including HDA/USB audio, is not supported by this first driver; absent hardware reports unavailable while the rest of the OS keeps working. The QEMU test records real PCM and checks WAV resampling, queue wraparound, ownership and process cleanup. The register implementation follows the [Intel AC'97 controller documentation](https://www.intel.com.br/content/dam/doc/manual/io-controller-hub-7-hd-audio-ac97-manual.pdf).

## Networking

Xinyi now has an initial Ethernet stack: an RTL8139 PCI driver, ARP, IPv4, DHCP with lease renewal/rebinding, and ICMP echo. After explicit enabling, it acquires an address when a supported adapter has a link. The release QEMU launcher has no network adapter by default; developers can attach RTL8139 explicitly for testing.

- `net` shows link, MAC address, DHCP configuration and packet counters.
- `net dhcp` starts a fresh DHCP request after a failed configuration or to reset the lease.
- `ping 10.0.2.2` starts four background pings to QEMU's virtual router. `ping` shows progress/results. Terminal stays usable while other apps run.

For a manual QEMU launch add `-netdev user,id=net0 -device rtl8139,netdev=net0,id=nic0`. There are no host port forwards. The initial driver supports PCI vendor/device `10ec:8139`; it does not support the HP laptop's Wi-Fi or arbitrary modern Ethernet controllers. DNS server addresses are displayed, but hostname lookup, TCP, HTTP/downloads and application socket APIs are future work. Public Internet ping depends on the host/backend; the local QEMU router is the supported test target. See [QEMU networking documentation](https://www.qemu.org/docs/master/system/devices/net.html).

Run `rustc --edition=2021 --test tools/network_tests.rs -o target/network-tests.exe` and the resulting executable for packet/state tests. `python tools/network_qemu.py --kernel PATH --disk PATH` checks QEMU user networking, a private local packet peer, short leases, receive-ring wrapping/recovery, link changes and desktop/Terminal operation. Test images are disposable copies.

Networking is disabled on every boot. The developer-only command `net experimental-enable` enables it for the current boot; `net off` disables it again. These commands are omitted from help/completion and have no Settings control or saved enable flag. Disabled adapters have RX/TX, interrupts and PCI bus mastering off; no DHCP or packet parsing runs until enabled.

## Dragging files

In the desktop, drag selected files or folders between Files windows. A green outline marks a receiving Files window; hovering a folder marks that row. Drop onto a folder to copy into it, or onto empty list space to copy into the displayed directory. Hold Shift to move instead; the pointer badge shows Copy or Move. Ctrl/Shift selection supports multiple items, and dragging a selected row preserves the selection. Escape cancels an active drag. Closing the source also cancels it.

Drops onto unsupported windows or the taskbar do nothing. Same-folder drops are no-ops, existing names offer Paste As instead of overwriting, and moves into a folder's own descendants are refused. The drag payload is independent of Copy/Move/Paste's clipboard. Transfers use the existing serialized filesystem worker, including its volume checks, busy-directory checks and size limits (currently 64 KiB per file for Files copy/move). Batch failures may leave earlier items completed. Wallpaper and desktop folder drops use the Files transfer flow described below; Notepad and Player accept supported document drops.

Drag one file onto Notepad to open it; unsaved edits retain the normal Save/Discard/Cancel prompt. Drag one WAV onto Player to load and play it. Application targets show **Open file** and always leave the source in place, even with Shift held. Folder/multiple-file drops and drops during an existing app dialog are refused.

### Desktop files and folders

The wallpaper shows up to 20 files and folders from `/DESKTOP` on the mounted drive. The Desktop shortcut opens that folder in Files, including items beyond the icon limit. Items persist across boots; icons refresh while the filesystem worker is idle. Click a folder to browse it, or a document to open its associated app. Executables are not automatically run by desktop document opening.

Drag from Files onto exposed wallpaper to copy into `/DESKTOP`, or onto a desktop folder to copy into it. Hold Shift to move. A Files window opens to perform the transfer and handle existing-name prompts; a free application slot is required. Drag desktop icons into Files or compatible document apps using the same rules. Escape cancels an active drag. These gestures do not replace the shared file clipboard. Drag a desktop icon onto exposed wallpaper to rearrange it on the grid. Dropping onto an occupied document cell swaps positions; dropping onto a different folder still transfers into it. Positions save automatically to `/DESK.CFG` on the mounted drive when the desktop is idle, and restore across desktop restarts and reboots. Invalid layout files fall back to automatic placement. A failed save plays the error sound; the session layout remains usable. Right-click an icon for Open, or empty wallpaper for Open Desktop. Both menus offer Reset icon positions, which also updates the saved layout. Escape or an outside click dismisses the menu.

Desktop selection supports Ctrl-click toggles and Shift-click ranges in grid order. Hold Ctrl on the first click to select without opening. Drag any selected icon to transfer the entire selection to Files or an unselected desktop folder; Shift moves. Dragging the group onto wallpaper preserves its relative grid shape, clamps it inside the desktop grid, and relocates other occupied icons into vacated cells. Click empty wallpaper to clear the selection. Open-file apps still accept one regular file only.

Click exposed wallpaper to give the desktop keyboard focus. Arrow keys select nearby icons in the grid; Home/End select the first/last occupied cell. Shift extends the range, Ctrl+arrows moves the selection cursor without replacing selected items, Space toggles the current icon, and Ctrl+A selects all desktop icons. Enter opens a single selected item. Escape clears selection; when no icons are selected and no apps are running, Escape retains the return-to-shell shortcut. Application keys continue to go to the focused application.

Desktop Ctrl+C/Ctrl+X stages the selected icons in the same file clipboard used by Files. Paste in a Files window with Ctrl+V, or focus wallpaper and use Ctrl+V to paste into `/DESKTOP`. Desktop Paste opens Files to perform the transfer and resolve name conflicts; it requires a free app slot. A changed clipboard during loading cancels the queued paste. Icon context menus also offer Copy/Cut, and the wallpaper menu offers Paste. Cut items are only removed after a successful transfer; the clipboard updates as each move completes.

## Recycle Bin

Files now confirms **Recycle** instead of permanent deletion. Open **Recycle Bin** from Apps or the desktop shortcut to select a record and choose Restore (R/Enter, then Y). It restores the original path, refuses an existing name, and keeps the record if the original parent is missing. N/Escape cancels. Refresh and page controls browse the records. Select an item and choose Delete forever (or press Delete) for a separate Y/N confirmation; this cannot be undone. Empty Bin is not implemented yet. The physical shell's `delete` command still deletes permanently.

Records live under `/TRASH/T0000000` and similar IDs, with `INFO.DAT` recording the original path and `ITEM` holding the data. Recycling uses the existing copy-then-remove transfer path: 64 KiB per file, up to 256 tree entries and eight path components. Oversized/in-use items are refused; successful earlier items in a selection can remain recycled if a later item fails. Up to 1024 record IDs are supported. Data is not journalled: interrupted/failed operations keep unfinished records for manual recovery, and the app refuses automatic restore of those records. Do not edit or recycle the `/TRASH` subtree through Files. Restore is not an overwrite operation; move/rename a conflicting original first.

On the desktop, select icons and press Delete or right-click **Recycle**. Files opens with that exact selection and asks for Y/N confirmation; nothing is recycled before Y. Cancel or close the window to leave the originals intact. Multi-selection is supported, and the shared Copy/Cut clipboard is preserved. A free application slot is required.

Permanent deletion preflights up to 32 payload entries, including the selected root, and refuses larger trees before changing data. Unfinished/corrupt recycle records remain protected. A deletion interrupted after its marker is saved cannot be restored; select it and use Delete forever again to retry. This is bounded cleanup, not guaranteed power-loss recovery.
