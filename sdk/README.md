# Writing a Xinyi program

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

Adjust the module path for your source location. The SDK supplies `_start`, syscall wrappers and a panic handler that exits with code 101. `main` returns the exit code. Stack locals and immutable constants are supported; writable globals, heap allocation, input and command-line arguments are not yet supported.

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

The builder compiles an ELF internally and extracts a flat read-only image linked at `0x40000000`, with entry at its first byte. The kernel loads flat images, not arbitrary ELF/Windows executable files. `sdk/user.ld` rejects writable globals/BSS; the separate user stack is writable and 16 KiB. The memory loader supports up to 64 KiB, while FAT16 currently restricts disk-loaded programs to 1024 bytes. The supplied hello program is 104 bytes with the current compiler.

The builder uses offline Cargo and caches libraries in `target/user-build`. Existing binary outputs require `--force`. Incompatible raw code generally faults and returns to the shell. ELF loading, a richer memory layout and filesystem/input syscalls remain future work.

## Syscalls

The experimental ABI uses `int 0x80`: EAX = number, EBX/ECX/EDX = arguments, EAX = result; other general registers and flags are preserved on returning calls.

| Number | Call | Result |
| --- | --- | --- |
| 1 | `exit(code)` | Terminates the current process |
| 4 | `write(fd, pointer, length)` | Byte count or a negative error |
| 20 | `getpid()` | Current process ID |

Descriptors 1 and 2 both use the text console. Each write accepts at most 1024 bytes wholly within the initialized image or user stack. The kernel validates the active address space and copies the complete buffer before output. A supported descriptor with zero length succeeds without reading the pointer. Errors are `-9` (descriptor), `-14` (buffer), `-22` (length), and `-1` (unavailable/unknown call). SDK `write` returns the signed result; `print` ignores it.

The `count` example prints numbers using a stack buffer. The `check_io` and `pid` examples exercise buffer validation and process IDs. `tools/programs_qemu.py` tests the hello, buffer-validation and process-ID examples on disposable images.

To add demos to an existing manual QEMU test session, close its QEMU window and run `python tools/add_qemu_demos.py target/manual-qemu/SESSION/config.json`, substituting its session folder. The helper preserves existing files, tests a copy through the installed BIOS loader, and backs up the original disk before publishing the additions. The programs appear under `/APPS`; `cat /apps/demos.txt` describes them. `FAULT.BIN` deliberately triggers an invalid-instruction exception to test returning to the shell.
