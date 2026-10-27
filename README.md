# Xinyi

WARNING: Below is not human written

Xinyi is an experimental 32-bit x86 kernel written primarily in Rust, with a small amount of x86 assembly for bootstrapping and interrupt handling.

The project is being developed as a learning project, with the long-term goal of building a small operating system with a shell, applications, and eventually a graphical interface.

Automated development tests use QEMU with disposable image files. The user handles real-hardware testing when needed. The dc7800 resumed working after its RAM was reconfigured, and Xinyi boots there again.

## Current Features

- 32-bit x86 bare-metal kernel
- Rust `no_std` environment
- Kernel-owned flat Global Descriptor Table (GDT)
- VGA text-mode output with scrolling and hardware cursor support
- Interactive shell
- Command history and command-line editing
- Tab completion for commands
- Real-time clock date and time command
- CPU exception handlers with fault details (vector, error code, EIP, CS, EFLAGS, and CR2 for page faults)
- Kernel panic reporting with message, source location, and debug-console output
- Interrupt Descriptor Table (IDT)
- Programmable Interrupt Controller (PIC) remapping
- Programmable Interval Timer (PIT) at 100 Hz
- Timer-based uptime tracking
- Interrupt-driven PS/2 keyboard and mouse input with separate event queues
- A graphical desktop test with draggable overlapping windows, focus, close buttons and restoration
- Bootloader memory-summary and memory-map reporting
- Supervisor kernel mappings with private 4 KiB user code/stack pages
- Ring 3 user-mode entry with a TSS-backed kernel stack
- `int 0x80` system-call interface
- Synchronous protected user-program execution, text output and exit/fault recovery
- Foreground program keyboard events, cursor/colour control, positioned text drawing and interrupt-based waits
- Coloured text-cell rows, cursor hiding, FAT16 programs up to 64 KiB and a playable text-mode Tetris demo
- Snake with growth, pause/restart, wall/body collisions and a PC-speaker beep for each eaten dot
- Programs are ordinary replaceable FAT16 files in `/APPS`; `apps` lists them and `run NAME` searches that directory
- Full-screen ASCII text editor with open/save/save-as, scrolling, one-level undo/redo and unsaved-change prompts
- Automatic system-drive mounting plus numbered `disks` / `use NUMBER` / `install NUMBER` commands
- File listing, creation, reading, and replacement in RAM or a mounted FAT16 volume
- FAT16 nested folders and file persistence with ASCII 8.3 names and files up to 1024 bytes
- Native IDE PIO plus BIOS boot-drive access (including USB), and a BIOS bootable installer with a separate FAT16 data partition
- Reboot command
- An 800x600 graphics test using QEMU standard VGA or BIOS VESA 16/24/32-bit RGB with clipped lines, filled/outlined triangles, bitmap text, arrow-key movement and restoration of the text console

## Requirements

Development currently uses MSYS2 UCRT64 on Windows.

Required tools include:

- Rust nightly toolchain
- Rust source component (`rust-src`)
- NASM assembler
- QEMU system emulator for x86
- Cargo and the Rust build tools
- Python 3 for building and packaging application files

The project uses a custom target specification: `i686-unknown-none.json`.

## Build

Run this from the repository root:

`cargo +nightly build -Z build-std=core`

Both debug and release builds compile the sources listed in `programs/apps.txt` into **separate application files**, not kernel resources. The release packager places `HELLO.BIN`, `EDIT.BIN`, `TETRIS.BIN` and `SNAKE.BIN` in the FAT16 `/APPS` directory. They can be deleted or replaced without rebuilding the kernel. `run NAME` searches the current directory and then `/APPS`; explicit paths load the specified file.

The installed system drive mounts automatically. Use `apps`, `run snake`, `run tetris` or `edit note.txt` directly. On an uninstalled boot, a unique application volume (or sole FAT16 volume) is opened automatically. With multiple candidates, use `disks` and `use NUMBER`; no PCI-address typing is necessary. Switching validates the destination before detaching the current drive. Unsupported/blank drives remain available as installation targets but are not formatted automatically.

`install NUMBER` stages application files from the currently mounted `/APPS`, previews the destination, and copies them after `install confirm`. Existing compatible user files are preserved; matching destination app files are replaced. Cancellation releases staging memory without disk writes. Launching a bare kernel without an app filesystem is a development-only, kernel-only installation path.

