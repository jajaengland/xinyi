"""Verify graphics pixels, movement, font/text recovery and frame reclamation."""
import argparse
from pathlib import Path
import re
import shutil
import tempfile
import time
from smoke import Guest,ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel",type=Path,action="append")
    parser.add_argument("--disk",type=Path,help="Exercise graphics after native BIOS disk boot")
    args=parser.parse_args();kernels=args.kernel or [ROOT/f"target/i686-unknown-none/{p}/rust-os" for p in ("debug","release")]
    qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    output=ROOT/"target/smoke";output.mkdir(parents=True,exist_ok=True);transcript=[]
    try:
        with tempfile.TemporaryDirectory(prefix="graphics-",dir=output) as temporary:
            directory=Path(temporary)
            for kernel in kernels:
                guest=Guest(qemu,None if args.disk else kernel,directory,disk=args.disk,
                    attach_disk=args.disk is not None,memory_mib=8,extra_args=["-vga","std"])
                try:
                    def frames():return int(re.search(r"Free now: +(\d+)",guest.command("frames","Free now:"))[1])
                    devices=guest.call("query-pci")[0]["devices"]
                    device=next(d for d in devices if d.get("id",{}).get("vendor")==0x1234 and d["id"]["device"]==0x1111)
                    base=next(r["address"] for r in device["regions"] if r["bar"]==0)
                    def memory(address,length):
                        path=directory/"memory.bin"
                        guest.call("human-monitor-command",{"command-line":f'pmemsave {address:#x} {length} "{path.as_posix()}"'})
                        return path.read_bytes()
                    def font():
                        data=memory(base,256*1024)
                        return bytes(data[(glyph*32+row)*4+2] for glyph in range(256) for row in range(16))
                    baseline=frames();original_font=font()
                    for cycle in range(2):
                        offset=len((directory/"debug.log").read_text())
                        guest.type("graphics\n");deadline=time.monotonic()+10
                        while "Graphics: frame ready" not in (directory/"debug.log").read_text()[offset:]:
                            assert time.monotonic()<deadline,guest.screen();time.sleep(.02)
                        # Rendering can finish after the mode-change log line.
                        deadline=time.monotonic()+10
                        while memory(base+((340*800+350)*4),4)!=bytes.fromhex("d0cc5800"):
                            assert time.monotonic()<deadline,"Graphics frame did not finish drawing";time.sleep(.02)
                        guest.call("screendump",{"filename":str(output/f"graphics-{kernel.parent.name}.png"),"format":"png"})
                        png=(output/f"graphics-{kernel.parent.name}.png").read_bytes()
                        assert int.from_bytes(png[16:20],"big")==800 and int.from_bytes(png[20:24],"big")==600
                        assert memory(base,4)==bytes.fromhex("706c1700")
                        assert memory(base+((360*800+235)*4),4)==bytes.fromhex("4fa3e500"),"Filled triangle missing"
                        assert memory(base+((295*800+570)*4),4)==bytes.fromhex("86e18f00"),"Triangle outline missing"
                        assert memory(base+((370*800+570)*4),4)==bytes.fromhex("ac655800"),"Second triangle missing"
                        guest.key("right");deadline=time.monotonic()+10
                        while memory(base+((340*800+350)*4),4)!=bytes.fromhex("33282000"):
                            assert time.monotonic()<deadline,"Arrow key did not move the box";time.sleep(.02)
                        if cycle==0:
                            def box_x():
                                deadline=time.monotonic()+5
                                while True:
                                    row=memory(base+340*800*4,800*4)
                                    columns=[x for x in range(800) if row[x*4:x*4+4]==bytes.fromhex("d0cc5800")]
                                    if len(columns)==48 and columns[-1]-columns[0]==47:return columns[0]
                                    assert time.monotonic()<deadline,"Box drawing did not finish"
                                    time.sleep(.002)
                            def key_state(key,down):
                                guest.call("input-send-event",{"events":[{"type":"key","data":{"down":down,"key":{"type":"qcode","data":key}}}]})
                            first_x=box_x();key_state("right",True);time.sleep(.25);key_state("right",False)
                            time.sleep(.04);stopped=box_x()
                            assert 32<=stopped-first_x<=200,"Held movement is delayed or not timer-paced"
                            time.sleep(.08);assert box_x()==stopped,"Movement continued after key release"
                            while box_x()>230:guest.key("left")
                            deadline=time.monotonic()+5
                            while memory(base+((360*800+235)*4),4)!=bytes.fromhex("d0cc5800"):
                                assert time.monotonic()<deadline,"Box did not cover triangle";time.sleep(.002)
                            while box_x()<310:guest.key("right")
                            deadline=time.monotonic()+5
                            while memory(base+((360*800+235)*4),4)!=bytes.fromhex("4fa3e500"):
                                assert time.monotonic()<deadline,"Moving box damaged the cached triangle";time.sleep(.002)
                        guest.key("esc");screen=guest.wait_for("Returned to text mode.",timeout=10)
                        assert font()==original_font,"Font planes were damaged by mode switching"
                        assert frames()==baseline,"Graphics buffers leaked frames"
                        transcript.append(screen)
                    transcript.append(guest.command("echo graphics survived","\ngraphics survived\n"))
                    # A fatal CPU exception must restore readable diagnostics too.
                    offset=len((directory/"debug.log").read_text());guest.type("graphics\n");deadline=time.monotonic()+10
                    while "Graphics: frame ready" not in (directory/"debug.log").read_text()[offset:]:
                        assert time.monotonic()<deadline;time.sleep(.02)
                    while memory(base+((340*800+350)*4),4)!=bytes.fromhex("d0cc5800"):
                        assert time.monotonic()<deadline;time.sleep(.02)
                    guest.call("inject-nmi")
                    transcript.append(guest.wait_for("System halted.",timeout=10))
                    assert font()==original_font,"Fatal diagnostics lost the text font"
                    print(f"PASS {kernel.parent.name}: 800x600 pixels/font, keyboard movement, repeated text/font restoration and frame balance",flush=True)
                finally:guest.close()
            guest=Guest(qemu,kernels[0],directory,attach_disk=False,extra_args=["-vga","cirrus"])
            try:
                transcript.append(guest.command("graphics","Graphics backend unavailable"))
                guest.command("echo usable","\nusable\n")
            finally:guest.close()
            print("PASS unsupported VGA: graphics refused, text shell remains usable",flush=True)
            guest=Guest(qemu,kernels[0],directory,attach_disk=False,memory_mib=2,extra_args=["-vga","std"])
            try:
                baseline=int(re.search(r"Free now: +(\d+)",guest.command("frames","Free now:"))[1])
                transcript.append(guest.command("graphics","Not enough memory"))
                after=int(re.search(r"Free now: +(\d+)",guest.command("frames","Free now:"))[1])
                assert baseline==after,"Failed graphics allocation leaked frames"
                guest.command("echo usable","\nusable\n")
            finally:guest.close()
            print("PASS low memory: no mode switch or leaked frames; shell remains usable",flush=True)
    finally:(output/("graphics-native-transcript.txt" if args.disk else "graphics-transcript.txt")).write_text("\n\n".join(transcript))


if __name__=="__main__":main()
