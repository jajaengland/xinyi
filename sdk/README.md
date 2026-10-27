# Application SDK

Applications are separate `/APPS/*.BIN` files. They run as protected 32-bit ring-3 programs with a 64 KiB read-only image and a 16 KiB writable stack. There is no `std`, allocator, writable image data/BSS or in-kernel ELF loader.

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

The builder uses offline Cargo and extracts a flat image linked at `0x40000000` from the compiled ELF. `sdk/user.ld` rejects writable globals/BSS. Add an app to `programs/apps.txt` for release packaging, or put the resulting `.BIN` on a FAT16 application volume. `run NAME` searches the current directory and `/APPS`; Apps lists regular `.BIN` files dynamically.

Physical-console `run` is synchronous. Desktop launches are scheduled; text apps get a Terminal automatically and graphical apps own separate windows. `spawn`/`kill` manage legacy diagnostic records, not a general desktop process API.

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

Descriptors 1 and 2 both use the text console. Each write accepts at most 1024 bytes wholly within the initialized image or user stack. The kernel validates the active address space and copies the complete buffer before output. A supported descriptor with zero length succeeds without reading the pointer. Errors are `-9` (descriptor), `-14` (buffer), `-22` (length), and `-1` (unavailable/unknown call). SDK `write` returns the signed result; `print` ignores it.

The `count` example prints numbers using a stack buffer. The `check_io` and `pid` examples exercise buffer validation and process IDs. `tools/programs_qemu.py` tests the hello, buffer-validation and process-ID examples on disposable images.

## Text input and drawing

`read_key(true)` waits; `read_key(false)` polls. The low 16 bits contain ASCII or a navigation code; bits 16..19 hold Shift/Ctrl/Alt/Caps. Enter is 10, Backspace 8, Escape 27; navigation keys are `0x100..0x109`, F1..F10 `0x120..0x129`. Use `xinyi::key` constants. Releases update decoder state without returning key events.

The text screen is 80×25 cells. Console call 53 uses EBX operation: 0 clear (ECX colour), 1 move (ECX `x | y<<16`), 2 set colour, 3 query dimensions, 4 query cursor, 5 show/hide cursor. Dimensions/cursor return `x | y<<16`. Colours use VGA attributes; positioned drawing validates the entire row span and does not scroll.

Call 54 takes EBX `x | y<<8 | colour<<16`, ECX text pointer, EDX length. Call 55 takes EBX `x | y<<8`, ECX cell pointer, EDX count; each cell is a character byte and attribute byte. Control bytes draw as spaces. Calls return byte/cell count or -22 for invalid bounds/colour and -14 for an invalid buffer.

`ticks()` wraps at 32 bits and runs at 100 Hz; compare with `wrapping_sub`. `sleep_ms` accepts 0..60000 ms, rounds nonzero waits to ticks plus an alignment tick and deschedules scheduled jobs. Foreground waits halt with interrupts enabled. `beep` accepts 37..20000 Hz for at most 1000 ms; zero frequency/duration stops sound. It uses the PC speaker, not a native audio driver.

## File editor and filesystem calls

The release includes `/APPS/EDIT.BIN`, launched with `edit [FILE]` or `run edit [FILE]`. The earlier `programs/terminal` example remains a screen-control exercise and is not shipped as the editor. The editor uses the current 1024-byte ASCII text limit, preserves its document on read/save errors, and prompts for unsaved changes and Save As overwrites. Save is to the mounted disk; no silent RAM fallback occurs.

Raw read call 57 takes EBX path pointer, ECX path byte length (1..105) and EDX output pointer to a writable 1024-byte user buffer. Raw save call 58 takes EBX path pointer, ECX `path_length | (data_length << 16)` and EDX data pointer; data length is 0..1024. Both return byte counts or negative errors: -2 not found, -13 read-only, -19 no mounted drive, -22 invalid name/length, -27 oversized file, -28 full disk, -14 invalid pointer, -5 I/O/metadata error. Reads copy only actual file bytes. Saving a nonexistent filename creates it; a failed new-file write can leave an empty directory entry, while replacement uses the existing copy/flush ordering. No power-loss journal is provided.

Call 59 takes EBX output pointer and ECX capacity. It copies the current raw argument bytes without a NUL terminator and returns the count, or -22 for insufficient capacity/-14 for an invalid writable range. Only the user stack is writable in the current image format. A zero-byte copy ignores the pointer. Arguments are at most 128 bytes; filenames still follow ASCII 8.3 path rules.

## Windows and scheduling

Calls 60..64 provide `window::open`, `event`, `draw`, `present` and `close`. The default client is 640x400. `window::open_sized(title,width,height)` uses call 65, packing width/height into EDX (low/high 16 bits). Sizes must be 160..640 by 80..400 with width divisible by 8 and height by 16. Draw coordinates are checked against that client size. Titles are 1..48 printable ASCII bytes; handles belong to the current PID. See `programs/counter/main.rs` for a small example and `programs/notepad/main.rs` for editing.

