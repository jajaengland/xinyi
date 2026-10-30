# Roadmap

## Selected major milestones (in order)

1. [x] Initial audio system: AC97 PCM output, media permissions and WAV Player with bundled dndt recording.
   - [x] Voice system sounds, shared info/notification clip, volume/mute/preview, saved settings and playback debounce.
2. [x] Graphics and fonts foundation: shared pixel metrics, proportional UI text, wrapping/clipping, faster glyph drawing and Fonts demo.
3. [x] Desktop shell redesign: bottom taskbar, searchable launcher, desktop shortcuts, clock, sound popup and new window chrome.
4. [ ] Application system (installation, metadata, associations and permissions UX).
   - [x] Filesystem manifests, metadata-driven launcher, restrictive loader profiles, TXT/WAV/data associations and bundle-preserving OS installation.
   - [x] Per-app folders and confirmed Files installation/removal, staged publication, asset copying and running-app refusal.
   - [ ] Updates, permissions review UI, launcher pagination and app package distribution.
   - [ ] App manager for install/update/remove, default-app selection and permission controls.

## Next priorities

Networking development is deferred; it remains off by default behind the developer command. Native USB input and broader laptop audio support are also deferred.

- [ ] Desktop icon rearrangement, multiple selection and context menus.
- [ ] Recycle bin and restore before permanent deletion.
- [ ] Files icon/compact views, sorting and clearer transfer progress.
- [ ] Safe app updates/recovery and launcher pagination beyond 16 apps.
- [ ] Further app isolation and kernel hardening before downloadable apps.

- [x] Alt+Tab / Shift+Alt+Tab window switching, Alt+F4 close requests and Ctrl+Esc Apps keyboard navigation.
- [x] Queued desktop application file operations with a block-bounded worker and visible I/O activity.
- [x] Yielding desktop executable reads, cancellation, progress and guarded launch publication.
- [ ] Extend queued I/O to kernel shell filesystem commands and catalog refreshes.
- [ ] Smaller application layouts and richer shared focus/accessibility conventions.
- [x] Process listing/control for real desktop jobs through Terminal `ps` / `kill PID`.

## Available now

- BIOS boot, IDE/BIOS boot-drive storage and bootable installation.
- FAT16 folders, file deletion/rename and bounded recursive copy/move.
- Protected flat/ELF32 programs, writable ELF data/BSS, private heaps, stack guards and timer preemption.
- Desktop with a bottom taskbar, off-screen clipping, minimize/restore and opt-in resize/maximize.
- Files with shared selections, context menus and conflict handling.
- Notepad with 64 KiB documents, scrolling, selection, four undo steps and Open/Save dialogs.
- Terminal using the shared shell, text-app handoff, scrollback and clipboard.
- Settings for persistent desktop/title colours; ACPI shutdown with a halted fallback.
- Disposable QEMU checks for persistence, isolation, UI behaviour and memory cleanup.

## Later work

- [x] Larger static ELF executable files (1 MiB), packaging/installation and isolated BigDemo.
- [ ] Growable stacks and stronger execute permissions.
- [x] Initial positional file APIs: 1 MiB files, 16 KiB chunks, copy-on-write edits and read-only Viewer.
- [ ] Efficient staged streaming writers.
- [x] Initial immutable read-only/isolated launch profiles, legacy/new syscall enforcement, PTY bypass prevention and Viewer defaults.
- [ ] Per-file grants, untrusted-by-default launch UX, executable authenticity and resource quotas before exposing networking to untrusted apps.
- [ ] Pipes and shell scripting.
- [x] Native proportional UI fonts and shared pixel layout.
- [ ] Unicode and loadable/scalable fonts.
- [ ] Shared driver interfaces; native AHCI/SATA, USB and audio backends.
  - Broader audio support deferred by request; real-hardware report: HP Pavilion x360 laptop, built-in speakers. Identify controller/codec before choosing HDA/DSP work.
- [ ] Graphics acceleration and broader hardware compatibility.
- [ ] FAT32/larger volumes when needed, while retaining FAT16 support.
- [ ] UEFI boot.
- [x] Initial RTL8139 Ethernet, bounded ARP/IPv4/UDP/DHCP/ICMP, lease renewal, status and background ping.
- [ ] DNS lookup, TCP and downloads; explicit socket permissions/quotas before application networking.
- [ ] Additional wired controllers and Wi-Fi.

## Reliability

- [ ] Reduce mutable-static references and remaining compiler warnings.
- [ ] Extend malformed-input testing and harden executable/user-memory permissions.
- [x] Read-only FAT16 audit, confirmed orphan cleanup and interrupted-write/cleanup tests.
- [ ] Journal or broader metadata recovery and interrupted installation handling.
- [ ] Widen block/partition geometry beyond u32 sector counts.

Keep generated artifacts out of Git. Update the relevant docs and run checks appropriate to each change before marking it complete.

- [x] Desktop Files drag/drop: multi-selection, folder targets, Copy/Shift-move, feedback, cancellation and clipboard independence.
- [x] Persistent wallpaper file/folder icons, Files/wallpaper transfers, and drops into Notepad/Player.
