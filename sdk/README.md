# Application SDK

Applications are separate `/APPS/*.BIN` or `*.ELF` files running as protected 32-bit ring-3 programs. Flat binaries retain their 64 KiB read-only image and 16 KiB stack. Static ELF32 apps add initialized writable globals, zeroed BSS and a bounded private heap. Rust `alloc` is available for ELF programs; `std`, dynamic linking, TLS and user threads are unavailable.

## Build an application

```rust
#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;

fn main() -> u32 {
    xinyi::print("Hello from Xinyi!\n");
    0
}
```

Adjust the SDK path for the source location. The SDK supplies entry/exit, syscall wrappers and a panic handler (exit code 101). From the repository root:

```powershell
python tools/build_program.py programs/hello/main.rs --output target/programs/hello.bin --force
```

The builder uses offline Cargo and extracts a flat image linked at `0x40000000` from the compiled ELF. `sdk/user.ld` rejects writable globals/BSS. Add an app to `programs/apps.txt` for release packaging, or put the resulting `.BIN` on a FAT16 application volume. `run NAME` searches the current directory and `/APPS`; Apps lists regular `.BIN`/`.ELF` files dynamically. Bare names prefer `.BIN` when both formats share a stem.

Physical-console `run` is synchronous. Desktop launches are scheduled; text apps get a Terminal automatically and graphical apps own separate windows. `spawn` retains legacy diagnostic records; `ps` and `kill PID` also report/control actual desktop processes.

## Syscalls

The experimental ABI uses `int 0x80`: EAX = number, EBX/ECX/EDX = arguments, EAX = result; other general registers and flags are preserved on returning calls.

| Number | Call | Result |
| --- | --- | --- |
| 1 | `exit(code)` | Terminates the current process |
| 4 | `write(fd, pointer, length)` | Byte count or a negative error |
| 20 | `getpid()` | Current process ID |
| 50 | `read_key(wait)` | Packed key press, zero when polling without input, or negative error |
| 51 | `ticks()` | Wrapping 100 Hz counter |
| 52 | `sleep_ms(ms)` | Wait using PIT interrupts; request 0..60000 ms |
| 53 | Console control | Clear/colour/cursor and size queries |
| 54 | `draw_text(x,y,colour,text)` | Replace a row span without moving the cursor or scrolling |
| 55 | `draw_cells(x,y,cells)` | Replace a row span with a separate character/colour per cell |
| 56 | `beep(frequency,ms)` | Play a bounded PC-speaker tone and stop before returning |
| 57 | `read_file(path, &mut [u8;1024])` | Read a mounted-drive text file |
| 58 | `save_file(path, bytes)` | Create/replace a mounted-drive text file |
| 59 | `arguments(&mut [u8;128])` | Copy raw argument bytes; returns byte count |

Descriptors 1 and 2 both use the text console. Each write accepts at most 1024 bytes within readable mapped segments, the current heap break or the user stack. The kernel validates the active address space and copies the complete buffer before output. A supported descriptor with zero length succeeds without reading the pointer. Errors are `-9` (descriptor), `-14` (buffer), `-22` (length), and `-1` (unavailable/unknown call). SDK `write` returns the signed result; `print` ignores it.

The `count` example prints numbers using a stack buffer. The `check_io` and `pid` examples exercise buffer validation and process IDs. `tools/programs_qemu.py` tests the hello, buffer-validation and process-ID examples on disposable images.

## File associations and manifests

`associations::open(path, volume)` resolves handlers from bounded sibling `.APP` manifests in `/APPS`, preserving the caller's working directory. TXT and similar documents use Notepad; WAV uses Player; DAT/RAW/HEX use Viewer in the bundled manifests. It returns 0 when a launch is queued, -2 for no handler, -17 for ambiguous handlers, -27 for an oversized candidate set, -116 for a changed drive, or other normal read/launch errors. Explicit BIN/ELF files return -22; callers must choose executable launching themselves. The helper requires launch authority and reads metadata through normal volume-checked file calls.

`launch_on(command, volume)` uses syscall 101 with command pointer, byte length (1..128), and expected filesystem epoch. It checks the epoch before queuing or copying a launch request, so an association cannot accidentally launch from a replacement drive. Existing call 71 retains its reserved-zero third argument and existing behavior. Restricted profiles cannot call 101. The loader applies any manifest access restriction beside the resolved executable and intersects it with filename defaults/requested flags. A manifest cannot grant access or redirect an executable path. The pure `app_manifest` SDK module exposes the same bounded parser used by the kernel. Format and limits are in `programs/manifests/README.md`.