The editor supports 1 KiB ASCII files, newline insertion, Backspace/Delete, arrows, Home/End, Page Up/Down, Ctrl+Home/End, scrolling, Ctrl+S Save, Ctrl+Shift+S Save As, Ctrl+O Open, Ctrl+N New, one-level Ctrl+Z/Y undo/redo and Ctrl+Q/Escape Quit. Unsaved changes and Save As overwrites require a choice; failed saves retain the document. CRLF is normalized to LF and tabs expand to four spaces when loading.

To build a distributable bootable image and verify its applications in QEMU:

```powershell
python tools/build_release.py
```

This creates a fresh directory under `target/releases/` containing `xinyi.img`, the release kernel, a Windows QEMU launcher with speaker audio, usage notes, the application manifest and SHA-256 checksums. It tests filesystem app loading, persistent editor saves, native BIOS and USB boot, generic VESA graphics, and copying apps onto another drive before producing the image. `target/latest-release.json` records the result. Existing output directories/images are refused. This creates local release artifacts; it does not publish a GitHub release.

## Run in QEMU

After a successful build, run:

`qemu-system-i386 -kernel target/i686-unknown-none/debug/rust-os`

## Debugging

Type `graphics` to try the framebuffer test. Arrow keys move the box and Escape returns to the shell. QEMU standard VGA retains its direct backend. When booting the disk/USB image through legacy BIOS, other video cards can use VESA 2/3 linear framebuffer modes at 800x600 with RGB565 or 24/32-bit RGB. `bios` reports firmware and video diagnostics. This is a kernel/display diagnostic, not a desktop or a graphics API for applications. The editor and games remain ordinary files in `/APPS`.

Booting the packaged image from USB now mounts that USB filesystem automatically through BIOS EDD calls. Applications and editor saves work on the boot USB without a native USB controller driver. `disks` lists this as **drive 0**; `use 0` returns to it. Native IDE disks keep numbers 1 and above. Use `install NUMBER` to install onto a native IDE destination, copying `/APPS` from the mounted source. Drive 0 is refused as a format/install destination; it may alias an IDE disk that also appears under its native number. Boot filesystem errors are reported without automatically selecting another disk.

This firmware path requires our legacy BIOS loader. Direct QEMU `-kernel` boot has no firmware handoff and retains native IDE/QEMU graphics behavior. USB hotplug, non-boot USB drives, UEFI and native USB/AHCI drivers are still outside this support. BIOS writes are synchronous and read back, but firmware provides no portable cache-flush durability guarantee; use `unmount` before removal. USB application loading, VESA graphics and saved-file persistence across reboot have also been confirmed by the user on an HP laptop; dc7800 verification of these new paths remains separate.

The renderer caches the static scene in an owned RAM back buffer. Box movement updates only its old/new footprints instead of redrawing and copying the whole screen. Held arrows use a 50 Hz timer-paced movement path, with immediate taps and release tracking, while idle time still uses HLT. Normal exit restores the VGA console (saved registers/font/VRAM on QEMU, BIOS text mode plus saved text/cursor on VESA), then frees the buffers; fatal diagnostics also attempt text restoration. Low-memory and unsupported-display paths leave the shell usable. `python tools/graphics_qemu.py` checks both kernel profiles and saves graphics screenshots under `target/smoke/`.

Fault and panic details are mirrored to QEMU's debug console at I/O port `0xE9`. To capture them without opening a display:

`qemu-system-i386 -kernel target/i686-unknown-none/debug/rust-os -display none -no-reboot -debugcon file:debug.log -global isa-debugcon.iobase=0xe9`

`-no-reboot` makes QEMU exit instead of restarting after a triple fault. The `keyboard` command reports modifier state and scancode statistics. The `breakpoint` and `panic` commands exercise the fault-reporting paths.

With `-display none`, the 80x25 VGA text buffer can still be read through QEMU's QMP interface with `pmemsave` at guest physical address `0xB8000`, size `80 * 25 * 2`.

If the build cannot find NASM on Windows, add MSYS2's tools to `PATH` for the session:

`$env:PATH = "C:\msys64\usr\bin;$env:PATH"`

## Shell Commands

