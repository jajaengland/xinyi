"""Install to private IDE images, then test BIOS disk-only boot and reinstall."""
import argparse
import hashlib
from pathlib import Path
import shutil
import struct
import tempfile

from smoke import Guest, ROOT
from storage_qemu import inspect


def fnv(data):
    value = 0x811C9DC5
    for byte in data:
        value = ((value ^ byte) * 0x01000193) & 0xFFFFFFFF
    return value


def verify_payload(image, kernel):
    disk = image.read_bytes()
    header = disk[63*512:64*512]
    assert header[:8] == b"XINYIK1\0"
    version, load, entry, size, memory, checksum = struct.unpack_from("<6I", header, 8)
    assert version == 1 and load == 0x100000 and size <= (2048-64)*512 and memory >= size
    payload = disk[64*512:64*512+size]
    assert fnv(payload) == checksum
    # Reconstruct the initialized image independently from ELF program headers.
    elf = kernel.read_bytes()
    assert entry == struct.unpack_from("<I", elf, 24)[0]
    expected = bytearray(size)
    phoff = struct.unpack_from("<I", elf, 28)[0]
    stride, count = struct.unpack_from("<HH", elf, 42)
    for i in range(count):
        kind, offset, _, physical, filesz, _, _, _ = struct.unpack_from("<8I", elf, phoff+i*stride)
        if kind == 1 and filesz:
            assert physical >= load and physical+filesz <= load+size
            expected[physical-load:physical-load+filesz] = elf[offset:offset+filesz]
    if payload != expected:
        differences = [(hex(i), a, b) for i, (a, b) in enumerate(zip(payload, expected)) if a != b]
        raise AssertionError(f"Installed image differs from ELF: {len(differences)} bytes; first differences {differences[:12]}")
    return disk, size


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel", type=Path, default=ROOT / "target/i686-unknown-none/debug/rust-os")
    parser.add_argument("--output", type=Path, help="Save the verified bootable disk; refuses overwrite")
    parser.add_argument("--disk-index",type=int,choices=[0,1,2,3],default=0)
    args = parser.parse_args()
    if not args.qemu or not args.kernel.is_file():
        parser.error("Build the kernel first and put QEMU on PATH")
    if args.output and args.output.exists():
        parser.error("Output already exists; refusing to overwrite it")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    required = "Install failed: run install first"
    try:
        with tempfile.TemporaryDirectory(prefix="install-", dir=output) as temporary:
            directory = Path(temporary)
            image = directory / "primary.img"
            with image.open("xb") as stream:
                stream.truncate(16*1024*1024)
            guest = Guest(args.qemu, args.kernel.resolve(), directory, disk=image,disk_index=args.disk_index)
            try:
                if args.disk_index:
                    position = f"{'secondary' if args.disk_index>=2 else 'primary'} {'slave' if args.disk_index%2 else 'master'}"
                    guest.command(f"disk select 00:01.1 {position}","Disk target selected")
                before = hashlib.sha256(image.read_bytes()).digest()
                guest.command("install confirm", required)
                guest.command("install", "To install, enter: install confirm")
                guest.command("echo cancel", "\ncancel\n")
                guest.command("install confirm", required)
                assert hashlib.sha256(image.read_bytes()).digest() == before
                # Deliberately change runtime global state before installation.
                guest.command("spawn", "Created task record 1")
                guest.command("touch ram.txt", "Created: ram.txt")
                preview = guest.command("install", "To install, enter: install confirm")
                assert "all existing partitions, files" in preview
                transcript.append(preview)
                guest.type("install confirm\n")
                transcript.append(guest.wait_for("Installation complete:", timeout=30))
                guest.command("install confirm", required)
            finally:
                guest.close()
            verify_payload(image, args.kernel)
            print("PASS install: original ELF image preserved despite runtime mutations", flush=True)
            # No -kernel argument and no removable/CD/floppy drive from here on.
            for boot in (1, 2):
                guest = Guest(args.qemu, None, directory, disk=image, memory_mib=128 if boot == 1 else 32,disk_index=args.disk_index)
                try:
                    transcript.append(guest.command("memory", "0x00100000-"))
                    transcript.append(guest.command("frames", "Freed:"))
                    transcript.append(guest.command("paging", "Paging: enabled"))
                    transcript.append(guest.command("ps", "Total tasks: 0"))
                    guest.command("cat ram.txt", "Storage: file not found")
                    guest.command("mount", "Mounted FAT16 at sector 2048")
                    if boot == 1:
                        guest.command("touch note.txt", "Created: note.txt")
                        guest.command("write note.txt installed disk works", "Wrote 20 bytes")
                    transcript.append(guest.command("cat note.txt", "\ninstalled disk works\n"))
                    if boot == 1:
                        guest.command("install 1", "To install, enter: install confirm")
                        guest.command("unmount", "File commands now use RAM")
                        preview = guest.command("install 1", "To install, enter: install confirm")
                        assert "FAT16 files are preserved" in preview
                        transcript.append(preview)
                        guest.type("install confirm\n")
                        transcript.append(guest.wait_for("Installation complete:", timeout=30))
                    guest.type("reboot\n")
                    guest.process.wait(timeout=5)
                finally:
                    guest.close()
                verify_payload(image, args.kernel)
                assert inspect(image, 2048) == {"NOTE.TXT": b"installed disk works"}
                print(f"PASS disk-only boot {boot}: E820 memory, allocator, shell, filesystem, and persistence", flush=True)
            good, size = verify_payload(image, args.kernel)
            # A corrupt/truncated header or payload must halt in the loader.
            for name, offset, expected in [
                ("checksum",64*512+size//2,"Xinyi disk loader: kernel checksum failed"),
                ("header",63*512,"Xinyi disk loader: invalid kernel header"),
            ]:
                damaged = directory / f"bad-{name}.img"
                content = bytearray(good)
                content[offset] ^= 1
                damaged.write_bytes(content)
                guest = Guest(args.qemu,None,directory,disk=damaged,boot_message=expected,disk_index=args.disk_index)
                try:
                    transcript.append(guest.screen())
                    assert expected in (directory / "debug.log").read_text()
                finally:
                    guest.close()
                print(f"PASS corrupt {name}: loader reports failure and does not enter kernel", flush=True)
            insufficient = directory / "insufficient-ram.img"
            content = bytearray(good)
            struct.pack_into("<I",content,63*512+24,0x1000000)
            insufficient.write_bytes(content)
            expected = "Xinyi disk loader: missing/insufficient E820 RAM"
            guest = Guest(args.qemu,None,directory,disk=insufficient,boot_message=expected,memory_mib=8,disk_index=args.disk_index)
            try:
                transcript.append(guest.screen())
                assert expected in (directory / "debug.log").read_text()
            finally:
                guest.close()
            print("PASS insufficient RAM: loader rejects the image before loading the kernel",flush=True)
            guest = Guest(args.qemu,args.kernel.resolve(),directory,attach_disk=False)
            try:
                transcript.append(guest.command("install","Install refused: disk I/O failed"))
                transcript.append(guest.command("install confirm",required))
                transcript.append(guest.command("echo usable","\nusable\n"))
            finally:
                guest.close()
            print("PASS missing disk: no installation armed and shell remains usable",flush=True)
            if args.output:
                with args.output.open("xb") as stream:
                    stream.write(good)
    finally:
        (output / "install-transcript.txt").write_text("\n".join(transcript),encoding="utf-8")
    print(f"Transcript: {output / 'install-transcript.txt'}")


if __name__ == "__main__":
    main()
