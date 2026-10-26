"""Measure key-to-pixel latency in a private QEMU graphics session."""
import argparse
import json
from pathlib import Path
import shutil
import statistics
import tempfile
import time
from smoke import Guest,ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel",type=Path,default=ROOT/"target/i686-unknown-none/debug/rust-os")
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args();qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    parent=ROOT/"target/smoke";parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="graphics-bench-",dir=parent) as temporary:
        directory=Path(temporary);guest=Guest(qemu,args.kernel,directory,attach_disk=False,extra_args=["-vga","std"])
        try:
            device=next(d for d in guest.call("query-pci")[0]["devices"] if d.get("id",{}).get("vendor")==0x1234)
            base=next(r["address"] for r in device["regions"] if r["bar"]==0)
            def pixel(x,y):
                path=directory/"pixel.bin"
                guest.call("human-monitor-command",{"command-line":f'pmemsave {base+(y*800+x)*4:#x} 4 "{path.as_posix()}"'})
                return path.read_bytes()
            guest.type("graphics\n");deadline=time.monotonic()+15
            while "Graphics: frame ready" not in (directory/"debug.log").read_text():
                assert time.monotonic()<deadline;time.sleep(.005)
            latency=[]
            for index in range(8):
                x=350+8*index;started=time.monotonic()
                guest.call("human-monitor-command",{"command-line":"sendkey right 1"})
                deadline=time.monotonic()+10
                while pixel(x,340)!=bytes.fromhex("33282000") or pixel(x+55,340)!=bytes.fromhex("d0cc5800"):
                    assert time.monotonic()<deadline;time.sleep(.001)
                latency.append((time.monotonic()-started)*1000)
            guest.key("esc");guest.wait_for("Returned to text mode.")
        finally:guest.close()
    report={"kernel":str(args.kernel),"key_to_pixel_ms":latency,"median_ms":statistics.median(latency)}
    args.output.write_text(json.dumps(report,indent=2));print(json.dumps(report))


if __name__=="__main__":main()
