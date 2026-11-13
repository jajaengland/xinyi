"""Install from USB onto a private SATA disk and boot it without the USB."""
import argparse
from pathlib import Path
import shutil
import tempfile

from folders_qemu import inspect_tree
from install_qemu import verify_payload
from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disk", type=Path, required=True)
    parser.add_argument("--kernel", type=Path, required=True)
    args = parser.parse_args()
    qemu = shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    with tempfile.TemporaryDirectory(prefix="sata-install-", dir=ROOT / "target") as tmp:
        work = Path(tmp)
        usb = work / "usb.img"
        sata = work / "sata.img"
        shutil.copyfile(args.disk, usb)
        apps = inspect_tree(usb, 2048)[0]
        with sata.open("xb") as disk:
            disk.truncate(16 * 1024 * 1024)
        empty = sata.read_bytes()
        args_usb = [
            "-machine", "q35",
            "-drive", f"if=none,id=stick,file={usb.as_posix()},format=raw",
            "-device", "qemu-xhci,id=xhci",
            "-device", "usb-storage,bus=xhci.0,drive=stick,bootindex=1",
        ]
        guest = Guest(qemu, None, work, disk=sata, extra_args=args_usb)
        try:
            guest.command("disks", "QEMU HARDDISK")
            guest.type("install 1\n")
            guest.wait_for("To install, enter: install confirm", timeout=120)
            guest.command("echo cancelled", "\ncancelled\n")
            assert sata.read_bytes() == empty, "Cancelled install wrote to SATA"
            guest.command("use 0", "Drive 0 ready")
            guest.type("install 1\n")
            guest.wait_for("To install, enter: install confirm", timeout=120)
            guest.type("install confirm\n")
            guest.wait_for("Installed drive ready", timeout=150)
        finally:
            guest.close()
            shutil.copyfile(work / "debug.log", ROOT / "target/ahci-install-debug.log")
        verify_payload(sata, args.kernel.resolve())
        files, _ = inspect_tree(sata, 2048)
        assert files == {name: data for name, data in apps.items() if name.startswith("APPS/")}
        guest = Guest(qemu, None, work, disk=sata, extra_args=["-machine", "q35"])
        try:
            guest.command("apps", "HARDWARE.BIN")
            guest.command("install 1", "Install refused:")
            assert "Selected: AHCI" in guest.command("disk", "Read sector 0 successfully")
            guest.command("touch SATA.TXT", "Created:")
            guest.command("write SATA.TXT persisted", "Wrote")
            guest.command("cat SATA.TXT", "\npersisted\n")
        finally:
            guest.close()
        assert inspect_tree(sata, 2048)[0]["SATA.TXT"] == b"persisted"
    print("PASS USB to SATA: cancellation, all apps, native install, cold boot, source-disk refusal and persistence")


if __name__ == "__main__":
    main()
