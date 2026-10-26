"""Build and exercise real protected Tetris programs on private FAT16 disks."""
import argparse
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

from program_disk import create_program_disk
from smoke import Guest, ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel",type=Path,action="append")
    args=parser.parse_args()
    kernels=args.kernel or [ROOT / f"target/i686-unknown-none/{profile}/rust-os" for profile in ("debug","release")]
    qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    output=ROOT/"target/smoke"
    output.mkdir(parents=True,exist_ok=True)
    transcript=[]
    try:
        with tempfile.TemporaryDirectory(prefix="tetris-",dir=output) as temporary:
            directory=Path(temporary)
            binary=directory/"TETRIS.BIN"
            subprocess.run([sys.executable,str(ROOT/"tools/build_program.py"),str(ROOT/"programs/tetris/main.rs"),"--output",str(binary)],check=True)
            assert 1024 < binary.stat().st_size <= 65536
            image=directory/"data.img"
            max_program=bytes.fromhex("b801000000bb2a000000cd80").ljust(65536,b"\0")
            create_program_disk(image,{"TETRIS.BIN":binary.read_bytes(),"MAX.BIN":max_program})
            original=image.read_bytes()
            for kernel in kernels:
                guest=Guest(qemu,kernel,directory,disk=image,memory_mib=8)
                try:
                    def command(text,expected):
                        screen=guest.command(text,expected)
                        deadline=time.monotonic()+5
                        while not guest.prompt_ready(screen):
                            assert time.monotonic()<deadline,screen
                            time.sleep(.02);screen=guest.screen()
                        transcript.append(f"[{kernel.parent.name}: {text}]\n{screen}")
                        return screen
                    def frames():
                        return int(re.search(r"Free now: +(\d+)",command("frames","Free now:"))[1])
                    def board():
                        screen=guest.screen()
                        return tuple(line[29:49] for line in screen.splitlines()[2:22])
                    def blocks(frame):
                        return sum(row.count("[") for row in frame)
                    def wait_board(predicate):
                        deadline=time.monotonic()+5
                        while True:
                            result=board()
                            if predicate(result):return result
                            assert time.monotonic()<deadline,"Unexpected board: "+repr(result)
                            time.sleep(.02)
                    baseline=frames()
                    command("mount","Mounted FAT16")
                    command("run max.bin","User program exited with code 42.")
                    assert frames()==baseline,"64 KiB program leaked frames"
                    guest.command("run tetris.bin","X I N Y I")
                    guest.key("p");guest.wait_for("PAUSED")
                    first=wait_board(lambda frame:blocks(frame)==4)
                    guest.call("screendump", {"filename":str(output / "tetris-preview.png"), "format":"png"})
                    time.sleep(.8)
                    assert board()==first,"Paused game continued falling"
                    halted=sum("HLT=1" in guest.call("human-monitor-command",{"command-line":"info registers"}) for _ in range(20))
                    assert halted>=10,f"Game busy-spins while waiting: {halted}/20"
                    guest.key("p");guest.key("left");guest.key("up");guest.key("p")
                    guest.wait_for("PAUSED")
                    moved=wait_board(lambda frame:frame!=first)
                    assert blocks(moved)==4
                    guest.key("p")
                    guest.key("spc")
                    guest.key("p");guest.wait_for("PAUSED")
                    wait_board(lambda frame:blocks(frame)==8)
                    # Resume and verify gravity independently of key movement.
                    guest.key("p")
                    falling=board()
                    wait_board(lambda frame:frame!=falling)
                    guest.key("r")
                    wait_board(lambda frame:blocks(frame)==4)
                    guest.key("esc")
                    guest.wait_for("User program exited with code 0.")
                    assert frames()==baseline,"Tetris leaked task or staging frames"
                    command("echo game finished","\ngame finished\n")
                    transcript.append((directory/"debug.log").read_text())
                    print(f"PASS {kernel.parent.name}: 64 KiB loading, Tetris rendering, pause/halt, movement/rotation, hard drop, gravity, restart and exit/frame reclamation",flush=True)
                finally:guest.close()
                assert image.read_bytes()==original,"Playing changed the disk"
    finally:(output/"tetris-transcript.txt").write_text("\n\n".join(transcript))


if __name__=="__main__":main()
