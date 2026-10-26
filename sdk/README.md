# Writing a Xinyi program

Standard applications are separate files in FAT16 `/APPS`. `programs/apps.txt` lists their sources for release packaging; the kernel does not embed them. `apps` lists actual files and `run NAME` searches the current directory then `/APPS`. Adding, removing or replacing a `.BIN` file changes the available apps. `edit [FILE]` runs the real file editor from `/APPS/EDIT.BIN`.

Programs are freestanding 32-bit Rust binaries, compiled on the host with the nightly toolchain and Rust sources used for the kernel. They do not use `std` or an allocator. See `programs/hello/main.rs`:

```rust
#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
fn main() -> u32 {
    xinyi::print("Hello from Rust on Xinyi!\n");
    0
}
```

Adjust the module path for your source location. The SDK supplies `_start`, syscall wrappers and a panic handler that exits with code 101. `main` returns the exit code. Stack locals, immutable constants, keyboard events and text-screen control are supported; bounded file access and arguments are supported; writable globals and heap allocation remain unfinished.

From the repository root, with the kernel already built:

```powershell
python tools/build_program.py programs/hello/main.rs --output target/programs/hello.bin --force
python tools/program_disk.py target/hello-data.img --file HELLO.BIN=target/programs/hello.bin
qemu-system-i386 -kernel target/i686-unknown-none/debug/rust-os -drive "file=target/hello-data.img,format=raw,if=ide,index=0"
```

The disk builder creates a FAT16 **data image**, not a bootable OS disk, and refuses to overwrite existing images. Choose a fresh filename when rebuilding one. In Xinyi:

```text
run demo
mount
run hello.bin
```

`run` creates a task, executes it synchronously in protected ring 3, reports exit/fault and reclaims its memory. `spawn demo` creates a paused diagnostic record; `kill PID` removes one. This is not multitasking.

## Binary format

The builder compiles an ELF internally and extracts a flat read-only image linked at `0x40000000`, with entry at its first byte. The kernel loads flat images, not arbitrary ELF/Windows executable files. `sdk/user.ld` rejects writable globals/BSS; the separate user stack is writable and 16 KiB. The memory loader and FAT16 program reader support up to 64 KiB. Ordinary text-file reads/writes and RAM files still have a 1024-byte limit; FAT16 deletion also handles the larger program files. The supplied hello program is 104 bytes with the current compiler.

The builder uses offline Cargo and caches libraries in `target/user-build`. Existing binary outputs require `--force`. Incompatible raw code generally faults and returns to the shell. ELF loading, a richer memory layout and filesystem syscalls remain future work.

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

## Interactive text programs

`programs/terminal/main.rs` is a 559-byte typing demo: arrows move, Home/End select row edges, typing overwrites, Backspace/Delete erase, Enter moves down and Escape quits. Its contents exist only on the screen; it is not a file editor. Build it with `python tools/build_program.py programs/terminal/main.rs --output target/programs/terminal.bin --force`, then put it on a data image with `tools/program_disk.py` as above.

`read_key(true)` waits with the CPU halted until a key press; `read_key(false)` polls. The positive return value has a key code in bits 0..15 and Shift/Ctrl/Alt/Caps flags in bits 16..19 (constants in `xinyi::key`). Characters use ASCII; Enter is 10, Backspace 8, Escape 27. Navigation keys are `0x100..0x109` and F1..F10 are `0x120..0x129`. Releases and modifier changes update decoder state without returning events. The keyboard currently uses a US-style scancode map; numpad/Unicode input is not implemented. The foreground app consumes the keyboard while the shell waits for it to finish.

The screen is 80 by 25 cells. `clear_screen(colour)`, `set_color(colour)` and `move_cursor(x,y)` return 0 or `-22` for invalid arguments. Colours are VGA attributes `0x00..0x7f` (foreground low nibble, background bits 4..6). `screen_size()` and `cursor_position()` return `x | (y << 16)`. `draw_text` validates the whole single-row span and user buffer before writing; it returns the byte count, `-22` for invalid bounds/colour or `-14` for an invalid buffer. Control bytes draw as spaces. Even the bottom-right cell can be replaced without scrolling. Zero-length drawing ignores the pointer, but still validates coordinates and colour.

Raw console call 53 uses EBX operation: 0=clear with ECX colour, 1=move to ECX packed `x | (y << 16)`, 2=set ECX colour, 3=query dimensions, 4=query cursor. Raw drawing call 54 takes EBX `x | (y << 8) | (colour << 16)`, ECX user pointer and EDX length. Blocking calls return with the user's original interrupt/direction flags intact. `sleep_ms` rounds up to 10 ms ticks plus one alignment tick; zero returns immediately and requests over 60000 return `-22`. This is foreground waiting, not a scheduler. Timing comparisons use `ticks().wrapping_sub(start)`.

After a program uses console control or drawing, exit/fault recovery restores the shell colour and places diagnostics at the bottom of the screen. Screen contents are retained, not restored. `tools/terminal_qemu.py` checks typing, key modifiers, CPU halt during input waits, timing, screen boundaries, pointer refusal and recovery in both kernel profiles.

