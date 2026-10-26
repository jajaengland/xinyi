"""QEMU-only idle/wakeup and large-disk capacity regression checks."""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

from smoke import Guest,ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu",default=shutil.which("qemu-system-i386"))
    parser.add_argument("--qemu-img",default=shutil.which("qemu-img"))
    parser.add_argument("--kernel",type=Path,default=ROOT / "target/i686-unknown-none/debug/rust-os")
    args = parser.parse_args()
    if not args.qemu or not args.qemu_img or not args.kernel.is_file():
        parser.error("Build the kernel and put QEMU/qemu-img on PATH")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True,exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="reliability-",dir=output) as temporary:
            directory = Path(temporary)
            guest = Guest(args.qemu,args.kernel.resolve(),directory)
            try:
                halted = 0
                for _ in range(30):
                    registers = guest.call("human-monitor-command",{"command-line":"info registers"})
                    halted += bool(re.search(r"HLT=1\b",registers))
                    time.sleep(0.01)
                assert halted >= 15, f"CPU was not halted at the idle prompt: {halted}/30"
                transcript.append(f"Idle samples: {halted}/30 halted\n{registers}")
                for i in range(12):
                    screen = guest.command(f"echo wake{i}",f"\nwake{i}\n")
                transcript.append(screen)
                first = guest.command("uptime","Uptime:")
                time.sleep(1.1)
                second = guest.command("uptime","Uptime:")
                assert re.search(r"Uptime: (\S+)",first)[1] != re.search(r"Uptime: (\S+)",second)[1]
                transcript.append(second)
                print(f"PASS idle: {halted}/30 halted samples; repeated keyboard wakeups and PIT progress",flush=True)
            finally:
                guest.close()
            image = directory / "large.qcow2"
            run = lambda *command: subprocess.check_output(command,text=True,creationflags=getattr(subprocess,"CREATE_NO_WINDOW",0))
            run(args.qemu_img,"create","-f","qcow2",str(image),"3T")
            before = json.loads(run(args.qemu_img,"map","--output=json",str(image)))
            assert not any(extent.get("data") for extent in before)
            guest = Guest(args.qemu,args.kernel.resolve(),directory,disk=image,disk_format="qcow2")
            try:
                screen = guest.command("disk","Full device: 3145728 MiB")
                assert "MiB accessible" in screen and "Exceeds block-device address range" in screen
                transcript.append(screen)
                for command,expected in [
                    ("format","Format refused: whole-disk format refused"),
                    ("format confirm","Format failed: run format first"),
                    ("install","Install refused: whole-disk format refused"),
                    ("install confirm","Install failed: run install first"),
                    ("echo usable","\nusable\n"),
                ]:
                    transcript.append(guest.command(command,expected))
            finally:
                guest.close()
            after = json.loads(run(args.qemu_img,"map","--output=json",str(image)))
            assert not any(extent.get("data") for extent in after), "Refusal wrote guest data clusters"
            print("PASS 3 TiB virtual disk: full/access capacity distinguished; format/fresh install refused with no allocated data",flush=True)
    finally:
        (output / "reliability-transcript.txt").write_text("\n".join(transcript),encoding="utf-8")
    print(f"Transcript: {output / 'reliability-transcript.txt'}")


if __name__ == "__main__":
    main()