## Text input and drawing

Graphical text uses the allocation-free `xinyi::text` module, shared with the kernel. `text::width(bytes, Font::Ui)` measures proportional pixel advances; `Font::Mono` measures 8-pixel cells. `fit`, `ellipsis`, `hit_test`, `Lines` and `height` provide matching prefix fitting, caret positions and word-wrap layout. Line height is 16 pixels; Ui uses tabular digits. Printable ASCII is supported; layout helpers map unsupported bytes to `?`, while graphical syscalls reject unsupported content.

`window::Draw::label(x,y,width,color,bytes)` adds draw kind 2: an ellipsized proportional label within a 16-pixel-high box. `Draw::paragraph(x,y,width,height,color,bytes)` adds kind 3: proportional word wrapping, explicit newlines and hard breaks for long words, clipped strictly to the box. Both retain the 32-byte command structure and 96-byte text limit; each box must fit the client surface. Kind 3 permits LF; kind 2 and legacy kind 1 accept printable ASCII only. All command structures and copied strings are validated before any pixels in a batch change. Existing `Draw::text` stays fixed-width and shared text inputs retain cell-based editing. Shared GUI buttons measure, centre and clip their proportional labels automatically.

`read_key(true)` waits; `read_key(false)` polls. The low 16 bits contain ASCII or a navigation code; bits 16..19 hold Shift/Ctrl/Alt/Caps. Enter is 10, Backspace 8, Escape 27; navigation keys are `0x100..0x109`, F1..F10 `0x120..0x129`. Use `xinyi::key` constants. Releases update decoder state without returning key events.

The text screen is 80×25 cells. Console call 53 uses EBX operation: 0 clear (ECX colour), 1 move (ECX `x | y<<16`), 2 set colour, 3 query dimensions, 4 query cursor, 5 show/hide cursor. Dimensions/cursor return `x | y<<16`. Colours use VGA attributes; positioned drawing validates the entire row span and does not scroll.

Call 54 takes EBX `x | y<<8 | colour<<16`, ECX text pointer, EDX length. Call 55 takes EBX `x | y<<8`, ECX cell pointer, EDX count; each cell is a character byte and attribute byte. Control bytes draw as spaces. Calls return byte/cell count or -22 for invalid bounds/colour and -14 for an invalid buffer.

`ticks()` wraps at 32 bits and runs at 100 Hz; compare with `wrapping_sub`. `sleep_ms` accepts 0..60000 ms, rounds nonzero waits to ticks plus an alignment tick and deschedules scheduled jobs. Foreground waits halt with interrupts enabled. `beep` accepts 37..20000 Hz for at most 1000 ms; zero frequency/duration stops sound. It uses the PC speaker, not a native audio driver.

## Legacy text files and arguments

The release includes `/APPS/EDIT.BIN`, launched with `edit [FILE]` or `run edit [FILE]`. The editor uses the current 1024-byte ASCII text limit, preserves its document on read/save errors, and prompts for unsaved changes and Save As overwrites. Save is to the mounted disk; no silent RAM fallback occurs.

Raw read call 57 takes EBX path pointer, ECX path byte length (1..105) and EDX output pointer to a writable 1024-byte user buffer. Raw save call 58 takes EBX path pointer, ECX `path_length | (data_length << 16)` and EDX data pointer; data length is 0..1024. Both return byte counts or negative errors: -2 not found, -13 read-only, -19 no mounted drive, -22 invalid name/length, -27 oversized file, -28 full disk, -14 invalid pointer, -5 I/O/metadata error. Reads copy only actual file bytes. Saving a nonexistent filename creates it; a failed new-file write can leave an empty directory entry, while replacement uses the existing copy/flush ordering. No power-loss journal is provided.

Call 59 takes EBX output pointer and ECX capacity. It copies the current raw argument bytes without a NUL terminator and returns the count, or -22 for insufficient capacity/-14 for an invalid writable range. Outputs may use the user stack, writable ELF segments or allocated heap. A zero-byte copy ignores the pointer. Arguments are at most 128 bytes; filenames still follow ASCII 8.3 path rules.

## Windows and scheduling

Calls 60..64 provide `window::open`, `event`, `draw`, `present` and `close`. The default client is 640x400. `window::open_sized(title,width,height)` uses call 65, packing width/height into EDX (low/high 16 bits). Sizes must be 160..792 by 80..544 with width divisible by 8 and height by 16. Draw coordinates are checked against that client size. Titles are 1..48 printable ASCII bytes; handles belong to the current PID. See `programs/counter/main.rs` for a small example and `programs/notepad/main.rs` for editing.