| Command | Description |
| --- | --- |
| `help` | Show available commands |
| `about` | Show information about Xinyi |
| `clear` | Clear the screen |
| `echo TEXT` | Print text |
| `version` | Show kernel version |
| `date` | Display date and time from the RTC |
| `uname` | Show architecture and environment |
| `uptime` | Show time since PIT initialization |
| `keyboard` | Show keyboard modifier state and scancode buffer diagnostics |
| `memory` | Show the multiboot memory summary and memory map |
| `paging` | Show paging configuration and control-register state |
| `frames` | Show physical frame allocation statistics and run a self-test |
| `usermode [probe]` | Run a protected user probe, report its exit/fault, and return to the shell |
| `ps` | Show current task ID and total task records |
| `run [NAME\|FILE]` | Search the current directory and /APPS for a program |
| `spawn [demo]` | Create a paused demo task record for diagnostics |
| `kill PID` | Remove a task record and release its program and stack frames |
| `ls [PATH]` | List files and folders in the current or specified directory |
| `mkdir PATH` | Create a folder (its parent must exist) |
| `cd [PATH]` | Change directory; no path returns to root |
| `pwd` | Show the current directory |
| `rmdir PATH` | Remove an empty folder |
| `touch FILE` | Create a file |
| `delete FILE` | Delete a file from the active filesystem |
| `write FILE [TEXT]` | Replace an existing file's contents; omit TEXT to clear it |
| `cat FILE` | Display file contents |
| `disks` | List supported drives by number |
| `use NUMBER` | Select a drive and open its FAT16 filesystem |
| `edit [FILE]` | Open/create a text file in /APPS/EDIT.BIN |
| `apps` | List the files in /APPS |
| `mount [NUMBER]` | Open a numbered drive or mount the selected disk |
| `unmount` | Flush/detach the volume and return to RAM files |
| `format` | Preview a destructive FAT16 quick format of the selected IDE disk |
| `format confirm` | Execute the immediately preceding format preview |
| `breakpoint` | Test the CPU breakpoint exception handler |
| `panic` | Test the kernel panic handler |
| `disk` | Identify the target IDE master, report model/errors, and read sector 0 |
| `disk scan` | Report PCI storage controllers and probe IDE drive positions without changing the target |
| `disk select BB:DD.F secondary master` | Select a controller/channel/drive; primary/slave and legacy are also supported |
| `install` | Preview a bootable installation on the selected IDE disk |
| `install confirm` | Execute the immediately preceding installation preview |
| `reboot` | Attempt to reboot the machine |

The shell also supports command history, cursor movement, Backspace, and Tab completion for uniquely matching commands.

The prompt shows the current directory, for example `xinyi:/>` or `xinyi:/DOCS>`. It updates after `cd`, mounting or unmounting a disk; `pwd` remains available.

`install` refuses mounted disks and previews whether it will preserve compatible existing FAT16 data or replace the disk layout. Only an immediately following `install confirm` writes. The `panic` dispatch works but is currently missing from the help/completion table.

Unknown commands are reported with a quoted name. When exactly one known command matches by prefix, the shell also prints a case-insensitive "Did you mean" suggestion. Commands are separated from their arguments by whitespace, and repeated spaces are ignored.

## Repeatable Smoke Checks

After building, run `python tools/smoke.py` with Python 3 and QEMU on `PATH`. Use `--qemu PATH` or `--kernel PATH` to override their locations. The script starts headless QEMU, connects to QMP over local loopback, injects paced PS/2 keys, and reads VGA memory. It creates and removes its own temporary 10 MiB IDE fixture under `target/smoke`; existing disk images are never attached.

Checks cover boot, memory reporting, paging, frame alloc/free counts, keyboard diagnostics, shell input/editing/history/completion, basic VFS creation/reading, task-record creation, disk sector reads, uptime, panic/breakpoint diagnostics, and ring 3 `getpid` followed by the expected protection fault. Each fatal check uses a fresh VM. The transcript is saved to `target/smoke/transcript.txt`. These checks do not establish multitasking, persistent storage, working installation, or user-memory isolation.

For storage checks, run:

```powershell
rustc --edition=2021 --test tools/storage_tests.rs -o target/storage-tests.exe
.\target\storage-tests.exe
python tools/storage_qemu.py
python tools/folders_qemu.py
python tools/format_qemu.py
python tools/reliability_qemu.py
python tools/lba48_qemu.py
python tools/install_qemu.py
python tools/ata_qemu.py
python tools/syscall_qemu.py
python tools/isolation_qemu.py
python tools/user_recovery_qemu.py
python tools/programs_qemu.py
rustc --edition=2021 --test tools/user_buffer_tests.rs -o target/user-buffer-tests.exe
.\target\user-buffer-tests.exe
rustc --edition=2021 --test tools/phys_tests.rs -o target/phys-tests.exe
.\target\phys-tests.exe
python tools/task_memory_qemu.py
```

