"""Exercise real ring-3 page protections in private QEMU guests."""
import argparse
from pathlib import Path
import re
import shutil
import tempfile

from smoke import Guest, ROOT


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
    cases = [("kernel-read", 5, 0x100000), ("kernel-write", 7, 0x100000),
             ("kernel-stack", 5, None), ("page-table", 5, None),
             ("code-write", 7, 0x40000000), ("stack-guard", 6, 0x403fb000),
             ("vga", 5, 0xb8000), ("kernel-exec", 5, 0x100000), ("image-alias", 5, None)]
    try:
        with tempfile.TemporaryDirectory(prefix="isolation-", dir=output) as temporary:
            directory = Path(temporary)
            for kernel in kernels:
                for mode, error, address in cases:
                    guest = Guest(args.qemu, kernel.resolve(), directory, memory_mib=8)
                    try:
                        snapshot = directory / "kernel-before.bin"
                        guest.call("human-monitor-command", {"command-line": f'pmemsave 0x100000 4096 "{snapshot.as_posix()}"'})
                        before = snapshot.read_bytes()
                        screen = guest.command(f"usermode {mode}", "User program faulted:")
                        debug = (directory / "debug.log").read_text()
                        transcript.append(f"[{kernel.parent.name}: {mode}]\n{screen}\n{debug}")
                        assert "Syscall frame: nr=4 args=00000001/00000000/00000000 cs=001b" in debug, debug
                        fault = re.search(r"vector=(\d+) error=([0-9a-f]+) eip=([0-9a-f]+) cs=([0-9a-f]+)", debug)
                        assert fault and int(fault[1]) == 14 and int(fault[2], 16) == error and int(fault[4], 16) == 0x1b, debug
                        eip = int(fault[3], 16)
                        assert eip == 0x100000 if mode == "kernel-exec" else 0x40000000 <= eip < 0x40010000
                        space = re.search(r"User space: cr3=([0-9a-f]+) image=([0-9a-f]+)", debug)
                        assert space, debug
                        if mode == "kernel-stack":
                            address = int(re.search(r"kernel ([0-9a-f]+)/", debug)[1], 16)
                        elif mode == "page-table":
                            address = int(space[1], 16)
                        elif mode == "image-alias":
                            address = int(space[2], 16)
                        assert f"Fault address: {address:08x}" in debug, debug
                        registers = guest.call("human-monitor-command", {"command-line": "info registers"})
                        assert int(re.search(r"CR0=([0-9a-fA-F]+)", registers)[1], 16) & 0x10000
                        recovered = re.search(r"User recovery: cr3=([0-9a-f]+)", debug)
                        assert recovered and int(re.search(r"CR3=([0-9a-fA-F]+)", registers)[1], 16) == int(recovered[1], 16)
                        assert int(recovered[1], 16) != int(space[1], 16)
                        guest.call("human-monitor-command", {"command-line": f'pmemsave 0x100000 4096 "{snapshot.as_posix()}"'})
                        assert snapshot.read_bytes() == before, "User probe modified kernel bytes"
                        guest.command("echo recovered", "\nrecovered\n")
                        print(f"PASS {kernel.parent.name}: {mode} -> user page fault {error}, correct CR2, kernel intact", flush=True)
                    finally:
                        guest.close()
                guest = Guest(args.qemu, kernel.resolve(), directory, memory_mib=8,
                              extra_args=["-cpu", "qemu32,pse=off"])
                try:
                    screen = guest.command("paging", "Paging: disabled")
                    transcript.append(screen)
                    before = guest.command("frames", "Free now:")
                    baseline = int(re.search(r"Free now: +(\d+)", before)[1])
                    transcript.append(guest.command("usermode", "User isolation unavailable"))
                    transcript.append(guest.command("run", "Failed to create task record"))
                    after = guest.command("frames", "Free now:")
                    assert int(re.search(r"Free now: +(\d+)", after)[1]) == baseline
                    transcript.append(guest.command("echo usable", "\nusable\n"))
                    print(f"PASS {kernel.parent.name}: no PSE -> user setup refused, frames balanced, shell usable", flush=True)
                finally:
                    guest.close()
    finally:
        (output / "isolation-transcript.txt").write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {output / 'isolation-transcript.txt'}")


if __name__ == "__main__":
    main()
