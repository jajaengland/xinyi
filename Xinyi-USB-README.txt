Xinyi BIOS USB image

Flash Xinyi-USB.img as a raw disk image (for example Rufus DD mode or balenaEtcher).
Choose the correct USB: flashing replaces its partition table and existing contents.
Boot in legacy BIOS mode. This image contains the kernel and FAT16 /APPS files.
Rufus: Device = your USB, Boot selection = Disk or ISO image, SELECT Xinyi-USB.img, then START.
Raw .img files are written as disk images; choose DD mode if Rufus offers a mode choice.
Keep the image's existing MBR/FAT16 layout; do not extract its contents or create a FreeDOS drive.
In firmware enable Legacy/CSM boot where available (this may require disabling Secure Boot).
UEFI-only machines without CSM cannot boot this image.
At the Xinyi prompt, type desktop. This runs from the USB; installing to an internal disk is separate.
The image is 16 MiB; remaining USB capacity is unallocated after raw flashing.
MEMDEMO in Apps demonstrates ELF globals/BSS and heap-backed Rust collections.
The boot USB mounts automatically as drive 0. Type apps, edit note.txt, run snake, or run tetris.
Graphical editor: notepad note.txt. Drag its title bar; close requests check unsaved changes.
Notepad: drag/Shift+arrows to select; Ctrl+A/C/X/V selects all/copies/cuts/pastes using a shared clipboard.
Terminal: type terminal or choose TERMINAL in Apps; it runs the same shell as the physical console.
FILES in Apps browses folders and opens files. Terminal history: Up/Down; scrollback: Shift+PgUp/PgDn; Ctrl+Q closes. Each terminal has its own directory.
Files/Notepad resize from the lower-right grip; Terminal can show up to 33 rows. Use the title-bar maximize/restore button or double-click to fill the desktop below the taskbar.
shutdown powers off or safely halts after flushing; use shutdown confirm from the desktop terminal.
Notepad Open/Save As browses folders; Terminal has a right-hand scrollbar for history.
Settings in Apps changes desktop/title colours; Save keeps them on the boot drive.
Settings also controls system voice volume/mute; Test voice plays the info/notification recording.
FONTS compares fixed/proportional text: Space toggles wrap width, +/- adjusts it, Escape closes.
PLAYER supports PCM WAV/test audio, but sampled sound currently requires Intel 8086:2415 AC97 hardware.
Modern HDA/USB audio is unsupported; absent audio hardware leaves the OS usable but without voice sounds.
Files supports Ctrl/Shift selection, right-click actions and folder copy/move across windows.
Editor: Ctrl+S save, Ctrl+Q quit. Notepad supports 64 KiB ASCII documents, a scrollbar and four undo steps; EDIT/shell text commands and clipboard retain a 1024-byte limit.
Type graphics for the 800x600 framebuffer test, arrows to move, Escape to return.
Type mouse for the PS/2 arrow pointer, two clickable buttons and right-click menu.
Type desktop for concurrent graphical apps and a top taskbar. Click running apps to switch focus.
Apps opens text programs in Terminal automatically. The _ button minimizes; taskbar buttons restore.
Windows can move partly off-screen; the top taskbar remains accessible.
Type multitask for independent counter windows: Q quits, C crashes the focused app, Escape exits.
Ctrl+Alt+K force-stops the focused app; multitask hog tests an app that never yields.
Run text programs in Terminal to keep other windows running. Ctrl+C interrupts; Ctrl+Shift+C copies.
BIOS VESA 2/3 RGB565/24/32-bit modes are supported; bios prints diagnostics.
To install: disks, then install NUMBER for the desired IDE destination, inspect the preview, then install confirm.
Installation copies the apps from the mounted source. Drive 0 is not a format/install destination.
Use unmount before removing the USB. Firmware writes are read back but have no portable cache-flush guarantee.
Support is for the BIOS boot drive, without native USB hotplug, non-boot USB, AHCI, or UEFI support.
Input requires PS/2-compatible keyboard/mouse or firmware legacy USB emulation; no native USB input driver.
Rufus reference: https://github.com/pbatard/rufus/wiki/FAQ
Verified with QEMU USB-only boot and Cirrus VESA.

Build: current working source, 29 October 2026. Includes all 14 standard apps, Fonts and voice sounds.
Verified in QEMU USB-only BIOS boot; compatibility with your particular physical PC is not yet tested.