The host tests cover file/cluster boundaries, full filesystems, malformed volumes/chains, mirror mismatches, and injected write/flush failures. The QEMU tests create disposable FAT16 volumes, read host-created files in the guest, write files in the guest, reboot and start a fresh VM, then independently read back the image on the host. Both whole-volume and primary MBR-partition layouts are checked, along with a missing disk and RAM fallback. Results are saved to `target/smoke/storage-transcript.txt`.

For `syscall_qemu.py`, build both profiles first: `cargo +nightly build -Z build-std=core` and `cargo +nightly build --release -Z build-std=core`. It checks repeated calls with distinct arguments, returns, preserved registers/segments/stack and restored flags before a recoverable privilege fault. It verifies invalid-descriptor handling and uses a zero-byte write as its success marker. Results are saved to `target/smoke/syscall-transcript.txt`; `--kernel PATH` checks a single build.

`task_memory_qemu.py` checks both profiles with 8 MiB of RAM: program bytes and zeroed allocation tails/stacks, distinct owned page directories/tables and their permissions, all 16 task slots, repeated full-table refusals without leaks, complete reclamation through `kill`, slot reuse, and the protected ring-3/syscall probe. It saves `target/smoke/task-memory-transcript.txt`. `phys_tests.rs` checks contiguous allocation, fragmentation, exhaustion and balanced release without booting a guest. Actual execution and scheduling of task records remain future work.

`isolation_qemu.py` checks both profiles with 8 MiB RAM. It verifies real ring-3 faults for kernel reads/writes/execution, access to the syscall stack, page directory, VGA and physical image alias, writes to read-only user code, and writes into the unmapped stack guard. Fault addresses/error codes are checked, kernel bytes remain unchanged, and each fault returns to a usable shell. A CPU without PSE must refuse user setup without leaking frames. Results are saved to `target/smoke/isolation-transcript.txt`.

`usermode exit` runs the diagnostic probe and exits with code 42. Plain `usermode` demonstrates a recoverable privilege fault. The kernel restores its address space, stack, segments, flags and TSS stack before reclaiming memory. `run demo` executes a hello-world process; `mount` followed by `run hello.bin` executes a flat image from the active filesystem. Execution is synchronous; scheduling remains pending.

`user_recovery_qemu.py` repeats normal exits and faults in the same 8 MiB VM for debug and release builds. It independently reads CR3 and TSS ESP0, checks frame balance and restored flags, exercises timer IRQs with null user segments using deterministic virtual time, and verifies that a subsequent kernel breakpoint remains fatal. Its transcript is `target/smoke/user-recovery-transcript.txt`.

Formatter tests cover layout sizing, metadata replacement, write/flush failures, mounted-disk refusal, preview cancellation, one-use confirmation, and empty/missing disks. `format_qemu.py` formats only its private disposable images and verifies host readback and cold-boot persistence; its transcript is `target/smoke/format-transcript.txt`.

`reliability_qemu.py` samples the idle virtual CPU's halted state, checks repeated keyboard wakeups and advancing uptime, and verifies format/fresh-install refusal on a private sparse 3 TiB QCOW2 disk beyond the current block-interface limit without guest-data allocation. It needs `qemu-img` on `PATH` and saves `target/smoke/reliability-transcript.txt`. These are emulator checks, not physical temperature or power measurements.

`lba48_qemu.py` uses private sparse 160 GiB QCOW2 images to verify FAT16 reads/writes beyond the 128 GiB boundary, independent host readback, unchanged low-address aliases, cold-boot persistence, preserving installation with a high-address partition, and BIOS disk-only boot. A fresh installation also checks backup-GPT cleanup at the actual disk end. It needs `qemu-img` and `qemu-io` and saves `target/smoke/lba48-transcript.txt`.

Installer tests verify the installed payload against the original ELF image, including after runtime state changes. They then boot the IDE disk with **no `-kernel`, CD, or floppy**, test memory/allocation/paging and persistent files at 32/128 MiB RAM, reinstall from the running installed system while preserving data, and reject corrupt headers/payloads or insufficient RAM. Host tests also check conflicting layouts, changed targets, and write failures. Results are in `target/smoke/install-transcript.txt`. `python tools/install_qemu.py --output target/xinyi-installed.img` saves a new verified bootable image and refuses to overwrite an existing file.

