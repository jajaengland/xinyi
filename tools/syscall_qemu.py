"""Check the saved-frame syscall ABI in debug and release QEMU kernels."""
import argparse
from pathlib import Path
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
        parser.error("Build both profiles first and put QEMU on PATH (or supply --kernel)")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="syscall-", dir=output) as temporary:
            directory = Path(temporary)
            for kernel in kernels:
                guest = Guest(args.qemu, kernel.resolve(), directory)
                try:
                    screen = guest.command("usermode", "User program faulted:")
                    debug = (directory / "debug.log").read_text()
                    transcript.append(f"[{kernel}]\n{screen}\n{debug}")
                    assert "Syscall frame: nr=4 args=00000001/00000000/00000000 cs=001b" in debug, debug
                    for number in (20, 4, 0x7fffffff):
                        frame = f"Syscall frame: nr={number} args=11223344/55667788/99aabbcc cs=001b"
                        assert debug.count(frame) == 3, debug
                    assert "vector=13" in debug and "cs=0000001b" in debug, debug
                    print(f"PASS {kernel.parent.name}: saved arguments, returns, GPR/segment/stack/DF/IF preservation; expected ring-3 GPF", flush=True)
                finally:
                    guest.close()
    finally:
        (output / "syscall-transcript.txt").write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {output / 'syscall-transcript.txt'}")


if __name__ == "__main__":
    main()
