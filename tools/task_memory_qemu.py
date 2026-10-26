"""Verify task memory ownership, full-table failure and allocated ring-3 stacks."""
import argparse
from pathlib import Path
import re
import shutil
import struct
import tempfile
import time

from smoke import Guest, ROOT


def kernel_end(kernel):
    elf = kernel.read_bytes()
    offset = struct.unpack_from("<I", elf, 28)[0]
    size, count = struct.unpack_from("<HH", elf, 42)
    return max(address + memory for kind, _, _, address, _, memory, _, _ in
               (struct.unpack_from("<8I", elf, offset + i * size) for i in range(count)) if kind == 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel", type=Path, action="append")
    args = parser.parse_args()
    kernels = args.kernel or [ROOT / f"target/i686-unknown-none/{profile}/rust-os" for profile in ("debug", "release")]
    if not args.qemu or not all(kernel.is_file() for kernel in kernels):
        parser.error("Build both profiles first and put QEMU on PATH")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="task-memory-", dir=output) as temporary:
            directory = Path(temporary)
            for kernel in kernels:
                guest = Guest(args.qemu, kernel.resolve(), directory, memory_mib=8)
                try:
                    def command(text, expected):
                        screen = guest.command(text, expected)
                        if text == "spawn" and expected.startswith("Created task record"):
                            deadline = time.monotonic() + 5
                            while not re.search(r"Page directory: 0x[0-9a-f]{8}", screen):
                                assert time.monotonic() < deadline, screen
                                time.sleep(0.01)
                                screen = guest.screen()
                        transcript.append(f"[{kernel.parent.name}: {text}]\n{screen}")
                        return screen

                    def free_frames():
                        screen = command("frames", "Free now:")
                        return int(re.search(r"Free now: +(\d+)", screen)[1])

                    def memory(address, size):
                        path = directory / "memory.bin"
                        guest.call("human-monitor-command", {"command-line": f'pmemsave {address} {size} "{path.as_posix()}"'})
                        return path.read_bytes()

                    baseline = free_frames()
                    ranges = []
                    for pid in range(1, 17):
                        screen = command("spawn", f"Created task record {pid} ")
                        entry = int(re.search(r"Program: (0x[0-9a-f]+)", screen)[1], 16)
                        user_top = int(re.search(r"User stack backing top: (0x[0-9a-f]+)", screen)[1], 16) + 4
                        kernel_top = int(re.search(r"Kernel stack top: (0x[0-9a-f]+)", screen)[1], 16)
                        owned = [(entry, entry + 4096), (user_top - 16384, user_top), (kernel_top - 16384, kernel_top)]
                        for start, end in owned:
                            assert start % 4096 == 0 and start >= kernel_end(kernel) and end <= 8 * 1024**2
                            assert all(end <= old_start or start >= old_end for old_start, old_end in ranges), "Overlapping allocations"
                            ranges.append((start, end))
                        size = int(re.search(r"Program: 0x[0-9a-f]+ \((\d+) bytes\)", screen)[1])
                        image = memory(entry, 4096)
                        assert b"Hello from a Xinyi program!\n" in image[:size]
                        assert image[size:] == bytes(4096 - size)
                        assert memory(user_top - 16384, 16384) == bytes(16384)
                        assert memory(kernel_top - 16384, 16384) == bytes(16384)
                        directory_address = int(re.search(r"Page directory: (0x[0-9a-f]+)", screen)[1], 16)
                        entries = struct.unpack("<1024I", memory(directory_address, 4096))
                        assert entries[256] & 0x87 == 7
                        assert all(not entry & 4 for index, entry in enumerate(entries) if index != 256)
                        table_address = entries[256] & 0xfffff000
                        pages = struct.unpack("<1024I", memory(table_address, 4096))
                        assert pages[0] == entry | 5
                        assert all(page == 0 for page in pages[1:1020])
                        assert pages[1020:] == tuple((user_top - 16384 + index * 4096) | 7 for index in range(4))
                        for address in (directory_address, table_address):
                            assert address >= kernel_end(kernel) and address < 8 * 1024**2
                            assert all(address + 4096 <= old_start or address >= old_end for old_start, old_end in ranges)
                            ranges.append((address, address + 4096))
                    full = free_frames()
                    assert baseline - full == 16 * 11
                    for _ in range(3):
                        command("spawn", "Failed to create task record")
                        assert free_frames() == full, "Full-table failure leaked frames"
                    command("kill 999", "Task record not found")
                    for pid in range(1, 17):
                        command(f"kill {pid}", f"Removed task record {pid}")
                    command("ps", "Total tasks: 0")
                    assert free_frames() == baseline, "Removal leaked owned frames"
                    command("spawn demo", "Created task record 17 ")
                    command("kill 17", "Removed task record 17")
                    assert free_frames() == baseline
                    screen = command("usermode", "User program faulted:")
                    debug = (directory / "debug.log").read_text()
                    assert "Syscall frame: nr=4 args=00000001/00000000/00000000 cs=001b" in debug, debug
                    stacks = re.search(r"User stacks: ([0-9a-f]+)/([0-9a-f]+) kernel ([0-9a-f]+)/([0-9a-f]+)", debug)
                    assert stacks, debug
                    ub, ut, kb, kt = (int(value, 16) for value in stacks.groups())
                    assert ut - ub == kt - kb == 16384 and (ut <= kb or kt <= ub)
                    assert min(ub, kb) >= kernel_end(kernel) and max(ut, kt) < 8 * 1024**2
                    transcript.append(debug)
                    print(f"PASS {kernel.parent.name} / 8 MiB: owned zeroed regions, full-table rollback, removal/reuse, allocated ring-3 and syscall stacks", flush=True)
                finally:
                    guest.close()
    finally:
        (output / "task-memory-transcript.txt").write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {output / 'task-memory-transcript.txt'}")


if __name__ == "__main__":
    main()