ATA tests verify read-only scans with disks in primary-master, primary-slave, and secondary-master positions, reject an unsupported AHCI controller, and check that scanning never redirects file/format/install operations to a discovered disk. `target/smoke/ata-transcript.txt` records these diagnostics. Native-mode PCI BAR decoding and tick-based timeout behavior have host tests; the available QEMU IDE models exercise compatibility-mode I/O, not the physical HP ICH9 native-mode path.

## Writing Programs

See [the SDK guide](sdk/README.md) and [the Rust hello example](programs/hello/main.rs). `tools/build_program.py` builds a flat image; `tools/program_disk.py` packages it into a new FAT16 data image. `programs_qemu.py` verifies built-in/file execution, console output, real process IDs, invalid buffers, cleanup and unchanged disk contents in both kernel profiles. Its transcript is `target/smoke/programs-transcript.txt`.

## Diagnosing a Disk That BIOS Detects but Xinyi Does Not

Keep the controller in IDE emulation mode and boot a newly built image, then run:

```text
disk
disk scan
```

The driver now discovers the first PCI IDE controller instead of assuming ports `0x1f0`/`0x3f6`. Compatibility-mode channels retain the legacy addresses; native-mode channels use BIOS-assigned PCI I/O BARs. PCI configuration is read only: Xinyi does not switch controller modes, enable disabled decoding, or rewrite BARs. That controller's primary master is the default until an explicit or installed target is supplied. A scan can identify a disk at a different channel/slot, but never makes it the format/install target automatically. Scan while unmounted.

Diagnostics include PCI vendor/device IDs, interface/command flags, BARs and controller mapping fields, plus per-drive status/error registers, signature, model and accessible capacity. They distinguish absent drives, timed-out polls, ATA errors, ATAPI optical devices, unsupported sector sizes, and unavailable routing. IDENTIFY gets a five-second PIT deadline, normal commands two seconds, and flush ten seconds. A separate stagnant-timer poll bound prevents an infinite loop; the former total-iteration limit could expire too early on hardware.

The HP photos confirmed the WDC drive at PCI `00:1f.2`, secondary master (`0x170`/`0x376`), while the old default was `00:03.2` primary master. Choose the detected disk explicitly in the updated build:

```text
disk select 00:1f.2 secondary master
disk
mount
```

Selection issues IDENTIFY without writing disk sectors, displays the model/capacity, and retains the prior target if identification fails. It is refused while mounted. Mounting, formatting, and installation share the selected device; destructive previews include its path and model. Any selection command cancels pending destructive confirmations. A disk can identify successfully while mount still rejects a non-FAT16 filesystem.

Installation saves the PCI controller identity/channel/drive in a checksummed header record. The BIOS loader also passes its actual boot-drive number. BIOS disk/USB boots mount that boot filesystem as drive 0, so moved images do not depend on the original PCI target. Native destinations remain explicit and use PCI identity rather than inferring ATA targets from BIOS numbering. `disk select legacy secondary master` is also supported; PCI paths are preferred when known.

The WDC's earlier 131071 MiB report was the LBA28 accessible-range limit. The driver now uses LBA48 PIO and extended cache flushes when IDENTIFY supplies valid LBA48 capacity; legacy-only devices retain LBA28 commands. `disk` reports the addressing mode. The block interface still limits the accessible sector count to `u32::MAX` (just under 2 TiB with 512-byte sectors), retaining the full capacity separately. Whole-disk formatting and fresh installation are refused if full capacity is unknown or exceeds that range. Compatible existing FAT16 partitions entirely within the accessible range can still be mounted and preserved during installation. FAT16 volumes remain capped just below 2 GiB. Native AHCI/RAID and USB controller drivers remain unsupported; the BIOS boot drive is accessible through the firmware bridge, including USB boot. QEMU verifies explicit selection and saved-target restoration; physical LBA48 behavior has not yet been verified.

## Using Persistent Storage

Create a new FAT16 data image and attach it to QEMU (the image tool refuses to overwrite a file):

```powershell
python tools/mkfat16.py target/xinyi-data.img
qemu-system-i386 -kernel target/i686-unknown-none/debug/rust-os -drive file=target/xinyi-data.img,format=raw,if=ide,index=0
```

In Xinyi:

```text
mount
touch note.txt
write note.txt hello from xinyi
ls
cat note.txt
reboot
mount
cat note.txt
```

