"""Read-only PCI/ATA diagnostic checks on private disposable QEMU disks."""
import argparse
import hashlib
from pathlib import Path
import shutil
import tempfile

from smoke import Guest, ROOT
from mkfat16 import create_image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu",default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel",type=Path,default=ROOT / "target/i686-unknown-none/debug/rust-os")
    args = parser.parse_args()
    if not args.qemu or not args.kernel.is_file():
        parser.error("Build the kernel first and put QEMU on PATH")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True,exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="ata-",dir=output) as temporary:
            directory = Path(temporary)
            disk = directory / "test.img"
            create_image(disk)
            before = hashlib.sha256(disk.read_bytes()).digest()
            for index in (0,1,2):
                guest = Guest(args.qemu,args.kernel.resolve(),directory,disk=disk,disk_index=index)
                try:
                    if index == 0:
                        transcript.append(guest.command("disk","Valid boot signature"))
                        guest.command("mount","Mounted FAT16 at sector 0")
                        transcript.append(guest.command("disk scan","Unmount the filesystem before scanning"))
                        guest.command("unmount","File commands now use RAM")
                    else:
                        transcript.append(guest.command("disk","No ATA disk found"))
                    screen = guest.command("disk scan","Scan leaves the selected target unchanged.")
                    transcript.append(f"[index={index}]\n{screen}\n")
                    assert "class=01/01" in screen
                    expected = "I/O=0x0170 ctrl=0x0376 master: identified" if index == 2 else f"I/O=0x01f0 ctrl=0x03f6 {'master' if index == 0 else 'slave'}: identified"
                    assert expected in screen, screen
                    assert "QEMU HARDDISK" in screen, screen
                    if index != 0:
                        # Scanning must never redirect mount/format/install to a found disk.
                        guest.command("mount","Mount failed: disk I/O failed")
                        guest.command("install","Install refused: disk I/O failed")
                    position = "secondary master" if index == 2 else f"primary {'master' if index == 0 else 'slave'}"
                    selected = guest.command(f"disk select 00:01.1 {position}","Disk target selected")
                    transcript.append(selected)
                    assert "QEMU HARDDISK" in selected
                    guest.command("disk","Valid boot signature")
                    guest.command("disk select 00:1f.7 primary master","Selection refused")
                    guest.command("mount","Mounted FAT16 at sector 0")
                    guest.command("disk select legacy primary master","Unmount the filesystem before changing")
                    guest.command("cat missing","Storage: file not found")
                    guest.command("unmount","File commands now use RAM")
                finally:
                    guest.close()
                assert hashlib.sha256(disk.read_bytes()).digest() == before
                print(f"PASS IDE position {index}: scan identifies disk without writes or target changes",flush=True)
            guest = Guest(args.qemu,args.kernel.resolve(),directory,disk=disk,extra_args=["-machine","q35"])
            try:
                transcript.append(guest.command("disk","PCI IDE routing unavailable/disabled"))
                screen = guest.command("disk scan","No PCI IDE controller found")
                transcript.append(screen)
                assert "class=01/06" in screen and "AHCI/RAID are not supported" in screen, screen
            finally:
                guest.close()
            assert hashlib.sha256(disk.read_bytes()).digest() == before
            print("PASS AHCI: reported as unsupported without probing/writing ATA media",flush=True)
    finally:
        (output / "ata-transcript.txt").write_text("\n".join(transcript),encoding="utf-8")
    print(f"Transcript: {output / 'ata-transcript.txt'}")


if __name__ == "__main__":
    main()