PIT interrupts preempt user code. Scheduled `sleep_ms`, `window::event(...,true)` and `yield_now()` (66, zero arguments) return control to the dispatcher. Desktop filesystem calls park the caller in a serialized queue and yield between completed disk-block slices; other syscalls run to completion. Queued file requests retain their submission volume generation and return -116 if that volume changes before execution. The scheduler preserves x87/MMX state and private ELF globals/heap; SSE/AVX remain unavailable.

The desktop supports mounted-drive file calls and concurrent graphical apps. A windowless app requesting console/key/speaker services gets a Terminal session automatically, retaining its process state. Console/speaker services return -16 for apps that already own graphical windows. Use call 71 to queue child launches. The `multitask` diagnostic runs Counter windows; its legacy file calls 57/58 return -16 outside desktop mode.

## Clipboard and terminal services

| Call | SDK | Contract |
| --- | --- | --- |
| 67 | `clipboard_set(bytes)` | EBX pointer, ECX length 0..1024, EDX zero; accepts printable ASCII, tab, CR and LF. Zero clears. |
| 68 | `clipboard_get(output)` | EBX writable user pointer, ECX capacity 0..1024, EDX zero. Zero capacity queries length; insufficient capacity returns -28 with no partial copy. |
| 69 | Retired | Returns -38; Terminal uses the shared shell through PTY calls. |
| 70 | `working_directory(output)` | EBX writable 105-byte user buffer, ECX capacity >=105, EDX zero; returns path byte count. |
| 71 | `launch(line)` | EBX printable-ASCII `APP [ARGS]` pointer, ECX length <=128, EDX zero; queues a desktop launch and yields. |

Clipboard writes are validated/copied before replacing the shared value. Errors include -14 invalid range and -22 invalid length/content/reserved argument. The clipboard survives program close, with no on-disk persistence. PTY output uses bounded screen/history storage without a per-command output cap. Interactive text children use console calls 4/50/53..56 inside their terminal window.

Desktop jobs save/restore a checked canonical working-directory snapshot around each quantum. A terminal's `cd` changes only its own snapshot; a queued launch copies that directory and arguments into kernel-owned storage before yielding. Desktop file calls run through one serialized I/O worker. Terminal line editing and non-disk commands remain live, while filesystem commands wait for the worker and then execute synchronously. Launch succeeds as a request (0); subsequent loading errors appear in desktop status. No free slot returns -24, unavailable/busy desktop returns -16, and invalid arguments return -22. A child starts with its parent's directory; Apps launches use the physical shell's starting directory. The physical shell’s `terminal` command opens the scheduled desktop and starts Terminal.

## PTY and directory calls

PTY handles belong to their creator. All calls require an active user image and matching CR3. Calls 72..77 are exposed as `xinyi::pty`; invalid handles return -9, unavailable/busy sessions -16, capacity failure -28 and invalid arguments -22.

| Call | Arguments |
| --- | --- |
| 72 | EBX operation, ECX handle, EDX 0. Operations: 0 open (handle 0), 1 close if idle, 2 force close, 3 foreground PID, 4 closed flag, 5 invalidate display cache, 6 retained history rows (0..64), 7 output-scroll count (reset by console clear), 8 viewport rows in EDX (25..34; clears selection/cache). |
| 73 | EBX session, ECX owned 640..792-wide window, height 400..544; EDX scrollback rows 0..64. Set the PTY viewport rows to match the client height. |
| 74 | EBX session, ECX key/modifier event, EDX 0. |
| 75 | EBX session, ECX readable paste pointer, EDX byte length <=1024. Rejects oversized paste atomically. |
| 76 | EBX session, ECX anchor, EDX endpoint, both 0..`viewport_rows*80` cell offsets. Freezes the selected view. |
| 77 | EBX session, ECX 0 copy selection / 1 clear selection, EDX 0. Copy uses the shared clipboard. |
| 78 | EBX readable path, ECX path length low16 / entry offset high16, EDX writable 776-byte `directory::Listing`. |
| 79 | EBX readable path, ECX path length, EDX 0; changes only the caller’s directory. |
| 80 | EBX readable path, ECX path length, EDX 0 create folder / 1 delete file / 2 remove empty folder. |

