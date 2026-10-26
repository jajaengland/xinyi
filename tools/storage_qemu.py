"""Verify FAT16 interoperability and persistence on disposable QEMU images."""
import argparse
from pathlib import Path
import shutil
import struct
import tempfile

from mkfat16 import create_image
from smoke import Guest, ROOT


def inspect(path, start):
    """Independent host reader: compare mirrors, traverse chains, recover files."""
    data = path.read_bytes()
    base = start * 512
    reserved = struct.unpack_from("<H", data, base + 14)[0]
    spc = data[base + 13]
    entries = struct.unpack_from("<H", data, base + 17)[0]
    total = struct.unpack_from("<H", data, base + 19)[0] or struct.unpack_from("<I", data, base + 32)[0]
    spf = struct.unpack_from("<H", data, base + 22)[0]
    first = data[base + reserved * 512:base + (reserved + spf) * 512]
    second = data[base + (reserved + spf) * 512:base + (reserved + 2 * spf) * 512]
    assert first == second, "FAT mirrors differ"
    root = start + reserved + 2 * spf
    data_start = root + (entries * 32 + 511) // 512
    clusters = (total - (data_start - start)) // spc
    files = {}
    allocated = set()
    for slot in range(entries):
        entry = data[root * 512 + slot * 32:root * 512 + (slot + 1) * 32]
        if entry[0] == 0:
            break
        if entry[0] == 0xE5 or entry[11] & 0x18:
            continue
        name = entry[:8].decode().rstrip()
        extension = entry[8:11].decode().rstrip()
        if extension:
            name += "." + extension
        cluster = struct.unpack_from("<H", entry, 26)[0]
        size = struct.unpack_from("<I", entry, 28)[0]
        content = bytearray()
        seen = set()
        while cluster:
            assert 2 <= cluster < 0xFFF0 and cluster not in seen
            assert cluster not in allocated, "Cross-linked file"
            seen.add(cluster)
            allocated.add(cluster)
            assert cluster <= clusters + 1
            offset = (data_start + (cluster - 2) * spc) * 512
            content.extend(data[offset:offset + spc * 512])
            cluster = struct.unpack_from("<H", first, cluster * 2)[0]
            if cluster >= 0xFFF8:
                break
        assert len(content) >= size
        files[name] = bytes(content[:size])
    used = {c for c in range(2, clusters + 2) if struct.unpack_from("<H", first, c * 2)[0] != 0}
    assert used == allocated, "Leaked or unreferenced clusters"
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel", type=Path, default=ROOT / "target/i686-unknown-none/debug/rust-os")
    args = parser.parse_args()
    if not args.qemu or not args.kernel.is_file():
        parser.error("Build the kernel first and put QEMU on PATH")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="storage-", dir=output) as temporary:
            directory = Path(temporary)
            for partitioned in (False, True):
                image = directory / ("mbr.img" if partitioned else "volume.img")
                create_image(image, partitioned)
                start = 63 if partitioned else 0
                # Seed a file on the host; the guest must be able to read it.
                with image.open("r+b") as stream:
                    for copy in range(2):
                        stream.seek((start + 1 + 128 * copy) * 512 + 4)
                        stream.write(b"\xff\xff")
                    entry = bytearray(32)
                    entry[:11] = b"HOST    TXT"
                    entry[11] = 0x20
                    struct.pack_into("<H", entry, 26, 2)
                    struct.pack_into("<I", entry, 28, 9)
                    stream.seek((start + 257) * 512)
                    stream.write(entry)
                    stream.seek((start + 289) * 512)
                    stream.write(b"from host")
                first_sector = image.read_bytes()[:512]
                for boot in (1, 2):
                    guest = Guest(args.qemu, args.kernel.resolve(), directory, disk=image)
                    try:
                        commands = [("mount", f"Mounted FAT16 at sector {start}"),
                                    ("cat host.txt", "\nfrom host\n")]
                        if boot == 1:
                            commands += [
                                ("ls", "HOST.TXT     9 bytes"),
                                ("touch note.txt", "Created: note.txt"),
                                ("touch NOTE.TXT", "Storage: file already exists"),
                                ("touch toolonggg.txt", "Storage: use an ASCII 8.3 filename"),
                                ("write missing no", "Storage: file not found"),
                                ("write note.txt a longer first version", "Wrote 22 bytes"),
                                ("write note.txt", "Wrote 0 bytes"),
                                ("write note.txt survives a cold boot", "Wrote 20 bytes"),
                                ("cat note.txt", "\nsurvives a cold boot\n"),
                                ("ls", "NOTE.TXT     20 bytes"),
                                ("install", "Unmount the filesystem before"),
                                ("unmount", "File commands now use RAM"),
                                ("cat note.txt", "Storage: file not found"),
                                ("mount", f"Mounted FAT16 at sector {start}"),
                            ]
                        commands += [("cat note.txt", "\nsurvives a cold boot\n")]
                        if boot == 2:
                            commands += [("cat gone.txt", "Storage: file not found")]
                        commands += [
                            ("delete", "Usage: delete FILE"),
                            ("delete missing", "Storage: file not found"),
                            ("delete toolonggg.txt", "Storage: use an ASCII 8.3 filename"),
                            ("touch gone.txt", "Created: gone.txt"),
                            ("write gone.txt reclaim this space", "Wrote 18 bytes"),
                            ("delete GONE.TXT", "Deleted: GONE.TXT"),
                            ("cat gone.txt", "Storage: file not found"),
                            ("delete gone.txt", "Storage: file not found"),
                            ("touch empty.txt", "Created: empty.txt"),
                            ("delete empty.txt", "Deleted: empty.txt"),
                            ("ls", "2 file(s)"),
                        ]
                        for command, expected in commands:
                            screen = guest.command(command, expected)
                            transcript.append(f"[partitioned={partitioned}, boot={boot}, {command}]\n{screen}\n")
                        # -no-reboot exits QEMU on the guest's hardware reset.
                        guest.type("reboot\n")
                        guest.process.wait(timeout=5)
                    except BaseException:
                        if guest.process.poll() is None:
                            transcript.append(guest.screen())
                        raise
                    finally:
                        guest.close()
                    assert image.read_bytes()[:512] == first_sector, "Boot sector/MBR changed"
                    assert inspect(image, start) == {"HOST.TXT": b"from host", "NOTE.TXT": b"survives a cold boot"}
                    print(f"PASS {'MBR' if partitioned else 'whole-volume'} boot {boot}: host readback, mirrored FATs, no leaked clusters", flush=True)
            guest = Guest(args.qemu, args.kernel.resolve(), directory, attach_disk=False)
            try:
                for command, expected in [
                    ("disk", "No ATA disk found"),
                    ("mount", "Mount failed: disk I/O failed"),
                    ("touch ram.txt", "Created: ram.txt"),
                    ("write ram.txt still usable", "Wrote 12 bytes"),
                    ("cat ram.txt", "\nstill usable\n"),
                    ("delete RAM.TXT", "Deleted: RAM.TXT"),
                    ("cat ram.txt", "Storage: file not found"),
                    ("delete ram.txt", "Storage: file not found"),
                    ("touch ram.txt", "Created: ram.txt"),
                    ("delete ram.txt", "Deleted: ram.txt"),
                ]:
                    transcript.append(guest.command(command, expected))
                print("PASS missing disk: bounded failure and usable RAM fallback", flush=True)
            finally:
                guest.close()
    finally:
        (output / "storage-transcript.txt").write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {output / 'storage-transcript.txt'}")


if __name__ == "__main__":
    main()
