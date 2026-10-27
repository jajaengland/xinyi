"""Exercise real PS/2 IRQ packets, cursor pixels, buttons, menus and recovery."""
import argparse
from pathlib import Path
import re
import shutil
import tempfile
import time
from smoke import Guest,ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel",type=Path,default=ROOT/"target/i686-unknown-none/debug/rust-os")
    parser.add_argument("--disk",type=Path,help="Private bootable image, tested using a disposable copy")
    parser.add_argument("--vga",default="std")
    args=parser.parse_args();qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    output=ROOT/"target/smoke";output.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="mouse-",dir=output) as temporary:
        directory=Path(temporary);disk=None
        if args.disk:disk=directory/"boot.img";shutil.copyfile(args.disk,disk)
        guest=Guest(qemu,None if disk else args.kernel,directory,disk=disk,attach_disk=disk is not None,extra_args=["-vga",args.vga])
        try:
            def frames():return int(re.search(r"Free now: +(\d+)",guest.command("frames","Free now:"))[1])
            baseline=frames()
            for cycle in range(2):
                offset=len((directory/"debug.log").read_text());guest.type("mouse\n");deadline=time.monotonic()+20
                while "Mouse: frame ready" not in (directory/"debug.log").read_text()[offset:]:
                    assert time.monotonic()<deadline,guest.screen();time.sleep(.02)
                log=(directory/"debug.log").read_text()[offset:]
                if args.vga=="std":
                    dev=next(d for d in guest.call("query-pci")[0]["devices"] if d.get("id",{}).get("vendor")==0x1234)
                    base=next(r["address"] for r in dev["regions"] if r["bar"]==0);pitch=3200;size=4
                else:
                    mode=re.search(r"BIOS VBE mode [0-9a-f]+ base=([0-9a-f]+) pitch=(\d+) bpp=(\d+)",log)
                    assert mode,log;base=int(mode[1],16);pitch=int(mode[2]);size=int(mode[3])//8
                def pixel(x,y):
                    path=directory/"pixel.bin"
                    guest.call("human-monitor-command",{"command-line":f'pmemsave {base+y*pitch+x*size:#x} {size} "{path.as_posix()}"'})
                    return int.from_bytes(path.read_bytes(),"little")
                def until(condition,message):
                    deadline=time.monotonic()+8
                    while not condition():assert time.monotonic()<deadline,message;time.sleep(.01)
                def ui_message(text):until(lambda:text in (directory/"debug.log").read_text()[offset:],text)
                pos=[400,300]
                assert pixel(0,0)==0x80 and pixel(400,300)==0 and pixel(401,302)==0xffffff
                def move(x,y):
                    old=tuple(pos)
                    while pos!=[x,y]:
                        dx=max(-100,min(100,x-pos[0]));dy=max(-100,min(100,y-pos[1]))
                        guest.call("input-send-event",{"events":[{"type":"rel","data":{"axis":"x","value":dx}},{"type":"rel","data":{"axis":"y","value":dy}}]})
                        pos[0]+=dx;pos[1]+=dy;time.sleep(.03)
                    until(lambda:pixel(x,y)==0,"Cursor did not reach requested position")
                    return old
                def button(which,down):
                    guest.call("input-send-event",{"events":[{"type":"btn","data":{"button":which,"down":down}}]});time.sleep(.05)
                def click(which="left"):button(which,True);button(which,False)
                move(300,260);assert pixel(400,300)==0x80,"Cursor trail left behind"
                click();ui_message("clicks=1/0 menu=false")
                move(460,260);click();ui_message("clicks=1/1 menu=false")
                button("left",True);move(600,350);button("left",False)
                # Capture must not activate a button after dragging away.
                time.sleep(.05);assert "clicks=1/2" not in (directory/"debug.log").read_text()[offset:]
                click("right");ui_message("clicks=1/1 menu=true");assert pixel(602,352)!=0x80
                guest.call("screendump",{"filename":str(output/f"mouse-{args.vga}.png"),"format":"png"})
                move(620,365);click();ui_message("clicks=0/0 menu=false")
                move(799,599);click("right");time.sleep(.05)
                assert pixel(621,525)!=0x80,"Edge menu was not kept on screen"
                move(50,450);click();time.sleep(.05);assert pixel(621,525)==0x80,"Outside click did not dismiss menu"
                guest.key("esc");guest.wait_for("Returned to text mode.");assert frames()==baseline,"Mouse display buffers leaked"
                guest.command("echo keyboard survived","\nkeyboard survived\n")
                if disk:
                    guest.command("apps","EDIT.BIN")
                    guest.command("run hello","User program exited with code 0.")
            # Fatal display recovery must work in the mouse demo too.
            offset=len((directory/"debug.log").read_text());guest.type("mouse\n");deadline=time.monotonic()+20
            while "Mouse: frame ready" not in (directory/"debug.log").read_text()[offset:]:
                assert time.monotonic()<deadline;time.sleep(.02)
            guest.call("inject-nmi");guest.wait_for("System halted.")
            print(f"PASS PS/2 mouse ({args.vga}): cursor pixels/movement, clean background, both buttons, capture, right-click menu/reset/edge bounds, keyboard, repeated exit/frame balance and fatal recovery")
        finally:
            guest.close();shutil.copyfile(directory/"debug.log",output/f"mouse-{args.vga}-debug.log")
            shutil.copyfile(directory/"qemu.stderr",output/f"mouse-{args.vga}-stderr.log")


if __name__=="__main__":main()
