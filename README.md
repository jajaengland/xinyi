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

Type `desktop` at the shell. Apps lists the actual `.BIN` and `.ELF` files in `/APPS`; replacing or deleting a file changes the available programs. Up to four root application windows and their attached text processes can run together.

- **Files:** browse folders, open documents in Notepad, run programs through Terminal, create folders and confirm deletions. F5 refreshes; PageUp/PageDown changes pages. Click empty space to clear selection; arrow keys select an item again. F2 renames a file or unused folder. Ctrl+click toggles items; Shift+click selects a range; Ctrl+A selects up to 32 items. Right-click for actions. Copy/Move (Ctrl+C/X) shares the selection across Files windows, even after closing the source window; open a destination and Paste (Ctrl+V). Folders transfer recursively. Existing names offer Paste as rather than overwrite. Delete removes files and empty folders after confirmation.
- **Settings:** choose desktop and active title-bar colours. Apply changes the current session; Save writes `/GUI.CFG` on the mounted drive for the next boot.
- **Notepad:** edit ASCII documents up to 64 KiB, with scrolling, four undo steps and unsaved-change prompts. Drag or Shift+arrows selects text; Ctrl+A/C/X/V selects all, copies, cuts and pastes. Ctrl+S saves, Ctrl+Shift+S opens Save As, Ctrl+O opens a file, Ctrl+N starts a new document, Ctrl+Z/Y undoes/redoes and Ctrl+Q quits. Open/Save As browses folders without changing the working directory.
- **Terminal:** uses the same shell as the physical console. Text apps launched from Apps automatically get a terminal window. Ctrl+C interrupts a program; Ctrl+Shift+C copies selected output; Ctrl+V pastes. Drag the right-hand scrollbar, click its arrows/track, or use Shift+PageUp/PageDown for up to 64 rows of scrollback. The text area retains all 80 columns.
- **EDIT:** text-mode file editor, also usable inside Terminal. Ctrl+S saves; Ctrl+Q quits.
- **Snake / Tetris:** text games with pause/restart; Escape quits. Snake beeps when food is collected.
- **Counter / Hello:** small application and scheduling examples.
- **MEMDEMO:** an ELF application demonstrating writable globals, zeroed BSS, and heap-backed Rust `Vec`/`Box`. Run it from Apps or with `run memdemo`.

Use the maximize/restore title-bar button or double-click the title bar in Files, Notepad and Terminal. Maximized frames fill the desktop below the taskbar, respect app layout limits and restore their previous size/position. Minimize and taskbar restore preserve maximized state.

Drag the lower-right grip to resize Files or Notepad; Notepad reflows text and Files shows more rows. Terminal expands its background and right-edge scrollbar horizontally and grows vertically to display up to 34 rows, while preserving the 80-column text-program layout. Apps that do not opt in remain fixed-size.

Closing an app shifts the remaining taskbar buttons left; new apps append on the right.

Use **Alt+Tab** to cycle apps in taskbar order and **Shift+Alt+Tab** to cycle backwards, including minimized apps. **Alt+F4** requests a normal close so editors can prompt to save. **Ctrl+Esc** opens Apps; use arrows, Home/End and Enter to launch, or Escape to cancel.

During desktop file operations, the working app's window title shows `I/O` and a completed block-operation count. Other apps remain usable. Ctrl+Alt+K waits for an active filesystem operation to finish before reclaiming that process. Terminal typing and non-disk commands remain live; submitted filesystem commands wait for the worker.

Drag normal title bars to move windows, including partly off-screen. Windows stay below the top taskbar. The **_** button minimizes without closing or pausing the program; its taskbar button restores it into view. Ctrl+Alt+K stops the focused app. Close running apps before selecting Exit.

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
| Applications | `apps`, `run NAME [ARGS]`, `edit [FILE]`, `notepad [FILE]`, `terminal`, `settings` |
| Drives | `disks`, `use NUMBER`, `mount`, `unmount` |
| Recovery | `fsck`, `fsck repair-orphans`, `fsck confirm` |
| System | `version`, `uptime`, `date`, `reboot`, `shutdown` |

The system mounts its boot filesystem automatically. BIOS boot-drive access supports boot USB media; native disks use ATA/IDE PIO, including compatible IDE emulation. BIOS boot media appears as drive 0, with native IDE targets numbered separately. Use `disks` before selecting a target.

`install NUMBER` previews a bootable installation onto a native IDE destination and stages `/APPS` from the mounted source. Only `install confirm` immediately after the preview applies it. Compatible FAT16 layouts can be preserved; incompatible layouts are replaced after confirmation. `format` previews a whole-disk quick format; `format confirm` applies it and removes existing partitions and bootability. **Both operations can destroy data: inspect the target and preview carefully.**

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
- Notepad documents: 64 KiB of ASCII text, with a scrollbar and four undo steps. Shell text commands, EDIT and the shared clipboard retain their 1024-byte limit. Executable images: 64 KiB.
- Flat binaries remain supported. Static ELF32/i386 apps can use writable data/BSS (up to a 1 MiB image span) and a private heap capped at 1 MiB. Executable files still fit within 64 KiB. No dynamic linking, TLS, user threads or Linux ABI compatibility.
- Bounded opt-in window resizing, shared GUI controls, basic bitmap fonts and 800×600 framebuffer graphics; no GPU acceleration.
- Desktop app file calls are queued and yield between completed disk blocks; other apps keep running during saves and copies. Individual firmware/controller calls, executable loading and kernel shell file commands still run synchronously.
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
