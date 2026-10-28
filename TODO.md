# Roadmap

## Next priorities

- [x] Alt+Tab / Shift+Alt+Tab window switching, Alt+F4 close requests and Ctrl+Esc Apps keyboard navigation.
- [x] Queued desktop application file operations with a block-bounded worker and visible I/O activity.
- [ ] Extend queued I/O to executable loading and kernel shell filesystem commands.
- [ ] Smaller application layouts and richer shared focus/accessibility conventions.
- [x] Process listing/control for real desktop jobs through Terminal `ps` / `kill PID`.

## Available now

- BIOS boot, IDE/BIOS boot-drive storage and bootable installation.
- FAT16 folders, file deletion/rename and bounded recursive copy/move.
- Protected flat/ELF32 programs, writable ELF data/BSS, private heaps, stack guards and timer preemption.
- Desktop with a top taskbar, off-screen clipping, minimize/restore and opt-in resize/maximize.
- Files with shared selections, context menus and conflict handling.
- Notepad with 64 KiB documents, scrolling, selection, four undo steps and Open/Save dialogs.
- Terminal using the shared shell, text-app handoff, scrollback and clipboard.
- Settings for persistent desktop/title colours; ACPI shutdown with a halted fallback.
- Disposable QEMU checks for persistence, isolation, UI behaviour and memory cleanup.

## Later work

- [ ] Larger executable files, growable stacks and stronger execute permissions.
- [ ] Streaming/random-access file APIs beyond the current 64 KiB whole-file limit.
- [ ] Pipes and shell scripting.
- [ ] Unicode and loadable/proportional fonts.
- [ ] Shared driver interfaces; native AHCI/SATA, USB and audio backends.
- [ ] Graphics acceleration and broader hardware compatibility.
- [ ] FAT32/larger volumes when needed, while retaining FAT16 support.
- [ ] UEFI boot; networking after the core desktop/storage work.

## Reliability

- [ ] Reduce mutable-static references and remaining compiler warnings.
- [ ] Extend malformed-input testing and harden executable/user-memory permissions.
- [x] Read-only FAT16 audit, confirmed orphan cleanup and interrupted-write/cleanup tests.
- [ ] Journal or broader metadata recovery and interrupted installation handling.
- [ ] Widen block/partition geometry beyond u32 sector counts.

Keep generated artifacts out of Git. Update the relevant docs and run checks appropriate to each change before marking it complete.
