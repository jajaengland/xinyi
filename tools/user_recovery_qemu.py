"""Check repeated user exits/faults, kernel restoration and frame reclamation."""
import argparse
from pathlib import Path
import re
import shutil
import struct
import tempfile
import time

from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel", type=Path, action="append")
    args = parser.parse_args()
    kernels = args.kernel or [ROOT / f"target/i686-unknown-none/{profile}/rust-os" for profile in ("debug", "release")]
    if not args.qemu or not all(kernel.is_file() for kernel in kernels):
        parser.error("Build both profiles and put QEMU on PATH")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="user-recovery-", dir=output) as temporary:
            directory = Path(temporary)
            for kernel in kernels:
                # Deterministic virtual time ensures a PIT interrupt occurs
                # during the null-segment loop, regardless of host CPU speed.
                guest = Guest(args.qemu, kernel.resolve(), directory, memory_mib=8,
                              extra_args=["-icount", "shift=3,align=off,sleep=off"])
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

                    def registers():
                        info = guest.call("human-monitor-command", {"command-line": "info registers"})
                        return int(re.search(r"CR3=([0-9a-fA-F]+)", info)[1], 16)

                    def tss_stack():
                        info = guest.call("human-monitor-command", {"command-line": "info registers"})
                        base = int(re.search(r"TR\s*=\w+\s+([0-9a-fA-F]+)", info)[1], 16)
                        path = directory / "tss.bin"
                        guest.call("human-monitor-command", {"command-line": f'pmemsave {base + 4} 4 "{path.as_posix()}"'})
                        return struct.unpack("<I", path.read_bytes())[0]

                    baseline, original_cr3 = free_frames(), registers()
                    original_tss = tss_stack()
                    # Each run uses one image page, four user and four syscall
                    # stack pages, and two page-table pages: eleven frames.
                    modes = [("exit", None), ("", 13), ("kernel-write", 14),
                             ("stack-guard", 14), ("invalid-op", 6),
                             ("null-segments", None), ("exit-df", None)]
                    previous_log = ""
                    for cycle in range(2):
                        for mode, vector in modes:
                            text = "usermode" + (" " + mode if mode else "")
                            expected = "User program exited with code 42." if vector is None else f"User program faulted: vector={vector} "
                            command(text, expected)
                            assert registers() == original_cr3
                            assert tss_stack() == original_tss
                            info = guest.call("human-monitor-command", {"command-line": "info registers"})
                            assert "CPL=0" in info, info
                            assert free_frames() == baseline, "User run leaked frames"
                            debug = (directory / "debug.log").read_text()
                            recent = debug[len(previous_log):]
                            previous_log = debug
                            record = re.search(r"User recovery: cr3=([0-9a-f]+) tss=([0-9a-f]+) reclaimed=(\d+) ticks=(\d+) flags=([0-9a-f]+)", recent)
                            assert record and int(record[1], 16) == original_cr3 and int(record[3]) == 23, recent
                            assert int(record[2], 16) == original_tss
                            flags = int(record[5], 16)
                            assert flags & 0x200 and not flags & 0x7400, recent
                            if mode == "null-segments":
                                assert int(record[4]) > 0, "No timer IRQ during null-segment user loop"
                            assert "Syscall frame: nr=4 args=00000001/00000000/00000000 cs=001b" in recent, recent
                            print(f"PASS {kernel.parent.name} cycle {cycle + 1}: {mode or 'cli'} recovered; CR3/TSS and frames restored", flush=True)
                    # Verify the shell and string/file operations still work
                    # after null segments, a set user DF, and repeated faults.
                    command("touch recover.txt", "Created: recover.txt")
                    command("write recover.txt shell still works", "Wrote 17 bytes")
                    command("cat recover.txt", "\nshell still works\n")
                    command("delete recover.txt", "Deleted: recover.txt")
                    command("spawn demo", "Created task record 1")
                    command("kill 1", "Removed task record 1")
                    assert free_frames() == baseline
                    command("echo recovered", "\nrecovered\n")
                    transcript.append((directory / "debug.log").read_text())
                    # A stale user recovery context must never swallow a
                    # kernel exception after the user run has been cleaned up.
                    screen = guest.command("breakpoint", "System halted.")
                    time.sleep(0.1)
                    debug = (directory / "debug.log").read_text()
                    assert "vector=3" in debug and "cs=00000008" in debug, debug
                    transcript.append(screen)
                    print(f"PASS {kernel.parent.name}: kernel breakpoint remains fatal after user recovery", flush=True)
                finally:
                    guest.close()
    finally:
        (output / "user-recovery-transcript.txt").write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {output / 'user-recovery-transcript.txt'}")


if __name__ == "__main__":
    main()