Mounting is explicit on every boot. Until `mount` succeeds, file commands use a separate RAM filesystem; mounting/unmounting never copies files between the two. Names are case-insensitive ASCII 8.3 names, listed in uppercase. Files are limited to 1024 bytes; the shell's existing 128-byte input-line limit also limits each typed `write` command. `touch` rejects existing names and `write` requires an existing file. Shell whitespace trimming applies to file text too.

Folders work on both backends:

```text
mkdir docs
cd docs
touch note.txt
write note.txt hello
pwd
ls
cd ..
cat /docs/note.txt
```

All file commands, including `run`, accept relative or absolute paths. `/` is the root; slash/backslash separators, `.` and `..` are supported. `ls PATH` lists another folder without changing directory. `rmdir` removes empty folders only and refuses the root, current folder and ancestors; use `delete` for files. Fresh mount, unmount and disk-error fallback reset navigation to root. Paths have at most eight components; FAT16 subdirectories are bounded to 32 clusters and RAM shares 32 slots between files/folders. Non-ASCII short names display a placeholder; navigation still requires ASCII 8.3 names.

`folders_qemu.py` checks host-created folders, nested file/program paths, RAM navigation, cold-boot persistence on both volume layouts, mirrored FATs and cluster reclamation with an independent recursive reader. Corrupt-folder fallback and OEM-name display are also checked. Transcripts are in `target/smoke/folders-transcript.txt` and, for `--corrupt-only`, `target/smoke/folders-errors-transcript.txt`.

`delete FILE` removes a file from the active backend, using the same case-insensitive ASCII 8.3 names. FAT16 deletion refuses read-only files, directories, files over the 64 KiB program limit, and invalid chains. It flushes the deleted directory entry before releasing clusters in each FAT copy. The slot and clusters can then be reused; file data is not securely erased. RAM deletion clears and releases its file slot.

The driver accepts 512-byte-sector FAT16 whole-volume images or the first supported primary MBR FAT16 partition. Writes update FAT mirrors and flush the device cache. I/O or chain-corruption errors detach the disk filesystem and announce the return to RAM. There is no journal or power-loss recovery: an interrupted metadata write can leave leaked clusters or inconsistent FAT copies. A failed delete can leave a file removed with some space unreclaimed. Long filenames, append and general read/write access to files larger than 1 KiB are future work; the separate program reader and deletion support files up to 64 KiB. `mkfat16.py --partitioned PATH` creates the MBR layout; neither layout is an OS installer or bootable kernel image.

To initialize or replace the selected IDE disk's filesystem from inside Xinyi:

```text
unmount
format
format confirm
mount
```

Read the preview before confirming: this is a **whole-disk destructive quick format**, including existing partitions and bootability. It targets the selected IDE disk, whose path, model, and accessible capacity appear in the preview. It does not infer an ATA target from BIOS drive numbering. It refuses mounted disks. `format` only previews; `format confirm` must be the next command, and every confirmation attempt is consumed. Any other command cancels the preview. The formatter creates one FAT16 LBA partition at sector 2048, with two FATs and an empty root directory. It selects a supported cluster size and uses at most just under 2 GiB; excess disk space remains unallocated and is announced in the preview. Too-small disks are refused before writing. Old primary/backup GPT metadata is cleared. Free data is not securely erased. It flushes and reads back metadata before reporting completion; after a failed format the disk remains unmounted and may require reformatting.

The formatter creates a data disk; `install` adds a bootable system. Formatting an installed disk removes its bootloader, so use `install` again afterward.

Before arming a format preview or writing any format metadata, the driver must establish that its accessible sector count equals the full physical count. Otherwise formatting is refused, including fresh installation. This ensures backup GPT metadata is cleared at the actual end of the device, rather than the LBA28 access boundary. The FAT16 volume-size cap above still applies to fully accessible disks.

## Installing and Booting From the IDE Disk

In a running Xinyi session with a writable IDE disk selected (use disk select first if needed):

```text
unmount
install
install confirm
```

Read the preview: a valid MBR/FAT16 layout whose partitions all start at or beyond sector 2048 can retain its data and partition table. The bootloader is replaced. Whole-volume FAT images, earlier/overlapping partitions, GPT, blank disks, and unsupported layouts instead get a **destructive new FAT16 layout** after confirmation, with up to just under 2 GiB used and the remainder unallocated. `install` alone is read-only; any other command cancels confirmation. Installation checks the target again, reads back writes, flushes, and publishes the bootable MBR last. An interrupted/failed installation may leave the disk unbootable and require recovery or a new installation; it is not an atomic update.

