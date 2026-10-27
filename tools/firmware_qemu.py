"""Boot a fresh USB system and exercise BIOS-backed application/file access."""
import argparse
import hashlib
from pathlib import Path
import shutil
import re
import tempfile
import time
from install_qemu import verify_payload
from seed_apps import seed_apps
from smoke import Guest,ROOT
from folders_qemu import inspect_tree


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel",type=Path,default=ROOT/"target/i686-unknown-none/debug/rust-os")
    parser.add_argument("--package",type=Path,default=ROOT/"target/system-package")
    parser.add_argument("--output",type=Path)
    parser.add_argument("--vga",default="cirrus")
    args=parser.parse_args();kernel=args.kernel.resolve()
    qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    parent=ROOT/"target/smoke";parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="firmware-",dir=parent) as temporary:
        directory=Path(temporary);image=directory/"stick.img"
        with image.open("xb") as stream:stream.truncate(16*1024*1024)
        guest=Guest(qemu,kernel,directory,disk=image)
        try:
            guest.command("install","To install, enter: install confirm")
            guest.type("install confirm\n");guest.wait_for("Installation complete:",timeout=45)
        finally:guest.close()
        verify_payload(image,kernel);seed_apps(image,args.package)
        try:
            guest=Guest(qemu,None,directory,attach_disk=False,extra_args=[
                "-drive",f"if=none,id=stick,file={image.as_posix()},format=raw",
                "-device","piix3-usb-uhci,id=uhci","-device","usb-storage,bus=uhci.0,drive=stick,bootindex=1","-vga",args.vga])
            try:
                guest.command("apps","EDIT.BIN")
                guest.command("run hello","User program exited with code 0.")
                guest.command("edit usb.txt","Xinyi Editor");guest.type("saved from usb")
                guest.key("ctrl-s");guest.wait_for("Saved to disk",timeout=15)
                guest.key("ctrl-q");guest.wait_for("User program exited with code 0.")
                guest.command("cat usb.txt","saved from usb")
                guest.command("bios","VBE mode")
                baseline=int(re.search(r"Free now: +(\d+)",guest.command("frames","Free now:"))[1])
                for cycle in range(2):
                    offset=len((directory/"debug.log").read_text())
                    guest.type("graphics\n");deadline=time.monotonic()+20
                    while "Graphics: frame ready" not in (directory/"debug.log").read_text()[offset:]:
                        assert time.monotonic()<deadline,guest.screen();time.sleep(.02)
                    log=(directory/"debug.log").read_text()[offset:]
                    if args.vga!="std":
                        mode=re.search(r"Graphics: BIOS VBE mode [0-9a-f]+ base=([0-9a-f]+) pitch=(\d+) bpp=(\d+)",log)
                        assert mode,log
                        base=int(mode[1],16);pitch=int(mode[2]);bpp=int(mode[3]);size=bpp//8
                        def pixel(x,y):
                            path=directory/"pixel.bin"
                            guest.call("human-monitor-command",{"command-line":f'pmemsave {base+y*pitch+x*size:#x} {size} "{path.as_posix()}"'})
                            return int.from_bytes(path.read_bytes(),"little")
                        def encoded(rgb):
                            return ((rgb>>19&31)<<11|(rgb>>10&63)<<5|(rgb>>3&31)) if bpp==16 else rgb
                        for x,y,rgb in [(0,0,0x176c70),(235,360,0xe5a34f),(570,295,0x8fe186),(570,370,0x5865ac),(350,340,0x58ccd0)]:
                            assert pixel(x,y)==encoded(rgb),(x,y,hex(pixel(x,y)),hex(encoded(rgb)))
                    guest.call("screendump",{"filename":str(parent/f"vbe-{args.vga}.png"),"format":"png"})
                    guest.key("right")
                    if args.vga!="std":assert pixel(350,340)==encoded(0x202833),"Box did not move"
                    guest.key("esc");guest.wait_for("Returned to text mode.",timeout=15)
                    assert baseline==int(re.search(r"Free now: +(\d+)",guest.command("frames","Free now:"))[1]),"VESA buffers leaked"
                guest.command("cat usb.txt","saved from usb")
            finally:guest.close()
        finally:
            if (directory/"debug.log").exists():shutil.copyfile(directory/"debug.log",parent/"firmware-debug.log")
            if (directory/"qemu.stderr").exists():shutil.copyfile(directory/"qemu.stderr",parent/"firmware-stderr.log")
        files,_=inspect_tree(image,2048);assert files["USB.TXT"]==b"saved from usb"
        destination=directory/"installed.img"
        with destination.open("xb") as stream:stream.truncate(16*1024*1024)
        guest=Guest(qemu,None,directory,disk=destination,extra_args=[
            "-drive",f"if=none,id=stick,file={image.as_posix()},format=raw",
            "-device","piix3-usb-uhci,id=uhci","-device","usb-storage,bus=uhci.0,drive=stick,bootindex=1","-vga",args.vga])
        try:
            guest.command("cat usb.txt","saved from usb")
            guest.command("install","Install refused: choose an IDE destination")
            guest.command("install 1","To install, enter: install confirm")
            guest.type("install confirm\n");guest.wait_for("Installed drive ready",timeout=60)
            guest.command("apps","EDIT.BIN")
        finally:guest.close()
        verify_payload(destination,kernel)
        installed,_=inspect_tree(destination,2048)
        assert {k:v for k,v in files.items() if k.startswith("APPS/")}==installed
        guest=Guest(qemu,None,directory,disk=destination)
        try:
            guest.command("apps","EDIT.BIN")
            guest.command("run hello","User program exited with code 0.")
        finally:guest.close()
        # Firmware write failures must retain the editor document and report failure.
        before=hashlib.sha256(image.read_bytes()).digest()
        guest=Guest(qemu,None,directory,attach_disk=False,extra_args=[
            "-drive",f"if=none,id=stick,file={image.as_posix()},format=raw,readonly=on",
            "-device","piix3-usb-uhci,id=uhci","-device","usb-storage,bus=uhci.0,drive=stick,bootindex=1","-vga",args.vga])
        try:
            guest.command("edit fail.txt","Xinyi Editor");guest.type("retain me")
            guest.key("ctrl-s");guest.wait_for("Save failed: disk error; document retained",timeout=20)
            assert "retain me" in guest.screen()
        finally:guest.close()
        assert hashlib.sha256(image.read_bytes()).digest()==before
        # A valid IDE filesystem must not be substituted for a broken boot USB.
        damaged=directory/"bad-filesystem.img";shutil.copyfile(image,damaged)
        with damaged.open("r+b") as stream:stream.seek(2048*512+510);stream.write(b"\0\0")
        guest=Guest(qemu,None,directory,disk=destination,extra_args=[
            "-drive",f"if=none,id=stick,file={damaged.as_posix()},format=raw",
            "-device","piix3-usb-uhci,id=uhci","-device","usb-storage,bus=uhci.0,drive=stick,bootindex=1","-vga",args.vga])
        try:
            assert "Boot filesystem unavailable" in guest.screen()
            screen=guest.command("apps","Applications:");assert "EDIT.BIN" not in screen
            guest.command("use 1","Files are persistent.");guest.command("apps","EDIT.BIN")
            offset=len((directory/"debug.log").read_text());guest.type("graphics\n");deadline=time.monotonic()+20
            while "Graphics: frame ready" not in (directory/"debug.log").read_text()[offset:]:
                assert time.monotonic()<deadline;time.sleep(.02)
            guest.call("inject-nmi");guest.wait_for("System halted.",timeout=15)
        finally:guest.close()
        if args.output:
            with args.output.open("xb") as stream:stream.write(image.read_bytes())
    print("PASS BIOS-backed USB and VESA: /APPS loading, protected programs, editor read/write, graphics/text recovery, cold-boot persistence, USB-to-IDE installation, read-only errors, no wrong-disk fallback and fatal text recovery")


if __name__=="__main__":main()
