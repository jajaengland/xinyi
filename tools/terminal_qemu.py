"""Verify foreground keyboard, timing and screen services on private disks."""
import argparse
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

from program_disk import create_program_disk
from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel", type=Path, action="append")
    args = parser.parse_args()
    kernels = args.kernel or [ROOT / f"target/i686-unknown-none/{profile}/rust-os"
                              for profile in ("debug", "release")]
    qemu = args.qemu or r"C:\Program Files\qemu\qemu-system-i386.exe"
    nasm = shutil.which("nasm") or r"C:\msys64\usr\bin\nasm.exe"
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="terminal-", dir=output) as temporary:
            directory = Path(temporary)
            demo, probe = directory / "TERM.BIN", directory / "CHECK.BIN"
            subprocess.run([sys.executable, str(ROOT / "tools/build_program.py"),
                            str(ROOT / "programs/terminal/main.rs"), "--output", str(demo)], check=True)
            subprocess.run([nasm, "-f", "bin", str(ROOT / "programs/terminal_check.asm"),
                            "-o", str(probe)], check=True)
            # Change terminal colour, then fault. Recovery must restore shell colour.
            fault = bytes.fromhex("b83500000031dbb94f000000cd800f0b")
            image = directory / "data.img"
            create_program_disk(image, {"TERM.BIN": demo.read_bytes(), "CHECK.BIN": probe.read_bytes(), "FAULT.BIN": fault})
            original = image.read_bytes()
            for kernel in kernels:
                guest = Guest(qemu, kernel, directory, disk=image)
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
                    def frames():
                        return int(re.search(r"Free now: +(\d+)", command("frames", "Free now:"))[1])
                    baseline = frames()
                    command("mount", "Mounted FAT16")
                    guest.command("run check.bin", "Terminal checks ready")
                    screen = guest.screen()
                    assert screen.splitlines()[24][-1:] == "X", screen
                    raw = (directory / "vga.bin").read_bytes()
                    assert raw[160:164] == b'C\x1eD\x2f'
                    halted = sum("HLT=1" in guest.call("human-monitor-command", {"command-line": "info registers"})
                                 for _ in range(10))
                    assert halted >= 5, f"Key wait did not halt: {halted}/10"
                    for key in ("ctrl-s", "shift-a", "alt-x", "home", "delete", "pgup", "pgdn", "insert", "end", "f1"):
                        guest.key(key)
                    transcript.append(guest.wait_for("User program exited with code 0."))
                    assert frames() == baseline
                    guest.command("run term.bin", "Xinyi typing demo")
                    guest.type("abc")
                    guest.key("left")
                    guest.type("z")
                    screen = guest.screen()
                    deadline = time.monotonic() + 5
                    while not screen.splitlines()[2].startswith("abz"):
                        assert time.monotonic() < deadline, screen
                        time.sleep(0.02)
                        screen = guest.screen()
                    assert screen.splitlines()[2].startswith("abz"), screen
                    guest.key("home")
                    guest.key("delete")
                    guest.key("end")
                    guest.type("q")
                    screen = guest.screen()
                    deadline = time.monotonic() + 5
                    while not screen.splitlines()[2].endswith("q"):
                        assert time.monotonic() < deadline, screen
                        time.sleep(0.02)
                        screen = guest.screen()
                    assert screen.splitlines()[2].startswith(" bz") and screen.splitlines()[2].endswith("q"), screen
                    assert "Screen only:" in screen.splitlines()[24], screen
                    guest.key("esc")
                    guest.wait_for("User program exited with code 0.")
                    assert frames() == baseline
                    command("run fault.bin", "User program faulted: vector=6")
                    screen = command("echo shell recovered", "\nshell recovered\n")
                    raw = (directory / "vga.bin").read_bytes()
                    for row, line in enumerate(screen.splitlines()):
                        if line.startswith("xinyi:/>"):
                            assert raw[(row * 80) * 2 + 1] == 0x0a
                    assert frames() == baseline
                    print(f"PASS {kernel.parent.name}: keyboard/modifiers, halted waits, PIT sleep, screen bounds/cursor, typing and fault cleanup", flush=True)
                finally:
                    guest.close()
                assert image.read_bytes() == original, "Terminal programs modified the data disk"
    finally:
        (output / "terminal-transcript.txt").write_text("\n\n".join(transcript))


if __name__ == "__main__":
    main()
