"""Build real Rust programs and run them from private FAT16 QEMU images."""
import argparse
import hashlib
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time

from mkfat16 import create_image
from smoke import Guest, ROOT
from storage_qemu import inspect


def seed(image, files):
    create_image(image, partitioned=True)
    start, cluster = 63, 2
    with image.open("r+b") as stream:
        for slot, (name, data) in enumerate(files.items()):
            first = cluster if data else 0
            count = (len(data) + 511) // 512
            for index in range(count):
                value = cluster + 1 if index + 1 < count else 0xffff
                for copy in range(2):
                    stream.seek((start + 1 + copy * 128) * 512 + cluster * 2)
                    stream.write(struct.pack("<H", value))
                stream.seek((start + 289 + cluster - 2) * 512)
                stream.write(data[index * 512:(index + 1) * 512])
                cluster += 1
            entry = bytearray(32)
            base, extension = name.split(".")
            entry[:11] = (base.ljust(8) + extension.ljust(3)).encode()
            entry[11] = 0x20
            struct.pack_into("<H", entry, 26, first)
            struct.pack_into("<I", entry, 28, len(data))
            stream.seek((start + 257) * 512 + slot * 32)
            stream.write(entry)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel", type=Path, action="append")
    args = parser.parse_args()
    kernels = args.kernel or [ROOT / f"target/i686-unknown-none/{profile}/rust-os" for profile in ("debug", "release")]
    if not args.qemu or not all(kernel.is_file() for kernel in kernels):
        parser.error("Build both kernels first and put QEMU on PATH")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="programs-", dir=output) as temporary:
            directory = Path(temporary)
            files = {}
            for source, name in [("hello", "HELLO.BIN"), ("check_io", "CHECK.BIN"), ("pid", "PID.BIN")]:
                binary = directory / name
                subprocess.run([sys.executable, str(ROOT / "tools/build_program.py"),
                                str(ROOT / f"programs/{source}/main.rs"), "--output", str(binary)], check=True)
                files[name] = binary.read_bytes()
                assert 0 < len(files[name]) <= 1024, (name, len(files[name]))
            files.update({"BAD.BIN": b"\x0f\x0b", "EMPTY.BIN": b"", "BIG.BIN": bytes(65537)})
            image = directory / "programs.img"
            seed(image, files)
            original = hashlib.sha256(image.read_bytes()).digest()
            assert inspect(image, 63) == files
            for kernel in kernels:
                guest = Guest(args.qemu, kernel.resolve(), directory, disk=image, memory_mib=8)
                try:
                    def command(text, expected):
                        screen = guest.command(text, expected)
                        deadline = time.monotonic() + 5
                        while not guest.prompt_ready(screen):
                            assert time.monotonic() < deadline, screen
                            time.sleep(0.01)
                            screen = guest.screen()
                        transcript.append(f"[{kernel.parent.name}: {text}]\n{screen}")
                        return screen

                    def free_frames():
                        return int(re.search(r"Free now: +(\d+)", command("frames", "Free now:"))[1])

                    baseline = free_frames()
                    screen = command("run demo", "User program exited with code 0.")
                    assert "Hello from a Xinyi program!" in screen and "Running process 1" in screen
                    command("ps", "Total tasks: 0")
                    assert free_frames() == baseline
                    command("mount", "Mounted FAT16 at sector 63")
                    command("spawn", "Created task record 2")
                    held = free_frames()
                    assert baseline - held == 11
                    command("run pid.bin", "User program exited with code 3.")
                    command("ps", "Total tasks: 1")
                    screen = command("run hello.bin", "User program exited with code 0.")
                    assert "Hello from Rust on Xinyi!" in screen and "Running process 4" in screen
                    debug_before = (directory / "debug.log").read_text()
                    screen = command("run check.bin", "User program exited with code 0.")
                    for message in ("Stack output works!", "Stderr output works!", "Console checks passed!"):
                        assert message in screen, screen
                    new_log = (directory / "debug.log").read_text()[len(debug_before):]
                    assert "Xinyi exception" not in new_log, new_log
                    assert free_frames() == held
                    command("run bad.bin", "User program faulted: vector=6 ")
                    assert free_frames() == held
                    command("run empty.bin", "Program is empty.")
                    command("run missing.bin", "Program: file not found")
                    command("run big.bin", "Program: program exceeds 64 KiB")
                    screen = command("run hello.bin", "User program exited with code 0.")
                    assert "Running process 7" in screen and "Hello from Rust on Xinyi!" in screen
                    command("kill 2", "Removed task record 2")
                    command("ps", "Total tasks: 0")
                    assert free_frames() == baseline
                    command("echo usable", "\nusable\n")
                    transcript.append((directory / "debug.log").read_text())
                finally:
                    guest.close()
                assert hashlib.sha256(image.read_bytes()).digest() == original, "Execution modified disk files"
                print(f"PASS {kernel.parent.name}: built-in and Rust file programs, real PID/stdout/stderr, bad buffers, fault cleanup, frame balance and unchanged FAT16", flush=True)
    finally:
        (output / "programs-transcript.txt").write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {output / 'programs-transcript.txt'}")


if __name__ == "__main__":
    main()
