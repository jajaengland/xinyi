"""Verify high-LBA storage and BIOS installation on private sparse QEMU images."""
import argparse
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile

from mkfat16 import create_image
from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("qemu", "qemu-img", "qemu-io"):
        parser.add_argument("--" + name, default=shutil.which("qemu-system-i386" if name == "qemu" else name))
    parser.add_argument("--kernel", type=Path, default=ROOT / "target/i686-unknown-none/debug/rust-os")
    args = parser.parse_args()
    if not all((args.qemu, args.qemu_img, args.qemu_io, args.kernel.is_file())):
        parser.error("Build the kernel and put QEMU, qemu-img and qemu-io on PATH")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []

    def run(*command):
        return subprocess.check_output(command, text=True, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))

    def io(image, command):
        return subprocess.check_output(
            [args.qemu_io, "-f", "qcow2", "-c", command, str(image)],
            cwd=image.parent, text=True, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))

    def read(image, offset, length):
        dump = io(image, f"read -v {offset} {length}")
        data = bytearray()
        for line in dump.splitlines():
            if re.match(r"^[0-9a-f]+:", line):
                # qemu-io prints 16 hexadecimal bytes followed by ASCII.
                tokens = line.split(":", 1)[1].split()[:16]
                data.extend(int(token, 16) for token in tokens if re.fullmatch(r"[0-9a-f]{2}", token))
        assert len(data) == length, dump
        return bytes(data)

    def command(guest, text, expected):
        screen = guest.command(text, expected)
        transcript.append(f"[{text}]\n{screen}")
        return screen

    try:
        with tempfile.TemporaryDirectory(prefix="lba48-", dir=output) as temporary:
            directory = Path(temporary)
            image = directory / "large.qcow2"
            run(args.qemu_img, "create", "-f", "qcow2", str(image), "160G")
            # Install a small FAT16 fixture beyond the 28-bit sector boundary.
            start, sectors = (1 << 28) + 2048, 32768
            fixture = directory / "volume.img"
            create_image(fixture)
            with fixture.open("r+b") as stream:
                stream.seek(28)
                stream.write(struct.pack("<I", start))
            mbr = bytearray(512)
            mbr[450] = 0x0e
            struct.pack_into("<II", mbr, 454, start, sectors)
            mbr[510:] = b"\x55\xaa"
            mbr_path = directory / "mbr.bin"
            mbr_path.write_bytes(mbr)
            io(image, f"write -s {mbr_path.name} 0 512")
            io(image, f"write -s {fixture.name} {start * 512} {sectors * 512}")
            # A truncated 28-bit address would damage this low-LBA sentinel.
            io(image, "write -P 0x5a 1048576 16777216")
            for boot in (1, 2):
                guest = Guest(args.qemu, args.kernel.resolve(), directory, disk=image, disk_format="qcow2", disk_index=2)
                try:
                    command(guest, "disk select 00:01.1 secondary master", "Disk target selected")
                    screen = command(guest, "disk", "163840 MiB accessible")
                    assert "Addressing: LBA48 PIO" in screen
                    command(guest, "mount", f"Mounted FAT16 at sector {start}")
                    if boot == 1:
                        command(guest, "touch high.txt", "Created: high.txt")
                        command(guest, "write high.txt beyond boundary", "Wrote 15 bytes")
                    command(guest, "cat high.txt", "\nbeyond boundary\n")
                    if boot == 2:
                        command(guest, "unmount", "File commands now use RAM")
                        command(guest, "install", "install confirm")
                        guest.type("install confirm\n")
                        transcript.append(guest.wait_for("Installation complete:", timeout=30))
                finally:
                    guest.close()
                assert read(image, (start + 1) * 512, 512) == read(image, (start + 129) * 512, 512)
                root = read(image, (start + 257) * 512, 512)
                assert root[:11] == b"HIGH    TXT" and struct.unpack_from("<I", root, 28)[0] == 15
                cluster = struct.unpack_from("<H", root, 26)[0]
                assert read(image, (start + 289 + cluster - 2) * 512, 512)[:15] == b"beyond boundary"
                io(image, "read -P 0x5a 1048576 16777216")
            guest = Guest(args.qemu, None, directory, disk=image, disk_format="qcow2", disk_index=2)
            try:
                command(guest, "mount", f"Mounted FAT16 at sector {start}")
                command(guest, "cat high.txt", "\nbeyond boundary\n")
            finally:
                guest.close()
            print("PASS high-LBA FAT16: host readback, FAT mirrors, cold boots, preserving install and disk-only boot; low aliases intact", flush=True)

            # Fresh installation must clear GPT remnants at the true disk end.
            fresh = directory / "fresh.qcow2"
            run(args.qemu_img, "create", "-f", "qcow2", str(fresh), "160G")
            tail = 160 * 1024**3 - 33 * 512
            io(fresh, f"write -P 0xa5 {tail} {33 * 512}")
            guest = Guest(args.qemu, args.kernel.resolve(), directory, disk=fresh, disk_format="qcow2")
            try:
                command(guest, "install", "install confirm")
                guest.type("install confirm\n")
                transcript.append(guest.wait_for("Installation complete:", timeout=30))
            finally:
                guest.close()
            io(fresh, f"read -P 0 {tail} {33 * 512}")
            guest = Guest(args.qemu, None, directory, disk=fresh, disk_format="qcow2")
            try:
                command(guest, "mount", "Mounted FAT16 at sector 2048")
                command(guest, "touch fresh.txt", "Created: fresh.txt")
                command(guest, "write fresh.txt installed", "Wrote 9 bytes")
                command(guest, "cat fresh.txt", "\ninstalled\n")
            finally:
                guest.close()
            print("PASS 160 GiB fresh install: actual-end GPT cleanup and disk-only boot with usable FAT16", flush=True)
    finally:
        (output / "lba48-transcript.txt").write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {output / 'lba48-transcript.txt'}")


if __name__ == "__main__":
    main()
