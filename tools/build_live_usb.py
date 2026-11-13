"""Build and verify the Live BIOS USB image from a Live kernel and app bundle."""
import argparse
import hashlib
from pathlib import Path
import shutil
import tempfile
import time

from install_qemu import verify_payload
from seed_apps import seed_apps
from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", type=Path, required=True)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Output already exists")
    qemu = shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=args.output.parent) as temp:
        work = Path(temp).resolve()
        image = work / "live.img"
        with image.open("xb") as stream:
            stream.truncate(16 * 1024 * 1024)
        guest = Guest(qemu, args.kernel.resolve(), work, disk=image)
        try:
            guest.command("install", "To install, enter: install confirm")
            guest.type("install confirm\n")
            guest.wait_for("Installation complete:", timeout=45)
        finally:
            guest.close()
        verify_payload(image, args.kernel)
        seed_apps(image, args.package)
        before = image.read_bytes()
        internal = work / "internal.img"
        shutil.copyfile(image, internal)
        boot = [
            "-machine", "q35", "-device", "qemu-xhci,id=xhci",
            "-drive", f"if=none,id=stick,file={image.as_posix()},format=raw",
            "-device", "usb-storage,bus=xhci.0,drive=stick,bootindex=1",
        ]
        for first_boot in (True, False):
            guest = Guest(qemu, None, work, disk=internal, extra_args=boot, boot_message=None)
            try:
                deadline = time.monotonic() + 180
                while "Desktop: ready" not in (work / "debug.log").read_text():
                    if time.monotonic() >= deadline:
                        raise AssertionError((work / "debug.log").read_text()[-2000:])
                    time.sleep(.2)
                assert "Live: RAM volume ready" in (work / "debug.log").read_text()
                guest.key("esc")
                guest.wait_for("xinyi:/", timeout=15)
                guest.command("run hello", "User program exited with code 0.")
                guest.command("disks", "Live RAM session")
                guest.command("use 1", "read-only")
                if first_boot:
                    guest.command("touch LIVE.TXT", "Created:")
                    guest.command("write LIVE.TXT session", "Wrote")
                    guest.command("cat LIVE.TXT", "\nsession\n")
                else:
                    guest.command("cat LIVE.TXT", "not found")
            finally:
                guest.close()
            assert image.read_bytes() == before, "Boot image changed"
            assert internal.read_bytes() == before, "Internal disk changed"
        with args.output.open("xb") as output:
            output.write(before)
        print("PASS Live USB desktop, bundled apps, RAM writes, reboot reset, unchanged boot and internal disks")
        print(f"{args.output.resolve()}\nSHA-256 {hashlib.sha256(before).hexdigest()}")


if __name__ == "__main__":
    main()