The `multitask` diagnostic starts two Counter instances with private contexts, mappings and surfaces. In this group `sleep_ms` deschedules the job, `window::event(...,true)` waits for its own queued input, and `yield_now()` (call 66, zero arguments) voluntarily yields. PIT interrupts preempt user code even if it makes no calls. Kernel calls run to completion. Console, file and speaker calls return -16 in this initial group; the foreground launcher retains those services. The scheduler saves x87/MMX state; SSE/AVX and writable image globals remain unavailable. The `desktop` command now uses this scheduler for up to four filesystem-launched graphical apps, allowing calls 57/58 with their existing pointer checks. Kernel calls run to completion, so filesystem operations are serialized. A windowless desktop app requesting console/key/speaker services is attached to an automatically opened Terminal session; the intercepted INT is retried with the existing process state. Other apps continue running. Console/speaker services remain busy for apps with an active graphical window. Writable image globals remain future work. Terminal sessions now run text children concurrently; call 71 queues launches.

## Clipboard and terminal services

| Call | SDK | Contract |
| --- | --- | --- |
| 67 | `clipboard_set(bytes)` | EBX pointer, ECX length 0..1024, EDX zero; accepts printable ASCII, tab, CR and LF. Zero clears. |
| 68 | `clipboard_get(output)` | EBX writable stack pointer, ECX capacity 0..1024, EDX zero. Zero capacity queries length; insufficient capacity returns -28 with no partial copy. |
| 69 | Retired | Returns -38; Terminal uses the shared shell through PTY calls. |
| 70 | `working_directory(output)` | EBX writable 105-byte stack buffer, ECX capacity >=105, EDX zero; returns path byte count. |
| 71 | `launch(line)` | EBX printable-ASCII `APP [ARGS]` pointer, ECX length <=128, EDX zero; queues a desktop launch and yields. |

Clipboard writes are validated/copied before replacing the shared value. Errors include -14 invalid range and -22 invalid length/content/reserved argument. The clipboard survives program close, with no on-disk persistence. PTY output uses bounded screen/history storage without a per-command output cap. Interactive text children use console calls 4/50/53..56 inside their terminal window.

Desktop jobs save/restore a checked canonical working-directory snapshot around each quantum. A terminal's `cd` changes only its own snapshot; a queued launch copies that directory and arguments into kernel-owned storage before yielding. File calls and terminal commands complete serially in kernel context. Launch succeeds as a request (0); subsequent loading errors appear in desktop status. No free slot returns -24, unavailable/busy desktop returns -16, and invalid arguments return -22. A child starts with its parent's directory; Apps launches use the physical shell's starting directory. The physical shell’s `terminal` command opens the scheduled desktop and starts Terminal.


## PTY and directory calls

PTY handles belong to their creator. All calls require an active user image and matching CR3. Calls 72..77 are exposed as `xinyi::pty`; invalid handles return -9, unavailable/busy sessions -16, capacity failure -28 and invalid arguments -22.

| Call | Arguments |
| --- | --- |
| 72 | EBX operation, ECX handle, EDX 0. Operations: 0 open (handle 0), 1 close if idle, 2 force close, 3 foreground PID, 4 closed flag, 5 invalidate display cache. |
| 73 | EBX session, ECX owned 640×400 window, EDX scrollback rows 0..64. |
| 74 | EBX session, ECX key/modifier event, EDX 0. |
| 75 | EBX session, ECX readable paste pointer, EDX byte length <=1024. Rejects oversized paste atomically. |
| 76 | EBX session, ECX anchor, EDX endpoint, both 0..2000 cell offsets. Freezes the selected view. |
| 77 | EBX session, ECX 0 copy selection / 1 clear selection, EDX 0. Copy uses the shared clipboard. |
| 78 | EBX readable path, ECX path length low16 / entry offset high16, EDX writable 776-byte `directory::Listing`. |
| 79 | EBX readable path, ECX path length, EDX 0; changes only the caller’s directory. |
| 80 | EBX readable path, ECX path length, EDX 0 create folder / 1 delete file / 2 remove empty folder. |

Directory paths are 1..105 printable ASCII bytes and retain FAT 8.3/depth rules. Listings contain up to 32 initialized entries plus returned count and total, excluding `.` and `..`. Each entry is 24 bytes: `[u8;12]` name, u32 name length, u32 file size, u32 directory flag (bit0). The Files program pages 18 visible rows. Errors include -14 invalid buffer, -17 exists, -20 not directory, -21 not file and -39 directory not empty. `directory_check` exercises paging, invalid ranges, create/delete and failure preservation.


## Desktop lifecycle and current limits

The desktop supports four root application windows and up to four attached text children in eight internal job slots. A text app launched from Apps receives a Terminal at its first console request; its existing process is attached rather than reloaded. A missing Terminal or failed allocation/open stops the waiting child cleanly. Applications need no executable-name registry or GUI/text classification.

Desktop title bars provide minimize and close. Minimize keeps the process, surface, pending document and scheduler state alive; another visible window takes input focus. Hidden apps continue executing and drawing into their retained surfaces. Clicking the taskbar restores and brings the window fully into view. Moving partly beyond screen edges does not change client coordinates or buffer sizes: the kernel clips the final surface copy. The top taskbar remains protected. The synchronous foreground API has no taskbar minimize control.

Application images remain flat code/read-only data with a 16 KiB writable user stack and guard page. There is no application heap, writable image data/BSS or in-kernel ELF loader yet. Window dimensions are fixed at creation, text files/clipboard are capped at 1024 ASCII bytes, and kernel file calls are serialized. Shared GUI widgets and richer file/process APIs are planned additions rather than current ABI guarantees.
