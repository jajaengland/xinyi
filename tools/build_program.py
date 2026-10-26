"""Build a freestanding Rust source into a fixed-address Xinyi flat image."""
import argparse
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import sys

ROOT = Path(__file__).resolve().parents[1]
BASE = 0x40000000


def flat_image(elf):
    if elf[:7] != b"\x7fELF\x01\x01\x01" or struct.unpack_from("<H", elf, 18)[0] != 3:
        raise ValueError("Expected a little-endian i386 ELF image")
    if struct.unpack_from("<I", elf, 24)[0] != BASE:
        raise ValueError("Program entry must be 0x40000000")
    offset = struct.unpack_from("<I", elf, 28)[0]
    size, count = struct.unpack_from("<HH", elf, 42)
    image = bytearray()
    for index in range(count):
        kind, file_offset, address, _, file_size, memory_size, flags, _ = struct.unpack_from("<8I", elf, offset + index * size)
        if kind != 1 or memory_size == 0:
            continue
        if address < BASE or address + memory_size > BASE + 65536 or memory_size != file_size or flags & 2:
            raise ValueError("Program must be a read-only image within the 64 KiB user code area")
        end = address + file_size - BASE
        if end > len(image):
            image.extend(bytes(end - len(image)))
        image[address - BASE:end] = elf[file_offset:file_offset + file_size]
    if not image:
        raise ValueError("Program has no loadable bytes")
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()
    if not args.source.is_file():
        parser.error("Source file does not exist")
    if args.output.exists() and not args.force:
        parser.error("Output exists; use --force to replace it")
    def run(*command):
        return subprocess.check_output(command, text=True)
    sysroot = Path(run("rustc", "+nightly", "--print", "sysroot").strip())
    host = next(line.split(": ", 1)[1] for line in run("rustc", "+nightly", "-vV").splitlines() if line.startswith("host: "))
    linker = sysroot / "lib/rustlib" / host / "bin" / ("rust-lld.exe" if host.endswith("windows-msvc") or host.endswith("windows-gnu") else "rust-lld")
    (ROOT / "target").mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="xinyi-program-", dir=ROOT / "target") as temporary:
        manifest = Path(temporary) / "Cargo.toml"
        manifest.write_text('[package]\nname="xinyi-user-program"\nversion="0.1.0"\nedition="2021"\n'
                            '[workspace]\n[[bin]]\nname="xinyi-user-program"\npath=' + json.dumps(args.source.resolve().as_posix()) +
                            '\n[profile.release]\npanic="abort"\nopt-level="s"\n', encoding="utf-8")
        built = subprocess.run(["cargo", "+nightly", "rustc", "--manifest-path", str(manifest),
                        "--target", str(ROOT / "i686-unknown-none.json"), "--target-dir", str(ROOT / "target/user-build"),
                        "--release", "--offline", "-Z", "build-std=core", "--message-format=json", "--",
                        "-C", "relocation-model=static",
                        "-C", f"linker={linker}", "-C", f"link-arg=-T{ROOT / 'sdk/user.ld'}",
                        "-C", "link-arg=--gc-sections"], capture_output=True, text=True)
        sys.stderr.write(built.stderr)
        artifacts = []
        for line in built.stdout.splitlines():
            record = json.loads(line)
            if record.get("reason") == "compiler-message":
                sys.stderr.write(record["message"].get("rendered", ""))
            elif record.get("reason") == "compiler-artifact" and record.get("executable"):
                artifacts.append(Path(record["executable"]))
        built.check_returncode()
        elf_path = artifacts[-1]
        image = flat_image(elf_path.read_bytes())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("wb" if args.force else "xb") as stream:
        stream.write(image)
    print(f"Built {args.output.resolve()}: {len(image)} bytes")
    if len(image) > 1024:
        print("This exceeds the current FAT16 file limit; use a smaller program until larger files are supported.")


if __name__ == "__main__":
    main()
