"""Measure window-drag latency and cursor visibility under burst mouse input."""
import argparse,json,re,shutil,statistics,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disk",type=Path,required=True)
    parser.add_argument("--vga",default="cirrus")
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--check",action="store_true",help="Require a visible cursor throughout sampled drag bursts")
    args=parser.parse_args();qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    parent=ROOT/"target/smoke";parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="drag-",dir=parent) as temporary:
        directory=Path(temporary);image=directory/"disk.img";shutil.copyfile(args.disk,image)
        guest=Guest(qemu,None,directory,disk=image,extra_args=["-vga",args.vga])
        try:
            guest.type("notepad drag.txt\n");deadline=time.monotonic()+20
            while "nr=61" not in (directory/"debug.log").read_text():
                assert time.monotonic()<deadline;time.sleep(.01)
            log=(directory/"debug.log").read_text()
            if args.vga=="std":
                dev=next(d for d in guest.call("query-pci")[0]["devices"] if d.get("id",{}).get("vendor")==0x1234)
                base=next(r["address"] for r in dev["regions"] if r["bar"]==0);pitch=3200;size=4
            else:
                mode=re.search(r"BIOS VBE mode [0-9a-f]+ base=([0-9a-f]+) pitch=(\d+) bpp=(\d+)",log)
                base=int(mode[1],16);pitch=int(mode[2]);size=int(mode[3])//8
            def memory(address,length):
                path=directory/"memory.bin";guest.call("human-monitor-command",{"command-line":f'pmemsave {address:#x} {length} "{path.as_posix()}"'});return path.read_bytes()
            def motion(dx,dy):guest.call("input-send-event",{"events":[{"type":"rel","data":{"axis":"x","value":dx}},{"type":"rel","data":{"axis":"y","value":dy}}]})
            def button(down):guest.call("input-send-event",{"events":[{"type":"btn","data":{"button":"left","down":down}}]})
            for dx,dy in [(-80,-80),(-80,-80),(-80,-37),(-60,0)]:motion(dx,dy);time.sleep(.05)
            button(True);time.sleep(.1)
            def rgb(data,x):return int.from_bytes(data[x*size:(x+1)*size],"little")
            samples=[];absent=0;visible_samples=0;x=72;cursor=100
            for cycle in range(8):
                delta=1 if cycle%2==0 else -1;x+=20*delta;cursor+=20*delta;started=time.monotonic()
                for _ in range(20):motion(delta,0);time.sleep(.001)
                deadline=time.monotonic()+10
                while True:
                    band=memory(base+103*pitch,pitch*3)
                    pointer=any(rgb(band,c)==0 and rgb(band[2*pitch:],c+1)==0xffffff for c in range(80,150))
                    absent+=not pointer;visible_samples+=1
                    bottom=memory(base+531*pitch,800*size)
                    columns=[c for c in range(800) if rgb(bottom,c)==0x40]
                    if columns==list(range(x+6,x+662)) and rgb(band,cursor)==0 and rgb(band[2*pitch:],cursor+1)==0xffffff:break
                    assert time.monotonic()<deadline,"Window/cursor failed to catch up";time.sleep(.002)
                samples.append((time.monotonic()-started)*1000)
            button(False);time.sleep(.1);guest.key("ctrl-q");guest.wait_for("User program exited with code 0.")
        finally:guest.close()
    report={"vga":args.vga,"drag_latency_ms":samples,"median_ms":statistics.median(samples),"cursor_absent_samples":absent,"samples":visible_samples}
    args.output.write_text(json.dumps(report,indent=2));print(json.dumps(report))
    if args.check:assert absent==0,"Cursor disappeared during sampled window transfers"


if __name__=="__main__":main()
