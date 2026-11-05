# Focused security review

This records incremental review evidence, not a completed security audit or a claim that Xinyi can safely run untrusted downloaded applications. Networking remains disabled by default.

## First pass: syscall buffers and deferred ownership (2026-11-04)

Reviewed the syscall dispatch permission gate, current-address-space checks, ELF range validation, heap mapping changes, clipboard copies, filesystem request copies and the disk worker's process context/lifetime handling. Relevant code is in `interrupts.rs`, `user_memory.rs`, `usermode.rs`, `elf.rs`, `paging.rs`, `clipboard.rs`, `files_api.rs`, `directory_api.rs`, `disk_io.rs` and the I/O/termination paths in `multitask.rs` under `src/`.

Observed in the checked source:

- Ring-3 calls pass the profile gate before scheduler interception. The disk worker checks the owning profile again before dispatching a deferred filesystem request.
- User-memory validation requires the active process and matching CR3. Range arithmetic uses checked addition, and output ranges require writable regions. Heap syscall access ends at the logical program break, including inside a mapped page.
- Filesystem request metadata and paths are copied before interrupts are enabled. Write data goes into bounded kernel scratch; successful reads copy back before returning. These observations cover the examined filesystem APIs, not every syscall.
- A process with pending I/O does not resume user code. The worker installs that process's address space, launch profile and working directory on each step. Termination of the active I/O owner is deferred until its operation completes; the worker stack cannot be freed while busy. These lifetime observations are from source review, not a new exhaustive cancellation/preemption test.

No new defect was confirmed in this bounded pass. No kernel behavior was changed as a result of it.

### Added runtime evidence

`programs/memory_check/main.rs` has a `bounds` mode. Run it with:

```text
python tools/memory_boundaries_qemu.py --disk PATH/TO/xinyi.img
```

The harness uses a private disk copy and installs its helper under `/TOOLS`, keeping it out of the app launcher. It runs twice from the physical console and twice as a desktop Terminal child. Each run checks:

- Null/kernel addresses, arithmetic wraparound, stack guard crossings and stack-top crossings are rejected by console/clipboard input calls.
- Oversized console writes are rejected; a zero-byte write accepts an unused pointer without dereferencing it.
- Clipboard output rejects read-only code, a stack-top crossing and a range crossing the logical heap break.
- Rejected clipboard sets preserve the existing clipboard. Rejected output copies leave valid destination bytes unchanged, and a subsequent valid copy succeeds.
- Shrinking the heap invalidates syscall access to released pages and to bytes past the logical break in a still-mapped page.
- All four helper processes exit normally, no unexpected process/kernel fault is reported, and free-frame counts return to baseline after foreground runs and desktop teardown.

Passed locally against the release kernel and BIOS-booted image on 2026-11-04. This does not exercise every syscall, restricted profile, concurrent I/O cancellation or real hardware. Logs are retained in `target/smoke/memory-boundaries-debug.log` and `memory-boundaries-qemu.stderr`. The release builder includes this check; the quick GitHub kernel smoke workflow does not.

## Second pass: disk-operation owner termination (2026-11-04)

Extended the existing `tools/io_qemu.py` scenario rather than introducing another disk-worker harness. The test now queues a legacy write behind a different process's recursive copy, confirms the request was queued, and kills its owner before the active copy finishes. It checks that the dead owner's request never starts, its destination file is absent, the other process finishes its copy with exact contents, Counter progresses, and a replacement process can reuse the vacated taskbar slot.

The existing remainder of the scenario checks active-owner termination: stop the copying process, require a deferred-termination trace, and require its disk completion before its forced-exit trace. A separate queued request must reject a changed volume. The entire scenario checks serialized ownership and free-frame balance after desktop teardown.

Added a queued-request debug trace at the point where the scheduler records `job.io`, so a test can distinguish a submitted request from one that actually reached the worker. This does not change cancellation or filesystem behavior. Updated the test's launch matcher to recognize current per-app bundle paths.

The focused mode seeds two 64 KiB source files offline and skips the broader save/editor/timer workload, while retaining the owner-termination, stale-volume, exact-copy, Counter progress and frame-balance assertions:

```text
python tools/io_qemu.py --disk PATH/TO/xinyi.img --native --termination-only
python tools/io_qemu.py --disk PATH/TO/xinyi.img --termination-only
```

Validation: both focused native IDE and BIOS IDE cases passed locally on 2026-11-04. Initial broader runs on both backends exceeded the earlier 90-second wait during copy/read completion; they did not reach the new termination assertions. The trace showed the copy completing, and the harness now allows 300 seconds per wait. Those initial runs are not recorded as passes. The focused mode does not replace the broader release suite. Latest focused evidence is retained in `target/io-review-native-debug.log` and `target/io-review-bios-debug.log`.

No new kernel defect has been confirmed in this pass. This scenario does not yet cover forced faults, every queued syscall type, all device backends or crash/power-loss recovery.

## Image support follow-up (2026-11-05)

The new pixel syscall checks the calling process and window handle before copying a 20-byte request. Rectangles use checked bounds/product arithmetic and are capped at 2048 packed RGB pixels. Request and source data are copied into owned kernel scratch pages; invalid ranges or colour high bytes reject the entire tile before surface modification. Scratch pages are freed before returning. Image parsing and scaling stay in the read-only userspace Image app, with a 768 KiB file cap and bounded dimensions. This is a BMP subset, not a general-purpose hardened image codec.

Source inspection also identified a foreground rendering limit: the u32 dirty-row mask cannot represent rows 32/33 of a valid 544-pixel client. It is now u64. The new runtime probe covers that tall client, bad rectangles, guarded/kernel pointers, invalid colour atomicity, foreign handles and ReadOnly/Isolated callers. Desktop probing uses a real second application's handle. Standard VGA and BIOS VESA tests verify actual framebuffer colours as well as client surfaces, file/desktop associations, drops, fit/native/panning/maximise, malformed-file handling, source preservation and frame balance. Host decoder tests include padding, orientation, integer/header failures and mutated inputs.

Integration testing with 17 app candidates exposed an obsolete 16-candidate limit in SDK file associations. That limit now matches the 64-app launcher, with a separate 256-entry root scan bound. The image integration test retains the extra app so this case remains covered.

## Remaining review work

The later launcher/update work exposed a supervisor-stack overflow when expanding the catalogue to 64 entries. Temporary catalogue construction in refresh, shell listing and app installation now uses owned kernel pages. The guard was retained; local QEMU coverage boots and navigates a 55+ app catalogue, exercises foreground and worker update paths, and checks frame balance. This is a regression found during the new feature work, not evidence of an earlier exploit. Versioned updates refuse capability expansion, bind their retained backup to an explicit app ID record, and validate rollback images before unpublishing the current app. Ordered filesystem renames still do not provide a power-loss-atomic transaction.

- Enumerate all syscall handlers and pointer/length fields, including nested graphical requests, audio, PTYs, drag delivery and app installation; record coverage individually.
- Exercise worker cancellation/owner termination while other processes run, including faults and queued-but-not-started requests. Recheck interrupt/preemption assumptions and resource cleanup across all exit paths.
- Extend restricted-profile and cross-process handle/ownership checks, including newly added calls. Default/trusted launches and manifests are policy choices, not executable authentication or a download sandbox.
- Review parsers and filesystem failure paths separately. The current 32-bit paging setup lacks execute-disable protection for user data/stack pages; writable/executable ELF segment rejection does not supply hardware NX.
- Keep ABI/layout inventory and compatibility tests as the next distinct foundation task.
