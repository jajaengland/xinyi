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

Type `desktop` at the shell. Apps lists the actual `.BIN` files in `/APPS`; replacing or deleting a file changes the available programs. Up to four root application windows and their attached text processes can run together.

- **Files:** browse folders, open documents in Notepad, run programs through Terminal, create folders and confirm deletions. F5 refreshes; PageUp/PageDown changes pages.
- **Notepad:** graphical text editing, open/save/save-as, undo/redo and unsaved-change prompts. Drag or Shift+arrows selects text; Ctrl+A/C/X/V selects all, copies, cuts and pastes. Ctrl+S saves; Ctrl+Q quits.
- **Terminal:** uses the same shell as the physical console. Text apps launched from Apps automatically get a terminal window. Ctrl+C interrupts a program; Ctrl+Shift+C copies selected output; Ctrl+V pastes. Shift+PageUp/PageDown browses scrollback.
- **EDIT:** text-mode file editor, also usable inside Terminal. Ctrl+S saves; Ctrl+Q quits.
- **Snake / Tetris:** text games with pause/restart; Escape quits. Snake beeps when food is collected.
- **Counter / Hello:** small application and scheduling examples.

Drag title bars to move windows, including partly off-screen. Windows stay below the top taskbar. The **_** button minimizes without closing or pausing the program; its taskbar button restores it fully into view. Ctrl+Alt+K stops the focused app. Close running apps before selecting Exit.

## Shell and storage

`help` lists commands. Common commands include:

| Purpose | Commands |
| --- | --- |
| Browse | `pwd`, `ls [PATH]`, `cd PATH`, `mkdir PATH`, `rmdir PATH` |
| Text files | `touch FILE`, `cat FILE`, `write FILE TEXT`, `delete FILE` |
| Applications | `apps`, `run NAME [ARGS]`, `edit [FILE]`, `notepad [FILE]`, `terminal` |
| Drives | `disks`, `use NUMBER`, `mount`, `unmount` |
| System | `version`, `uptime`, `date`, `reboot` |

The system mounts its boot filesystem automatically. BIOS boot-drive access supports boot USB media; native disks use ATA/IDE PIO, including compatible IDE emulation. BIOS boot media appears as drive 0, with native IDE targets numbered separately. Use `disks` before selecting a target.

`install NUMBER` previews a bootable installation onto a native IDE destination and stages `/APPS` from the mounted source. Only `install confirm` immediately after the preview applies it. Compatible FAT16 layouts can be preserved; incompatible layouts are replaced after confirmation. `format` previews a whole-disk quick format; `format confirm` applies it and removes existing partitions and bootability. **Both operations can destroy data: inspect the target and preview carefully.**

## USB images

```powershell
python tools/build_usb.py
```

This packages and tests the latest verified release. `target/latest-usb.json` records the raw image and checksum. Flash the image in raw/DD mode, then boot in legacy BIOS mode. Flashing replaces the selected USB drive's contents.

USB access uses BIOS services for the boot drive only. Unmount before removal. Firmware writes are read back, but there is no portable firmware cache-flush guarantee.

## Current limits

- Legacy BIOS boot; no UEFI or native USB/AHCI drivers.
- FAT16 volumes below 2 GiB, ASCII 8.3 names and paths up to eight components.
- Text files and the shared clipboard: 1024 bytes. Executable images: 64 KiB.
- Flat read-only program images with writable stacks; no writable globals, user heap or in-kernel ELF loader.
- Fixed-size windows, basic bitmap fonts and 800×600 framebuffer graphics; no GPU acceleration.
- Kernel services run to completion, so disk I/O can briefly delay other applications.
- No filesystem journal or power-loss recovery. This is a learning OS, not a hardened system for important data or untrusted programs.

## Development and tests

Automated storage tests use disposable QEMU images. `python tools/build_release.py` runs the release integration suite. Individual checks include:

```powershell
python tools/smoke.py --kernel target/i686-unknown-none/release/rust-os --qemu qemu-system-i386
python tools/window_lifecycle_qemu.py --disk PATH/TO/xinyi.img --vga cirrus --usb
rustc --edition=2021 --test tools/window_tests.rs -o target/window-tests.exe
```

`graphics`, `mouse`, `windowtest` and `multitask` provide interactive diagnostics. Deliberate fault/panic probes are test tools, not normal applications.

- [Architecture](ARCHITECTURE.md)
- [Application SDK](sdk/README.md)
- [Roadmap](TODO.md)
