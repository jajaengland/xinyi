"""Create a new 16 MiB FAT16 data image; never overwrites an existing file."""
import argparse
from pathlib import Path
import struct


def create_image(path, partitioned=False):
    start = 63 if partitioned else 0
    sectors, fat_sectors, root_entries = 32768, 128, 512
    boot = bytearray(512)
    boot[:11] = b"\xeb\x3c\x90XINYI   "
    struct.pack_into("<HBHBHHBHHHII", boot, 11,
                     512, 1, 1, 2, root_entries, sectors, 0xF8,
                     fat_sectors, 63, 16, start, 0)
    boot[36] = 0x80
    boot[38] = 0x29
    struct.pack_into("<I", boot, 39, 0x58494E59)
    boot[43:54] = b"XINYI DATA "
    boot[54:62] = b"FAT16   "
    boot[510:512] = b"\x55\xaa"
    with Path(path).open("xb") as stream:
        stream.truncate((start + sectors) * 512)
        if partitioned:
            mbr = bytearray(512)
            mbr[446 + 4] = 0x06
            # LBA partition; CHS is a saturated compatibility placeholder.
            mbr[447:450] = mbr[451:454] = b"\xfe\xff\xff"
            struct.pack_into("<II", mbr, 454, start, sectors)
            mbr[510:512] = b"\x55\xaa"
            stream.write(mbr)
        stream.seek(start * 512)
        stream.write(boot)
        for copy in range(2):
            stream.seek((start + 1 + copy * fat_sectors) * 512)
            stream.write(b"\xf8\xff\xff\xff")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path)
    parser.add_argument("--partitioned", action="store_true")
    args = parser.parse_args()
    create_image(args.path, args.partitioned)
    print(f"Created FAT16 data image: {args.path.resolve()} (not a bootable OS image)")
