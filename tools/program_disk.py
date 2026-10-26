"""Create a new FAT16 data image containing small Xinyi program binaries."""
import argparse
from pathlib import Path
import re
import struct

from mkfat16 import create_image


def create_program_disk(path, files):
    # Fixed mkfat16 geometry: a 16 MiB volume, 512 root entries, 128-sector FATs.
    if len(files) > 512:
        raise ValueError("Too many programs for the root directory")
    normalized = {}
    for name, data in files.items():
        if not re.fullmatch(r"[A-Za-z0-9_-]{1,8}\.[A-Za-z0-9_-]{1,3}", name):
            raise ValueError("Use ASCII 8.3 names such as HELLO.BIN")
        if len(data) > 1024:
            raise ValueError("Program exceeds Xinyi's current 1024-byte file limit")
        name = name.upper()
        if name in normalized:
            raise ValueError("Duplicate program name")
        normalized[name] = data
    create_image(path, partitioned=True)  # Exclusive creation; never overwrite.
    start, cluster = 63, 2
    with Path(path).open("r+b") as stream:
        for slot, (name, data) in enumerate(normalized.items()):
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
            for offset in (16, 18, 24):
                struct.pack_into("<H", entry, offset, 0x21)
            struct.pack_into("<H", entry, 26, first)
            struct.pack_into("<I", entry, 28, len(data))
            stream.seek((start + 257) * 512 + slot * 32)
            stream.write(entry)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    parser.add_argument("--file", action="append", default=[], metavar="NAME=PATH")
    args = parser.parse_args()
    files = {}
    for entry in args.file:
        name, separator, source = entry.partition("=")
        if not separator or not source or name.upper() in files:
            parser.error("Use unique --file HELLO.BIN=path/to/hello.bin entries")
        files[name.upper()] = Path(source).read_bytes()
    create_program_disk(args.image, files)
    print(f"Created {args.image.resolve()} with {len(files)} file(s); data disk, not a bootable OS image")


if __name__ == "__main__":
    main()