Directory paths are 1..105 printable ASCII bytes and retain FAT 8.3/depth rules. Listings contain up to 32 initialized entries plus returned count and total, excluding `.` and `..`. Each entry is 24 bytes: `[u8;12]` name, u32 name length, u32 file size, u32 directory flag (bit0). Files adjusts its visible row count to the client height. Errors include -14 invalid buffer, -17 exists, -20 not directory, -21 not file and -39 directory not empty. `directory_check` exercises paging, invalid ranges, create/delete and failure preservation.

## Desktop lifecycle and current limits

The desktop supports four root application windows and up to four attached text children in eight internal job slots. A text app launched from Apps receives a Terminal at its first console request; its existing process is attached rather than reloaded. A missing Terminal or failed allocation/open stops the waiting child cleanly. Applications need no executable-name registry or GUI/text classification.

Desktop title bars provide minimize and close, with maximize/restore for resizable apps. Minimize keeps the process, surface, pending document and scheduler state alive; another visible window takes input focus. Hidden apps continue executing and drawing into their retained surfaces. Clicking the taskbar restores and brings the window fully into view. Moving partly beyond screen edges does not change client coordinates or buffer sizes: the kernel clips the final surface copy. The bottom taskbar remains accessible. The synchronous foreground API has no taskbar minimize control.

Static ELF executable files now support up to 1 MiB; flat images remain capped at 64 KiB. User stacks remain at 16 KiB plus a guard page. ELF memory spans and heaps are capped separately at 1 MiB each. Whole-file calls 88..90 and Notepad support 64 KiB files; legacy text APIs, shell text commands, EDIT and the shared clipboard retain their 1024-byte limit. Desktop filesystem calls are queued transparently: the calling app waits, while other apps run between disk-block slices. Returned results and buffer validation are unchanged. An active operation finishes before forced termination reclaims its owner; queued operations can be discarded. Desktop executable reads now use the same worker; individual firmware/controller waits and kernel shell filesystem commands remain synchronous. Classic 32-bit paging protects code from writes and isolates processes but does not prevent executing writable pages.

## File transfers

`directory::transfer(source, destination, operation)` operates within the mounted volume: 0 renames within one directory (files or unused folders), 1 recursively copies a file or folder, 2 moves a file or folder. Files may be up to 64 KiB, including executable images. Existing destinations are refused; no implicit overwrite occurs. Folder trees are bounded to 256 entries and eight path components, including the destination prefix. Moving an in-use folder is refused. Folder transfers preflight before copying; a failed copy may leave a partial destination. Source deletion starts only after the whole tree has been copied. Moving between directories copies first, then deletes the source; interruption or deletion failure can leave both copies. Normal filesystem failure/durability limitations still apply.

For delayed selections, keep `directory::volume()` and pass it to `transfer_on`. Changing/unmounting the volume invalidates old selections with -116. The SDK helper without a tag uses the currently mounted volume.

Raw call 81 takes EBX readable 220-byte request pointer, ECX 220 and EDX zero. Its C layout is `[u8;105]` source, `[u8;105]` destination, u16 source length, u16 destination length, u16 operation and u32 volume generation. For operations 0..2, both lengths are 1..105 and paths must contain printable ASCII. Call 82 (zero arguments) returns the mounted-volume generation. Calls require an active image; invalid ranges return -14, invalid lengths/options -22, unavailable volume -19, conflicts -17, in-use folders -16, read-only sources/destinations -13, oversized files -27 and insufficient memory -12.

`directory::action_on(path, operation, volume)` tags create-folder (0), delete-file (1) and remove-empty-folder (2) requests with the same volume generation. Raw call 81 operations 3..5 implement these actions; destination length must be zero. Untagged call 80 remains available.

## File selections and appearance

Call 83 takes EBX operation (0 set, 1 get, 2 conditional update), ECX request pointer and EDX 3468. `file_clipboard::Selection` has C layout: 32 paths of 105 bytes, 32 u16 lengths, 32 u8 flags (bit0 folder), u16 count, u16 cut, u32 volume and u32 version. At most 32 canonical absolute non-root paths are accepted, with no duplicate or overlapping ancestors. Set/update reads a validated user range; get requires writable user memory. Nonempty selections must match the mounted volume. Set/update returns a positive version; get returns zero. Update requires the previous version (-116 on a changed selection). Count zero clears it. This clipboard survives application close, is separate from the ASCII clipboard and resets on reboot.