`show_cursor(false)` hides the hardware cursor, and `show_cursor(true)` shows it. Raw call 53 operation 5 takes ECX 0 or 1. The previous visibility is restored after exit or fault. `draw_cells` takes `#[repr(C)] Cell { character: u8, color: u8 }` values; raw call 55 uses EBX `x | (y << 8)`, ECX pointer and EDX cell count. At most the remaining cells in one row are accepted, and the entire buffer and all colours are checked before drawing. It preserves cursor position and returns a cell count or the same errors as `draw_text`.

## Tetris demo

`programs/tetris/main.rs` uses stack-local game state, keyboard polling, the tick counter, sleep, coloured row drawing and cursor hiding. It has a 10x20 board, seven-piece bags, rotation with simple wall/floor kicks, a landing guide, next-piece preview, score/lines/level, pause, restart and game over. It does not implement the official SRS rotation system or lock delay. The shared game logic in `game.rs` is host-tested; `tools/tetris_qemu.py` exercises the actual protected game in both kernel profiles, including the 64 KiB loading boundary and frame reclamation.

```powershell
python tools/build_program.py programs/tetris/main.rs --output target/programs/tetris.bin --force
python tools/program_disk.py target/tetris-data.img --file TETRIS.BIN=target/programs/tetris.bin
qemu-system-i386 -kernel target/i686-unknown-none/debug/rust-os -drive "file=target/tetris-data.img,format=raw,if=ide,index=0"
```

In Xinyi, use `mount` then `run tetris.bin`. Left/Right move, Up/X rotates clockwise, Z rotates back, Down soft-drops, Space hard-drops, P pauses, R restarts and Escape quits. General writable globals and heap allocation remain future work; this game fits in the existing protected 16 KiB user stack.

## Snake and PC-speaker sound

`programs/snake/main.rs` is a 24x18 Snake game: arrows or WASD steer, P pauses, R restarts and Escape quits. Eat the red dots to grow and increase speed; hitting a wall or the body ends the game. Filling the board wins. The first dot is directly ahead of the starting snake. Each eaten dot plays an 880 Hz, 70 ms beep, including the final winning dot.

`beep(frequency, milliseconds)` accepts 37..20000 Hz and 0..1000 ms, returning 0 or `-22` for invalid arguments. Zero frequency or duration silences the speaker. Raw syscall 56 uses EBX frequency and ECX duration. It programs PIT channel 2 and the speaker gate at port `0x61`, leaving the channel-0 system timer unchanged. The foreground call waits with the CPU halted and IRQs enabled around each wait, then disables the speaker before returning. Exit/fault cleanup also silences it. Timing uses 10 ms ticks plus an alignment tick.

Build Snake with `python tools/build_program.py programs/snake/main.rs --output target/programs/snake.bin --force` and package it as `SNAKE.BIN` using `tools/program_disk.py`. On Windows, enable QEMU PC-speaker playback by adding `-audiodev dsound,id=speaker -machine pcspk-audiodev=speaker` to the launch command. Then use `mount` and `run snake.bin`. The included manual Snake launcher has these audio settings.

`rustc --edition=2021 --test programs/snake/game.rs -o target/snake-tests.exe` builds host game tests. `tools/snake_qemu.py` checks debug/release gameplay, growth, one beep per eaten dot, pause, game over, restart, frame cleanup and unchanged disk data. It uses QEMU's WAV backend and verifies actual oscillating audio samples; captures are saved in `target/smoke/snake-debug.wav` and `snake-release.wav`.

To add demos to an existing manual QEMU test session, close its QEMU window and run `python tools/add_qemu_demos.py target/manual-qemu/SESSION/config.json`, substituting its session folder. The helper preserves existing files, tests a copy through the installed BIOS loader, and backs up the original disk before publishing the additions. The programs appear under `/APPS`; `cat /apps/demos.txt` describes them. `FAULT.BIN` deliberately triggers an invalid-instruction exception to test returning to the shell.

## File editor and filesystem calls

The release includes `/APPS/EDIT.BIN`, launched with `edit [FILE]` or `run edit [FILE]`. The earlier `programs/terminal` example remains a screen-control exercise and is not shipped as the editor. The editor uses the current 1024-byte ASCII text limit, preserves its document on read/save errors, and prompts for unsaved changes and Save As overwrites. Save is to the mounted disk; no silent RAM fallback occurs.

Raw read call 57 takes EBX path pointer, ECX path byte length (1..105) and EDX output pointer to a writable 1024-byte user buffer. Raw save call 58 takes EBX path pointer, ECX `path_length | (data_length << 16)` and EDX data pointer; data length is 0..1024. Both return byte counts or negative errors: -2 not found, -13 read-only, -19 no mounted drive, -22 invalid name/length, -27 oversized file, -28 full disk, -14 invalid pointer, -5 I/O/metadata error. Reads copy only actual file bytes. Saving a nonexistent filename creates it; a failed new-file write can leave an empty directory entry, while replacement uses the existing copy/flush ordering. No power-loss journal is provided.

Call 59 takes EBX output pointer and ECX capacity. It copies the current raw argument bytes without a NUL terminator and returns the count, or -22 for insufficient capacity/-14 for an invalid writable range. Only the user stack is writable in the current image format. A zero-byte copy ignores the pointer. Arguments are at most 128 bytes; filenames still follow ASCII 8.3 path rules.
