"""Exercise window dragging/focus/closing and compare VRAM with the composited scene."""
import argparse
from array import array
from pathlib import Path
import re,shutil,tempfile,time
from smoke import Guest,ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel",type=Path,default=ROOT/"target/i686-unknown-none/debug/rust-os")
    parser.add_argument("--disk",type=Path)
    parser.add_argument("--vga",default="std")
    args=parser.parse_args();qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    output=ROOT/"target/smoke";output.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="desktop-",dir=output) as temporary:
        directory=Path(temporary);disk=None
        if args.disk:disk=directory/"boot.img";shutil.copyfile(args.disk,disk)
        guest=Guest(qemu,None if disk else args.kernel,directory,disk=disk,attach_disk=disk is not None,extra_args=["-vga",args.vga])
        try:
            def log():return (directory/"debug.log").read_text()
            def until(test,message):
                deadline=time.monotonic()+15
                while not test():assert time.monotonic()<deadline,message;time.sleep(.02)
            def frames():return int(re.search(r"Free now: +(\d+)",guest.command("frames","Free now:"))[1])
            baseline=frames()
            for cycle in range(2):
                offset=len(log());guest.type("desktop\n");until(lambda:"Desktop: frame ready" in log()[offset:],"Desktop failed to start")
                back=int(re.search(r"Desktop: backbuffer=([0-9a-f]+)",log()[offset:])[1],16)
                if args.vga=="std":
                    dev=next(d for d in guest.call("query-pci")[0]["devices"] if d.get("id",{}).get("vendor")==0x1234)
                    base=next(r["address"] for r in dev["regions"] if r["bar"]==0);pitch=3200;size=4
                else:
                    mode=re.search(r"BIOS VBE mode [0-9a-f]+ base=([0-9a-f]+) pitch=(\d+) bpp=(\d+)",log()[offset:])
                    assert mode;base=int(mode[1],16);pitch=int(mode[2]);size=int(mode[3])//8
                def memory(address,length):
                    path=directory/"memory.bin";guest.call("human-monitor-command",{"command-line":f'pmemsave {address:#x} {length} "{path.as_posix()}"'});return path.read_bytes()
                pos=[400,300]
                def composite():
                    time.sleep(.05);guest.call("stop")
                    try:
                        scene=memory(back,800*600*4);screen=memory(base,pitch*600)
                    finally:guest.call("cont")
                    if size==3:
                        expected=bytearray(800*600*3)
                        for i in range(3):expected[i::3]=scene[i::4]
                    elif size==2:
                        rgb=array('I');rgb.frombytes(scene)
                        expected=array('H',(((c>>19&31)<<11|(c>>10&63)<<5|(c>>3&31)) for c in rgb)).tobytes()
                    else:expected=scene
                    # The only permitted difference is the transparent cursor's footprint.
                    for y in range(600):
                        intervals=[(0,800)] if not pos[1]<=y<pos[1]+24 else [(0,pos[0]),(min(800,pos[0]+16),800)]
                        for left,right in intervals:
                            actual=screen[y*pitch+left*size:y*pitch+right*size];wanted=expected[(y*800+left)*size:(y*800+right)*size]
                            if actual!=wanted:
                                i=next(i for i,(a,b) in enumerate(zip(actual,wanted)) if a!=b)
                                raise AssertionError(f"Damaged composition at ({left+i//size},{y}): screen={actual[i]:02x} expected={wanted[i]:02x}")
                def move(x,y):
                    while pos!=[x,y]:
                        dx=max(-80,min(80,x-pos[0]));dy=max(-80,min(80,y-pos[1]))
                        guest.call("input-send-event",{"events":[{"type":"rel","data":{"axis":"x","value":dx}},{"type":"rel","data":{"axis":"y","value":dy}}]})
                        pos[0]+=dx;pos[1]+=dy;time.sleep(.04)
                    time.sleep(.05)
                def button(which,down):
                    guest.call("input-send-event",{"events":[{"type":"btn","data":{"button":which,"down":down}}]});time.sleep(.07)
                def click(x,y,which="left"):move(x,y);button(which,True);button(which,False)
                def state(*parts):
                    def matches():
                        states=[line for line in log()[offset:].splitlines() if line.startswith("Desktop: windows=")]
                        return states and all(part in states[-1] for part in parts)
                    until(matches,"Expected desktop state: "+str(parts)+"\n"+log()[offset:][-1000:])
                composite()
                click(340,240);state("top=1","clicks=0/0")
                click(150,330);state("top=0");composite()
                click(150,240);click(340,240);state("clicks=1/1");composite()
                move(150,240);button("left",True);move(700,500);button("left",False);state("clicks=1/1");composite()
                move(140,155);button("left",True);move(240,135);button("left",False);state("pos=200,120;300,210");composite()
                move(600,225);button("left",True);move(680,275);button("left",False);state("top=1","pos=200,120;380,260");composite()
                click(230,310);state("top=0");composite()
                guest.call("screendump",{"filename":str(output/f"desktop-{args.vga}.png"),"format":"png"})
                click(542,135);state("windows=false/true","top=1");composite()
                click(702,275);state("windows=false/false","top=9");composite()
                click(700,450,"right");state("menu=true");composite()
                click(630,465);state("windows=true/true","pos=100,140;300,210","menu=false","clicks=1/1");composite()
                move(140,155);button("left",True);move(0,0);button("left",False);state("pos=0,84;300,210");composite()
                click(799,599,"right");state("menu=true");composite()
                click(630,530);state("pos=100,140;300,210","menu=false");composite()
                guest.key("esc");guest.wait_for("Returned to text mode.");assert frames()==baseline,"Desktop leaked frames"
                guest.command("echo desktop survived","\ndesktop survived\n")
                if disk:guest.command("apps","EDIT.BIN");guest.command("run hello","User program exited with code 0.")
            offset=len(log());guest.type("desktop\n");until(lambda:"Desktop: frame ready" in log()[offset:],"Desktop restart failed")
            guest.call("inject-nmi");guest.wait_for("System halted.")
            print(f"PASS desktop ({args.vga}): focus/occlusion, both controls, drag capture/bounds, closing/restoring, full VRAM composition checks, repeated exit/frame balance and fatal recovery")
        finally:
            guest.close();shutil.copyfile(directory/"debug.log",output/f"desktop-{args.vga}-debug.log");shutil.copyfile(directory/"qemu.stderr",output/f"desktop-{args.vga}-stderr.log")


if __name__=="__main__":main()