Call 84 takes EBX operation (0 get, 1 apply, 2 save and apply), ECX pointer and EDX 8. `appearance::Theme` is two u32 values: desktop RGB and active title RGB, each 0..0xFFFFFF. Get requires a writable user range. Invalid values return -22, invalid ranges -14; save propagates filesystem errors and leaves the current theme on failure. Apply affects the current session; Save writes the 14-byte ASCII `/GUI.CFG` format `RRGGBB RRGGBB\n` on the mounted drive. Startup loads a valid file; missing/malformed files use the default. Colours are independent of the text console palette.

For POINTER window events, `Event.code` contains the keyboard modifier snapshot at enqueue time, using the same modifier bits as KEY events. Motion coalescing preserves modifier changes.

## Shared GUI controls

`widgets::{Rect, Button, TextInput<N>, Scrollbar}` are allocation-free interaction models using client coordinates. Buttons capture a left press and activate on release inside; `reset(buttons)` cancels capture after RESET. Text fields support insertion, selection with Shift+arrows or Ctrl+A, cursor/Home/End, Backspace and Delete; oversized/invalid inserts leave the value unchanged. `gui::edit_key` adds shared-clipboard paste. `gui::{button,text_input,scrollbar,dialog}` paint controls; callers present after drawing.

Scrollbars need a rectangle at least 48 pixels tall. `range(max,page)` sets the maximum first-visible position and page size; `value` increases downward. Pass every pointer event to `pointer`, including release/captured motion, and call `reset` on RESET. Arrow clicks step one position, track clicks page, and thumb dragging clamps at both ends. Terminal adds this bar outside its full 80-column grid using a 656×400 client; existing 640×400 PTY surfaces remain supported.

`file_dialog::Dialog::new(save)` browses the caller's current folder without changing cwd. Feed KEY/POINTER/RESET events to `event`, paint with `paint`, and handle Pending/Cancel/Accept. Accept returns a 105-byte absolute path, length and volume generation. Validate that generation again before acting, including delayed overwrite confirmation. The picker supports folder double-click/Enter, Up/Alt+Up, scrollbar/PageUp/PageDown, a bounded filename/path field, Tab/Shift+Tab focus and Escape cancellation. The application owns reading/writing and overwrite policy; Notepad preserves its document on errors.

## Opt-in desktop resizing

`window::resizable(handle,min_width,min_height,max_width,max_height)` (85) enables the lower-right resize grip. Pack minimum width/height in ECX low/high16 and maximum width/height in EDX low/high16. Limits must contain the current size, align to 8×16 pixels, and stay within 160..792 by 80..544. Existing windows keep their size until an app accepts a request. The synchronous foreground API returns -16 for resizing; apps can continue with their initial layout.

A `window::RESIZE` event has proposed client width in `x` and height in `y`. Call `window::resize(handle,event)` (86: EBX handle, ECX packed width/height, EDX zero) before changing app layout. Only the latest matching proposal is accepted; stale/unrequested requests return -116, invalid sizes -22 and allocation failure -12. Failure leaves geometry and the surface intact. Success preserves overlapping pixels, initializes new space, replaces the old surface and requires repainting the new layout. Width/height changes do not map pixels into user space. Text drawing and `window::row` accept up to 96 characters within the actual client bounds. RESIZE is a proposal, so user code halfway through drawing can finish against the old geometry safely.

Resizable desktop apps also receive ordinary RESIZE proposals when users maximize or restore with the title-bar button/double-click. No additional app API is required. The frame fills the work area above the taskbar; its centered client respects the advertised maximum dimensions. A successful call 86 publishes the new placement; failure preserves the old frame/surface. Same-size placement changes reuse the surface. Fixed-size and foreground windows retain their existing controls.

Terminal uses `pty::viewport(handle,rows)` (control operation 8) for 25..34 rows after a successful vertical resize. Its 80-column text console remains compatible with existing games/editors; the expanded view includes history. Control 6 reports the scroll limit for that viewport; selection endpoints are bounded by `rows*80`. A viewport change resets scroll/selection and invalidates the cache.

## ELF programs and heap allocation

Build static ELF32/i386 apps with `python tools/build_program.py programs/memdemo/main.rs --format elf --output target/memdemo.elf --force`. The default remains `--format flat`; its linker still rejects writable globals/BSS. ELF builds use `sdk/user-elf.ld`, build Rust `core,alloc`, define `xinyi_elf`, and preserve compact ELF/program headers while dropping debug/symbol sections. Add `|elf` as the fourth field in `programs/apps.txt` to ship an `.ELF` app. An optional fifth field overrides the filename extension (`bin` or `elf`); Notepad uses `|elf|bin` to replace existing installations of NOTEPAD.BIN. Detection is by file contents, not filename extension. `run NAME` checks the exact cwd filename first, then `/APPS/NAME.BIN` and `/APPS/NAME.ELF`; explicit paths load directly.

