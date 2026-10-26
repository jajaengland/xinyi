"""Verify Snake gameplay, food beeps and captured PC-speaker audio."""
import argparse
from pathlib import Path
import re
import shutil
import subprocess
import struct
import sys
import tempfile
import time
import wave

from program_disk import create_program_disk
from smoke import Guest, ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel",type=Path,action="append")
    args=parser.parse_args()
    kernels=args.kernel or [ROOT/f"target/i686-unknown-none/{profile}/rust-os" for profile in ("debug","release")]
    qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    nasm=shutil.which("nasm") or r"C:\msys64\usr\bin\nasm.exe"
    output=ROOT/"target/smoke";output.mkdir(parents=True,exist_ok=True)
    transcript=[]
    try:
        with tempfile.TemporaryDirectory(prefix="snake-",dir=output) as temporary:
            directory=Path(temporary);binary=directory/"SNAKE.BIN";probe=directory/"SOUND.BIN"
            subprocess.run([sys.executable,str(ROOT/"tools/build_program.py"),str(ROOT/"programs/snake/main.rs"),"--output",str(binary)],check=True)
            subprocess.run([nasm,"-f","bin",str(ROOT/"programs/speaker_check.asm"),"-o",str(probe)],check=True)
            image=directory/"data.img"
            create_program_disk(image,{"SNAKE.BIN":binary.read_bytes(),"SOUND.BIN":probe.read_bytes()})
            original=image.read_bytes()
            for kernel in kernels:
                audio=directory/f"{kernel.parent.name}.wav"
                guest=Guest(qemu,kernel,directory,disk=image,memory_mib=8,
                    extra_args=["-audiodev",f"wav,id=speaker,path={audio.as_posix()}","-machine","pcspk-audiodev=speaker"])
                try:
                    def command(text,expected):
                        screen=guest.command(text,expected);deadline=time.monotonic()+5
                        while not guest.prompt_ready(screen):
                            assert time.monotonic()<deadline,screen
                            time.sleep(.02);screen=guest.screen()
                        transcript.append(f"[{kernel.parent.name}: {text}]\n{screen}")
                        return screen
                    def frames():
                        return int(re.search(r"Free now: +(\d+)",command("frames","Free now:"))[1])
                    def board():return tuple(line[16:64] for line in guest.screen().splitlines()[2:20])
                    baseline=frames();command("mount","Mounted FAT16")
                    command("run sound.bin","User program exited with code 0.")
                    guest.command("run snake.bin","X I N Y I   S N A K E")
                    guest.wait_for("DOTS: 001",timeout=5)
                    guest.key("p");guest.wait_for("PAUSED")
                    first=board()
                    assert sum(row.count("[]") for row in first)==3,first
                    assert sum(row.count("@@") for row in first)==1,first
                    assert sum(row.count("()") for row in first)==1,first
                    time.sleep(.5);assert board()==first,"Paused snake moved"
                    log=(directory/"debug.log").read_text()
                    assert log.count("Speaker: 880 Hz for 70 ms")==1,log
                    guest.call("screendump",{"filename":str(output/"snake-preview.png"),"format":"png"})
                    halted=sum("HLT=1" in guest.call("human-monitor-command",{"command-line":"info registers"}) for _ in range(20))
                    assert halted>=10,f"Snake busy-waits: {halted}/20"
                    guest.key("p");guest.key("up")
                    guest.wait_for("GAME OVER",timeout=6)
                    guest.key("r");guest.wait_for("DOTS: 000")
                    guest.key("p");guest.wait_for("PAUSED")
                    guest.key("esc");guest.wait_for("User program exited with code 0.")
                    assert frames()==baseline,"Snake leaked frames"
                    command("echo snake finished","\nsnake finished\n")
                    transcript.append((directory/"debug.log").read_text())
                    guest.call("quit");guest.process.wait(timeout=5)
                finally:guest.close()
                # This QEMU Windows build leaves placeholder RIFF/data sizes
                # on exit. Finalize those fields in our captured test artifact.
                captured=bytearray(audio.read_bytes())
                assert captured[:4]==b"RIFF" and captured[8:16]==b"WAVEfmt " and captured[36:40]==b"data"
                if captured[4:8]==bytes(4) and captured[40:44]==bytes(4):
                    struct.pack_into("<I",captured,4,len(captured)-8)
                    struct.pack_into("<I",captured,40,len(captured)-44)
                    audio.write_bytes(captured)
                shutil.copyfile(audio,output/f"snake-{kernel.parent.name}.wav")
                with wave.open(str(audio),"rb") as stream:
                    assert stream.getsampwidth()==2
                    samples=stream.readframes(stream.getnframes())
                    values=struct.unpack("<"+"h"*(len(samples)//2),samples)
                    assert min(values)<-1000 and max(values)>1000,"The food beep produced no oscillating audio"
                assert image.read_bytes()==original,"Playing Snake modified files"
                print(f"PASS {kernel.parent.name}: eating/growth, one beep per dot, recorded audio, pause/halt, wall collision, restart, quit/frame cleanup and unchanged disk",flush=True)
    finally:(output/"snake-transcript.txt").write_text("\n\n".join(transcript))


if __name__=="__main__":main()
