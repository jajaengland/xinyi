"""Verify legacy BIOS boot from emulated USB mass storage, without IDE disks."""
import argparse
import hashlib
from pathlib import Path
import shutil
import tempfile
import time

from install_qemu import verify_payload
from smoke import Guest,ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image",type=Path,required=True)
    parser.add_argument("--kernel",type=Path,required=True)
    args=parser.parse_args();image=args.image.resolve();kernel=args.kernel.resolve()
    verify_payload(image,kernel)
    original=hashlib.sha256(image.read_bytes()).digest()
    qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    output=ROOT/"target/smoke";output.mkdir(parents=True,exist_ok=True);transcript=[]
    try:
        with tempfile.TemporaryDirectory(prefix="usb-boot-",dir=output) as temporary:
            directory=Path(temporary)
            guest=Guest(qemu,None,directory,attach_disk=False,extra_args=[
                "-drive",f"if=none,id=stick,file={image.as_posix()},format=raw",
                "-device","piix3-usb-uhci,id=uhci",
                "-device","usb-storage,bus=uhci.0,drive=stick,bootindex=1",
                "-vga","cirrus",
            ])
            try:
                transcript.append(guest.command("version","Xinyi kernel version"))
                transcript.append(guest.command("disks","No supported IDE drives found"))
                transcript.append(guest.command("apps","EDIT.BIN"))
                transcript.append(guest.command("run hello","User program exited with code 0."))
                transcript.append(guest.command("bios","VBE mode"))
                transcript.append(guest.command("echo usb boot works","\nusb boot works\n"))
                guest.type("graphics\n");deadline=time.monotonic()+15
                while "Graphics: frame ready" not in (directory/"debug.log").read_text():
                    assert time.monotonic()<deadline,guest.screen();time.sleep(.02)
                guest.key("esc");transcript.append(guest.wait_for("Returned to text mode.",timeout=10))
                offset=len((directory/"debug.log").read_text());guest.type("mouse\n");deadline=time.monotonic()+20
                while "Mouse: frame ready" not in (directory/"debug.log").read_text()[offset:]:
                    assert time.monotonic()<deadline,guest.screen();time.sleep(.02)
                guest.call("input-send-event",{"events":[{"type":"rel","data":{"axis":"x","value":-100}},{"type":"rel","data":{"axis":"y","value":-40}}]})
                time.sleep(.08)
                for down in (True,False):
                    guest.call("input-send-event",{"events":[{"type":"btn","data":{"button":"left","down":down}}]});time.sleep(.08)
                deadline=time.monotonic()+8
                while "clicks=1/0 menu=false" not in (directory/"debug.log").read_text()[offset:]:
                    assert time.monotonic()<deadline;time.sleep(.02)
                guest.key("esc");guest.wait_for("Returned to text mode.")
                transcript.append(guest.command("apps","EDIT.BIN"))
                offset=len((directory/"debug.log").read_text());guest.type("desktop\n");deadline=time.monotonic()+20
                while "Desktop: frame ready" not in (directory/"debug.log").read_text()[offset:]:
                    assert time.monotonic()<deadline,guest.screen();time.sleep(.02)
                guest.call("input-send-event",{"events":[{"type":"rel","data":{"axis":"x","value":-250}},{"type":"rel","data":{"axis":"y","value":30}}]})
                time.sleep(.1)
                for down in (True,False):
                    guest.call("input-send-event",{"events":[{"type":"btn","data":{"button":"left","down":down}}]});time.sleep(.08)
                deadline=time.monotonic()+8
                while "top=0 clicks=0/0" not in (directory/"debug.log").read_text()[offset:]:
                    assert time.monotonic()<deadline;time.sleep(.02)
                guest.key("esc");guest.wait_for("Returned to text mode.")
                transcript.append(guest.command("echo usable","\nusable\n"))
            finally:guest.close()
        assert hashlib.sha256(image.read_bytes()).digest()==original,"USB boot modified image contents"
        print("PASS USB-only BIOS boot: bootloader/kernel, apps, keyboard, VESA, PS/2 mouse and desktop focus/text recovery; image unchanged")
        print("Boot filesystem and applications accessed through BIOS; generic VESA display exercised.")
    finally:(output/"usb-boot-transcript.txt").write_text("\n\n".join(transcript))


if __name__=="__main__":main()