ELF load segments must occupy distinct pages inside `0x40000000..0x40100000`; flags are readable RX, R or RW, and the entry must be in initialized executable bytes. The loader zeros memory beyond each segment's file bytes. PIE, interpreters, dynamic linking, TLS and constructors are unsupported. The existing Xinyi syscall ABI applies; this does not make Linux executables compatible.

ELF programs using collections supply an allocator and allocation-failure handler:

```rust
#![feature(alloc_error_handler)] // alongside no_std/no_main
extern crate alloc;
#[global_allocator]
static HEAP: xinyi::heap::Allocator = xinyi::heap::Allocator::new();
#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! { xinyi::exit(12) }
```

See `programs/memdemo/main.rs` for a complete `Vec`/`Box` example. The allocator is single-threaded per process, supports alignment and free-block coalescing, and returns allocation failure at the heap cap. Freed blocks are reused; the allocator retains pages until exit. Do not mix manual break changes with a live SDK allocator.

Raw call 87 takes EBX desired break (0 queries), ECX/EDX zero; it returns the break or -22 invalid bounds/arguments/-12 exhaustion. `heap::program_break` wraps it. The heap starts empty at `0x40100000` and ends no later than `0x40200000`. Growth allocates zeroed physical pages; failure leaves the old break/mappings intact. Shrink unmaps/frees whole pages. Syscalls reject buffers above the exact byte break; direct user accesses have page granularity, so the tail of the final page may remain accessible. All heap pages and private globals are reclaimed on exit or fault. Outputs can use writable globals/heap but cannot target code, read-only data, unmapped gaps or guards.

## Whole-file operations

`files::read(path, output)` and `files::save(path, bytes)` support files up to 65536 bytes, including empty files. `read_on`, `save_on` and `size_on` take an explicit volume generation obtained from `directory::volume()`. Keep that generation with an open document or accepted file-picker target to reject saving to a replacement mounted volume. The default wrappers use the current generation.

Raw calls 88 (read), 89 (save) and 90 (size) take EBX pointing to five consecutive `u32` fields: path pointer, path length, data pointer, buffer capacity/data length, volume generation. ECX is 20 and EDX is zero. Paths contain 1..105 printable ASCII bytes. Size requests require zero data pointer/length. Calls return byte count or a negative error. Read validates the entire writable capacity and copies only actual file bytes after a successful read; failures leave output unchanged. Save validates/copies the whole input before I/O and creates or replaces the named regular file. New-file saves publish the directory entry only after writing data; replacement follows the existing FAT chain ordering. No journal or power-loss rollback is provided.

Errors include -2 missing, -5 disk/metadata failure, -12 staging allocation failure, -13 read-only, -14 invalid user range, -19 no mounted drive, -22 invalid request/name/length, -27 file above 64 KiB, -28 insufficient read capacity/full disk and -116 stale volume. Legacy calls 57/58 retain their 1024-byte limit. Notepad uses the new APIs and ELF heap, with four undo steps and a document scrollbar; the shared text clipboard still holds at most 1024 bytes.


## Positional file access

Calls 91..94 add range reads, range writes, size and resize for regular files up to 1 MiB. SDK helpers are `files::read_at[_on](path, offset, output [, volume])`, `write_at[_on](path, offset, bytes [, volume])`, `stream_size[_on](path [, volume])` and `resize[_on](path, length [, volume])`. Each data call accepts at most 16 KiB, and reads return a short count or zero at EOF. Keep the volume generation with a selected file. These are pathname operations: successive calls observe the current file, not a stable snapshot or persistent open handle.

The raw request is six consecutive u32 fields: path pointer, path length, data pointer, length, volume generation, byte offset. ECX is 24 and EDX is zero. Call 93 requires zero data/length/offset; call 94 requires zero data/offset and uses length as the new file length. Invalid lengths/offset overflow/reserved arguments return -22, invalid memory -14, stale generations -116. Read failures leave user output unchanged. Successful writes return the input length; resize returns zero. All data and metadata operations use the desktop worker when scheduled.

