"""Test destructive format only on private disposable QEMU images."""
import argparse
import hashlib
from pathlib import Path
import shutil
import tempfile

from mkfat16 import create_image
from smoke import Guest, ROOT
from storage_qemu import inspect


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel", type=Path, default=ROOT / "target/i686-unknown-none/debug/rust-os")
    parser.add_argument("--disk-index",type=int,choices=[0,1,2,3],default=0)
    args = parser.parse_args()
    if not args.qemu or not args.kernel.is_file():
        parser.error("Build the kernel first and put QEMU on PATH")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    required = "Format failed: run format first"
    try:
        with tempfile.TemporaryDirectory(prefix="format-", dir=output) as temporary:
            directory = Path(temporary)
            for blank in (False, True):
                image = directory / ("blank.img" if blank else "old-fat16.img")
                if blank:
                    with image.open("xb") as stream:
                        stream.truncate(16 * 1024 * 1024)
                else:
                    create_image(image)
                guest = Guest(args.qemu, args.kernel.resolve(), directory, disk=image,disk_index=args.disk_index)
                try:
                    if args.disk_index:
                        position = f"{'secondary' if args.disk_index>=2 else 'primary'} {'slave' if args.disk_index%2 else 'master'}"
                        guest.command(f"disk select 00:01.1 {position}","Disk target selected")
                    def command(text, expected):
                        screen = guest.command(text, expected)
                        transcript.append(f"[blank={blank}, {text}]\n{screen}\n")
                        return screen
                    if not blank:
                        command("mount", "Mounted FAT16 at sector 0")
                        command("touch old.txt", "Created: old.txt")
                        command("write old.txt old data", "Wrote 8 bytes")
                        command("format", "Format refused: unmount the disk")
                        command("format confirm", required)
                        command("cat old.txt", "\nold data\n")
                        command("unmount", "File commands now use RAM")
                    before = hashlib.sha256(image.read_bytes()).digest()
                    command("format confirm", required)
                    command("format", "To erase this disk, enter: format confirm")
                    command("echo cancel", "\ncancel\n")
                    command("format confirm", required)
                    assert hashlib.sha256(image.read_bytes()).digest() == before, "Preview/cancellation wrote to disk"
                    preview = command("format", "To erase this disk, enter: format confirm")
                    assert "removes all partitions, files, and bootability" in preview
                    assert "selected IDE disk" in preview
                    # Guest.command() issues clear first, so confirmation deliberately
                    # uses raw input: it must immediately follow the preview.
                    guest.type("format confirm\n")
                    transcript.append(guest.wait_for("Format complete. Run mount", timeout=15))
                    command("format confirm", required)  # Confirmation is consumed.
                    command("mount", "Mounted FAT16 at sector 2048")
                    command("ls", "0 file(s)")
                    command("cat old.txt", "Storage: file not found")
                    command("touch new.txt", "Created: new.txt")
                    command("write new.txt formatted and persistent", "Wrote 24 bytes")
                    guest.type("reboot\n")
                    guest.process.wait(timeout=5)
                except BaseException:
                    if guest.process.poll() is None:
                        transcript.append(guest.screen())
                    raise
                finally:
                    guest.close()
                assert inspect(image, 2048) == {"NEW.TXT": b"formatted and persistent"}
                guest = Guest(args.qemu, args.kernel.resolve(), directory, disk=image,disk_index=args.disk_index)
                try:
                    if args.disk_index:
                        guest.command(f"disk select 00:01.1 {position}","Disk target selected")
                    transcript.append(guest.command("mount", "Mounted FAT16 at sector 2048"))
                    transcript.append(guest.command("cat new.txt", "\nformatted and persistent\n"))
                finally:
                    guest.close()
                print(f"PASS {'blank' if blank else 'existing'} disk: confirmation, format, host readback, and cold-boot persistence", flush=True)
            guest = Guest(args.qemu, args.kernel.resolve(), directory, attach_disk=False)
            try:
                transcript.append(guest.command("format", "Format refused: disk I/O failed"))
                transcript.append(guest.command("format confirm", required))
                transcript.append(guest.command("echo usable", "\nusable\n"))
                print("PASS missing disk: no format armed and shell remains usable", flush=True)
            finally:
                guest.close()
    finally:
        (output / "format-transcript.txt").write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {output / 'format-transcript.txt'}")


if __name__ == "__main__":
    main()