The installed system boots in **legacy BIOS mode** on x86 with BIOS EDD disk reads, E820 memory reporting, and an A20-capable PC platform. UEFI/Secure Boot and native NVMe/AHCI storage are unsupported. The user has booted Xinyi on real hardware; the new BIOS filesystem/VESA paths require a fresh hardware test. The bootloader loads the kernel independently of FAT16; after boot the boot drive is mounted automatically through BIOS. Use `apps` or `edit FILE` immediately.

To test a saved installed image, close the initial QEMU session and launch **without `-kernel`**:

```powershell
qemu-system-i386 -m 128M -drive file=target/xinyi-installed.img,format=raw,if=ide,index=0 -boot c
```

Rebooting a QEMU session that still has `-kernel` on its command line reloads that supplied kernel and does not prove that the installed disk can boot. The standalone command above uses only the disk. The root-level untracked bootloader/image experiments are older artifacts; the current loader is built from `src/disk_mbr.asm` and `src/disk_stage2.asm`.

## Project Documentation

- [Architecture and implementation details](ARCHITECTURE.md)
- [Development roadmap and known limitations](TODO.md)

## Development Notes

Xinyi is an experimental kernel and is not intended for production use. Features and internal interfaces may change as development continues.

The installation-capable debug build on 25 October 2026 succeeds with 22 warnings (the initial baseline had 43), including unused prototype code/imports and references to mutable statics in task management. These warnings have not all been audited; see the current baseline and priorities in `TODO.md`.

The keyboard IRQ drains all pending PS/2 bytes on each interrupt. If the kernel's scancode ring buffer overflows, it counts the dropped bytes and resets Shift and extended-prefix state so a lost release cannot leave a modifier stuck. Caps Lock toggles only on the press edge, so holding the key or auto-repeat cannot flip it repeatedly. The `keyboard` command reports this state and the drop count.

QEMU's emulated PS/2 controller queue is small (16 bytes) and can discard bytes before the kernel sees them during large synthetic input bursts. No kernel-side recovery can restore events that never arrive; normal typing is unaffected.

When making changes, build and test the kernel before committing. Preserve working functionality and update the architecture and roadmap documents when behavior changes.

To package the latest verified release for flashing, run `python tools/build_usb.py`. It creates a new raw image and instructions under `target/usb/`, checks USB-only boot and VESA graphics in QEMU, and records the image/checksum in `target/latest-usb.json`. Flash the image in raw/DD mode; this replaces the chosen USB contents.

## BIOS USB and VESA regression tests

`python tools/firmware_qemu.py` creates private images and verifies USB-only filesystem access, editor saves, independent FAT readback, cold boots, generic Cirrus VESA graphics with exact pixel checks, repeated text restoration/frame reclamation, and USB-to-IDE application installation. `--kernel PATH --package DIRECTORY` tests a release build. `rustc --edition=2021 --test tools/vbe_tests.rs -o target/vbe-tests.exe` builds pure mode-validation tests. No physical disks are attached by these tests.

## Mouse test

Type `mouse` for an 800x600 blue background with a traditional white arrow and black outline. Left-click either button to increment its counter; releasing outside cancels a pressed button. Right-click opens an on-screen menu with Reset clicks and Close menu. Outside clicks dismiss it without activating controls underneath. Escape restores the text console. The cursor restores only its previous footprint, with clipped drawing at screen edges.

This uses a standard three-byte PS/2 mouse on IRQ12; keyboard/mouse bytes are routed by the shared controller's AUX status. Initialization is bounded, command acknowledgements are consumed before streaming, and input queues recover conservatively after loss. Idle waits halt until input/timer interrupts. No native USB keyboard/mouse driver has been added; firmware legacy emulation may supply compatible input. Mouse support is verified in QEMU, with physical mouse testing still needed.

`rustc --edition=2021 --test tools/mouse_tests.rs -o target/mouse-tests.exe` and `tools/ps2_tests.rs` cover packet decoding, shared FIFO routing and UI state. `python tools/mouse_qemu.py` checks actual PS/2 movement/clicks, cursor pixels, capture, menu actions/edge bounds, keyboard recovery, frame balance and fatal text restoration. Use `--disk PATH --vga cirrus` to exercise the VESA backend with a private copy of a bootable image. Release builds now run that check too.

## Window system test