Writes create missing files, preserve bytes outside the replaced range and zero-fill growth/gaps. Resize creates missing files, zero-fills growth and discards truncated bytes. A zero-byte write does not extend an existing file. Each mutation validates the full old chain and writes a separate replacement chain before publishing the directory entry, then releases the old allocation. Consequently each edit needs space for the entire replacement and its cost grows with file size; appending many tiny chunks is inefficient. There is no journal or multi-call transaction. Interrupted metadata updates may leave reclaimable orphans or damage that fsck reports and refuses to guess at. Existing calls 57/58 and 88..90, Notepad and recursive copy/move keep their previous limits; executable image limits are described below. Rename/delete can handle 1 MiB regular files.

`programs/viewer/main.rs` is a read-only hex/ASCII viewer using a 256-byte page buffer and the shared file picker. Ctrl+O opens a file; arrows, Page Up/Down and Home/End navigate. Viewer now receives the kernel read-only profile by default, in addition to its UI never requesting writes.


## Application access profiles

`run --read-only APP [ARGS]` and `run --isolated APP [ARGS]` select immutable kernel-owned profiles. Normal launches retain trusted behavior except Viewer (read-only), BigDemo (isolated) and Player (media); these conventional-name rules only remove authority and never grant it. Explicit isolated mode remains stricter than Viewer's default. No executable or sidecar file can request a higher profile. There is no code-signing/trust database or per-file grant mechanism in this version; unfamiliar apps must be launched with an explicit restricted mode.

`permissions::current()` wraps call 95 (all three arguments must be zero) and returns a capability mask: FILE_READ=1, FILE_WRITE=2, LAUNCH=4, SHARED=8 (shared clipboards/settings), SHELL=16 (kernel PTY control), AUDIO=32. Trusted returns 63, media 33, read-only 1 and isolated 0. Test capability bits rather than equality: the mask may grow. This is a query, not a permission-setting call. Basic own-window/console, time, bounded sound, arguments, heap, cwd query and theme query remain available. Read-only permits file reads, listing, its own cwd changes and volume-generation queries. Isolated denies those file services. Both restricted profiles deny all mutations, raw transfer requests, shared clipboards, child launch and all PTY control APIs; an app cannot create a shell and inject commands to bypass file restrictions. Unknown/new syscalls are denied until explicitly classified.

Denials return -13 before dereferencing arguments or parking disk work. The I/O worker checks again using the suspended owner's restored profile; profile state follows the foreground/scheduled process and its address space. Restricted children cannot launch further children. Kernel-created Terminal wrappers may display a restricted child's output, but the child does not own or control their shell sessions. Profiles are shown by `ps`; kernel-created window labels append `[RO]` or `[isolated]`. Profiles limit these kernel services, not kernel-vulnerability exploitation, CPU/memory exhaustion or every global resource. Read-only apps can read any file on the selected volume. Networking is disabled by default and exposes no application socket API.


## Larger executable files

`tools/build_program.py --format elf` and the application packager accept static ELF files up to 1 MiB. The runtime uses `program_image.rs` to distinguish flat images (64 KiB cap) from ELF, then performs the same strict ELF header/segment/entry/overlap and writable-executable rejection before allocating private image backing. A 1 MiB file allowance does not expand the 1 MiB virtual image region, heap cap or user stack. ELF executable content may use the .BIN suffix for compatibility; detection uses the file header.

Kernel-only FAT image read/save paths accept up to 1 MiB without expanding userspace whole-file calls. Full bounded chain validation precedes copying; the installer stages app files and writes them using copy-on-write image replacement. The bounded 1 MiB loader staging allocation is released before entry. Desktop executable reads yield on the serialized disk worker. Files recursive copy/move and ordinary whole-file APIs still cap each file at 64 KiB. Installation supports the larger app files explicitly.

`programs/bigdemo/main.rs` contains a real 128 KiB file-backed asset, initialized globals, zeroed BSS and a private 96 KiB heap. Its default isolated profile needs no filesystem or shell privileges. `run bigdemo check` verifies data and exits; `run bigdemo fault` deliberately writes its read-only asset to verify a user-mode page fault and cleanup. Two GUI instances validate independent writable state while paging through the asset.


## Yielding desktop launch lifecycle

Apps-menu launches, launch requests from Terminal/app call 71 and automatically created Terminal wrappers now read their executable on the same serialized worker as app filesystem calls. Physical-console launch and initial desktop bootstrap remain synchronous. The worker owns its private path/staging state and yields only after completed device operations; no user-image pointer, live scheduler Job borrow or unprotected PTY reference is retained across a loader yield.

