# Roadmap

## Next priorities

- [ ] Files: rename, copy/move, richer selection and right-click menus.
- [ ] Shared GUI controls: buttons, text fields, scrollbars and dialogs; improve font readability.
- [ ] Program loading/memory: ELF executables, writable data/BSS and a user-space heap.
- [ ] Desktop polish: window resizing, keyboard shortcuts and saved preferences.

## Working foundations

- [x] BIOS boot and bootable installation with explicit destructive-operation previews.
- [x] Protected user programs, private address spaces, stack guards and fault recovery.
- [x] Timer preemption, saved CPU/FPU contexts, sleep and input waits.
- [x] FAT16 files/folders, IDE PIO/LBA48 and BIOS boot-drive storage.
- [x] QEMU/VESA framebuffer graphics, PS/2 input and a top-taskbar desktop.
- [x] Filesystem Apps launcher, shared-shell terminal sessions and automatic text-app handoff.
- [x] Files, Notepad, EDIT, shared clipboard and text selection.
- [x] Off-screen window clipping, minimizing and taskbar restoration.
- [x] Disposable QEMU integration tests for persistence, isolation, window lifecycle and memory cleanup.

## Later work

- [ ] Larger text files and richer filesystem APIs.
- [ ] Process listing/control, pipes, shell scripting and asynchronous I/O.
- [ ] Unicode and loadable/proportional fonts.
- [ ] Shared driver interfaces; native AHCI/SATA, USB and audio backends.
- [ ] Native graphics acceleration and broader hardware compatibility.
- [ ] FAT32/larger volumes when applications need the capacity; retain FAT16 support.
- [ ] UEFI boot support.

## Reliability and security

- [ ] Reduce mutable-static references and remaining compiler warnings.
- [ ] Harden user-memory/executable permissions and extend malformed-input testing.
- [ ] Improve filesystem metadata recovery and installation interruption handling.
- [ ] Widen block/partition geometry beyond the current u32 sector-count limit.

Build and run the checks relevant to each change, keep generated artifacts out of Git, and update the documentation when behavior changes. Mark features complete only after implementation and verification.