Type `desktop` to open two overlapping windows. Drag a title bar to move a window, click any exposed part to bring it forward, and click X to close it. Covered controls cannot receive clicks. The Buttons window retains its two counters when you close/reopen it. Right-click and choose Restore windows to reopen/reposition both windows; outside clicks dismiss the menu without activating controls underneath. Windows stay within the desktop area, and Escape returns to the shell. The existing `mouse` and `graphics` diagnostics remain available.

This is an initial kernel window manager test; windows do not yet host independent programs or multitask. It uses the existing PS/2 and QEMU/VESA display backends. The renderer recomposes the scene in RAM when window state changes and presents only damaged regions, including old/new window bounds and shadows. Ordinary pointer motion still updates only its old/new footprints.

`rustc --edition=2021 --test tools/desktop_tests.rs -o target/desktop-tests.exe` checks hit testing, focus, drag offsets/bounds, release capture, closing, menus and lost input. `python tools/desktop_qemu.py` verifies actual mouse interaction and compares the entire framebuffer against the composited RAM scene outside the pointer footprint after moving, raising, closing and restoring windows. `--disk PATH --vga cirrus` tests BIOS/VESA. Release packaging includes this check.

## Simple graphical font

The mouse and desktop tests use Xinyi Simple, a built-in ASCII bitmap font in `src/ui_font.rs`. Its hand-defined five-column glyphs expand to a 7x11 pixel shape inside the existing 8x16 character cells, so labels keep their current spacing and control layouts. It covers all 95 printable ASCII characters, including lowercase, digits and punctuation. Text is drawn directly into the graphical scene; the shell/editor VGA font and graphics diagnostic keep their existing font path. Loading font files, proportional spacing, Unicode and outline fonts remain future work.

## Foreground application windows and Notepad

`notepad [FILE]` (or `run notepad [FILE]`) loads `/APPS/NOTEPAD.BIN`, a protected ring-3 program. It creates a draggable 640x400 client window using the new window calls. The toolbar provides Save, Open, New, Save as and Quit. Click in the text area to place the caret; keyboard editing, navigation, scrolling, undo/redo and Ctrl+S/O/N/Q work as in the text editor. Save as checks before replacing another file. Closing via X uses the same unsaved-change prompt: S saves, D discards, C cancels. Save errors retain the document. ASCII text files still have the existing 1024-byte limit.

The kernel owns window chrome, input and a private client surface; the program receives client-relative mouse events and validated drawing calls, without direct framebuffer access. Application-owned text rows are cached, and presentation updates dirty 16-pixel strips. Keyboard and mouse queues share capture sequence numbers so a click is delivered before keys typed after it. This first API supports one synchronous foreground window; applications must poll/wait for window events to keep its chrome responsive. It does not yet run several programs concurrently or launch them from the desktop test.

The SDK in `sdk/xinyi.rs` exposes `window::open`, `event`, `draw`, `row`, `present` and `close`. Commands use RGB24 colours, fixed 8x16 text cells and a maximum batch of 16 draws. Entire batches (including ASCII text pointers) are checked before any drawing; event outputs must be writable user stack memory. Handles expire on close, and program exit/fault automatically restores text and frees surfaces. Text-console input/output calls return busy while a window is active; ticks, sleep, sound, arguments and file calls remain available.

`tools/window_tests.rs` covers geometry, capture, close requests and sequence wrap. `python tools/window_qemu.py` checks actual ring-3 pointer validation, invalid-batch rejection, stale handles, fault/low-memory cleanup, Notepad toolbar/caret/dragging, unsaved-close handling, read-only errors and cold-boot persistence. `--disk IMAGE --vga cirrus --usb` uses a private copy for BIOS USB/VESA verification. Release packaging runs this test; applications remain ordinary files copied by installation.

## Drag rendering

Window movement now transfers framebuffer spans in machine words, converting packed 16/24-bit VESA pixels in a RAM row buffer first. The cursor is placed before window transfers, and its footprint is protected while the rest of the scene is copied. Queued mouse motion with unchanged buttons is processed together before repainting; button transitions, input resets and keyboard ordering remain boundaries. Each movement still updates clamping/capture state, so input is not replaced with an unchecked summed displacement.

`python tools/window_drag_qemu.py --disk IMAGE --output target/drag.json --check` uses a disposable copy to measure burst-drag catch-up and sample cursor visibility. On this host's Cirrus/VESA QEMU test, median catch-up improved from about 122 ms to 50 ms, and missing-cursor samples fell from 90/98 to 0/16. These include input injection/QMP overhead and are not display frame-rate guarantees. Release builds include the cursor visibility regression check.