Requests snapshot volume generation, working directory, arguments and profile. Before publication the scheduler rechecks cancellation, source volume, parent session/process and free job/root slots. A new process/PID/address space is published only after complete image validation. A restricted profile cannot gain authority through queued loading. A closed parent or Ctrl+Alt+Esc cancellation drops staging and any completed unpublished program; cancelled reads use a distinct error so they do not detach a healthy filesystem. Device calls already in progress must finish before cancellation can be observed.

Worker selection alternates pending loads and app syscalls when both have work. Existing bounded Apps/command queues report overload instead of growing unboundedly. The status line reports logical blocks, not a percentage or bytes. Final bounded memory copy/page-table creation still happens synchronously, as do catalog refreshes and shell filesystem commands when no worker operation is active. `tools/async_load_qemu.py` throttles real QEMU disk reads to verify live input/drawing, cancellation, closed parents, stale-volume refusal, malformed images and frame reclamation.


## PCM audio API

`sounds::play(Event)` uses syscall 100 (`ebx=0`, `ecx=event`, `edx=0`) for best-effort, nonblocking system voice playback. Event IDs 0..8 are Startup, Shutdown, Error, Warning, Info, Complete, Connect, Disconnect, Empty. `sounds::notification()` reuses Info. Successful requests return 0 even when muted, debounced, missing hardware or busy; no request is queued. Trusted and Media profiles can trigger sounds; ReadOnly/Isolated cannot. `sounds::volume()` uses `(1,0,0)` and `sounds::set_volume(percent)` uses `(2,0..100,0)`; global volume controls require Trusted access. Invalid operations, event IDs, volume or reserved arguments return -22. Zero volume mutes system sounds independently of application PCM volume.

`audio::device_rate()` (96, all args zero) reports 48000 when the AC97 output device is ready, zero when absent. `audio::open()` (97, all args zero) acquires one exclusive stream and returns a positive handle. Only processes with AUDIO may open, submit or control PCM; `--media` / Player's default profile grants FILE_READ|AUDIO and denies mutation/launch/shell/shared-state services. Read-only and isolated profiles may query device availability only.

`audio::write(handle, stereo_i16_slice)` (98: EBX handle, ECX sample pointer, EDX stereo frame count) submits 1..2048 frames of 48 kHz stereo signed little-endian i16. Input is copied from validated readable process memory into a kernel-owned buffer before publishing the DMA descriptor. Returns frames accepted or -11 when the bounded queue is full. It does not block or retain a user pointer. Retry after yielding/sleeping, while still servicing GUI events. Zero-length, oversized or malformed sample counts return -22.

`audio::queued(handle)` (99: handle,1,0) returns outstanding frames at descriptor granularity. `audio::volume(handle,percent)` (99: handle,2,0..100) adjusts the codec's output attenuation. `audio::close(handle)` (99: handle,0,0) stops and releases playback; reopening creates a fresh non-reused handle. All controls are owner-checked. Errors include -9 invalid/foreign handle, -13 denied capability, -14 invalid sample range, -16 busy device, -19 missing device, -22 invalid arguments and -5 hardware/ring error. The kernel stops a stream on owner exit/fault/forced termination and before shutdown/reboot. An application cannot DMA to arbitrary addresses or control another owner's audio by guessing its handle.

The first driver exposes output only: no capture, mixing, multiple simultaneous PCM streams, per-app resampling or device selection. Its resident 65-page descriptor/sample allocation is system memory, retained while the driver exists and excluded from private-process KiB. Queue service is bounded polling from the timer and audio calls, with no PCI IRQ requirement. App sample format is fixed; Player preloads WAVs up to 512 KiB into its own heap and converts supported PCM formats in userspace. `src/wav.rs` supplies a bounded RIFF parser shared by Player and host tests.

`directory::app_action(path, operation, volume)` uses trusted-only syscall 102: operation 0 installs an app folder, operation 1 removes an installed `/APPS/ID` folder. It uses a volume-checked 220-byte Transfer request with an empty destination. UI callers should ask for confirmation before calling; running apps and busy directories cannot be removed.

`drag_drop::{register,begin,take,finish}` wraps syscall 103. Source registration and beginning a drag require Trusted access; receive-only operations are described below. Window events `DRAG_OVER`, `DROP` and `DRAG_END` carry client coordinates and delivery tokens. Only a physically captured registered source may begin a drag; only its chosen recipient may consume it. FileClipboard is the payload layout, but normal clipboard contents are independent.

`drag_drop::register_open()` (operation 4) registers a whole-client file-open target. `drag_drop::open_file(event)` consumes/finishes a DROP and returns one regular file path, length and volume. These receive-only calls are available to ReadOnly/Media; starting file drags remains trusted-only.
