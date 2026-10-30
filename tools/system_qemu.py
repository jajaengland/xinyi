"""Verify filesystem applications, a persistent editor and numbered disks."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import time

from folders_qemu import inspect_tree
from install_qemu import verify_payload
from seed_apps import seed_apps
from smoke import Guest,ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel",type=Path,required=True)
    parser.add_argument("--package",type=Path,required=True)
    parser.add_argument("--qemu",default=shutil.which("qemu-system-i386"))
    parser.add_argument("--output",type=Path)
    args=parser.parse_args();kernel=args.kernel.resolve();package=args.package.resolve()
    if args.output and args.output.exists():parser.error("Output exists")
    qemu=args.qemu or r"C:\Program Files\qemu\qemu-system-i386.exe"
    output=ROOT/"target/smoke";output.mkdir(parents=True,exist_ok=True);transcript=[]
    try:
        with tempfile.TemporaryDirectory(prefix="system-",dir=output) as temporary:
            directory=Path(temporary)
            def command(guest,text,expected):
                screen=guest.command(text,expected);deadline=time.monotonic()+8
                while not guest.prompt_ready(screen):
                    assert time.monotonic()<deadline,screen
                    time.sleep(.02);screen=guest.screen()
                transcript.append(f"[{text}]\n{screen}");return screen
            def quit_editor(guest):
                guest.key("ctrl-q");guest.wait_for("User program exited with code 0.")
            guest=Guest(qemu,kernel,directory,attach_disk=False)
            try:command(guest,"run snake","Program: file not found")
            finally:guest.close()
            image=directory/"source.img"
            with image.open("xb") as stream:stream.truncate(16*1024*1024)
            guest=Guest(qemu,kernel,directory,disk=image)
            try:
                command(guest,"install","To install, enter: install confirm")
                guest.type("install confirm\n");guest.wait_for("Installation complete:",timeout=30)
            finally:guest.close()
            verify_payload(image,kernel);expected=seed_apps(image,package);clean=image.read_bytes()
            verify_payload(image,kernel)
            check=directory/"check-package"; (check/"apps").mkdir(parents=True)
            subprocess.run([sys.executable,str(ROOT/"tools/build_program.py"),str(ROOT/"programs/file_check/main.rs"),"--output",str(check/"apps/apitest.bin")],check=True)
            (check/"applications.json").write_text(json.dumps([{"name":"apitest","file":"APITEST.BIN"}]))
            seed_apps(image,check)
            guest=Guest(qemu,None,directory,disk=image,memory_mib=32)
            try:
                # Native disk boot auto-mounts before the prompt. No mount command.
                screen=command(guest,"apps","EDIT.BIN")
                for name in expected:
                    if not name.endswith(('.BIN','.ELF')):continue
                    assert name.split('/')[-1] in screen,screen
                command(guest,"run hello","User program exited with code 0.")
                command(guest,"run apitest probe","File API checks passed!")
                command(guest,"delete /apps/apitest.bin","Deleted:")
                command(guest,"delete api.txt","Deleted:")
                for name,title in [("snake","X I N Y I   S N A K E"),("tetris","X I N Y I")]:
                    guest.command("run "+name,title);guest.key("esc");guest.wait_for("User program exited with code 0.")
                guest.command("edit note.txt","Xinyi Editor")
                guest.type("hello world\nsecond line")
                guest.key("ctrl-s");guest.wait_for("Saved to disk")
                quit_editor(guest)
                command(guest,"cat note.txt","hello world\nsecond line")
                command(guest,"touch guard.txt","Created:")
                command(guest,"write guard.txt keep this","Wrote")
                guest.command("edit note.txt","Xinyi Editor")
                guest.key("end");guest.type("!")
                guest.key("ctrl-z");guest.key("ctrl-y")
                guest.key("ctrl-q");guest.wait_for("Unsaved changes")
                guest.key("c");guest.wait_for("Cancelled")
                guest.key("ctrl-s");guest.wait_for("Saved to disk")
                guest.key("ctrl-shift-s");guest.wait_for("Save as:")
                guest.type("copy.txt\n");guest.wait_for("Saved to disk")
                guest.key("ctrl-shift-s");guest.wait_for("Save as:")
                guest.type("guard.txt\n");guest.wait_for("Replace existing file?")
                guest.key("n");guest.wait_for("Save cancelled")
                guest.call("screendump",{"filename":str(output/"editor-preview.png"),"format":"png"})
                quit_editor(guest)
                command(guest,"cat copy.txt","hello world!\nsecond line")
                command(guest,"cat guard.txt","keep this")
                guest.command("edit copy.txt","Xinyi Editor");guest.type("discard me")
                guest.key("ctrl-q");guest.wait_for("Unsaved changes");guest.key("d");guest.wait_for("User program exited with code 0.")
                command(guest,"cat copy.txt","hello world!\nsecond line")
                guest.command("edit","Xinyi Editor");guest.type("save on quit")
                guest.key("ctrl-q");guest.wait_for("Unsaved changes");guest.key("s");guest.wait_for("Save as:")
                guest.type("unnamed.txt\n");guest.wait_for("User program exited with code 0.")
                command(guest,"cat unnamed.txt","save on quit")
                # Removing the executable really removes the app; no kernel fallback.
                command(guest,"delete /apps/snake/snake.bin","Deleted:")
                command(guest,"run snake","Program: file not found")
            finally:guest.close()
            files,_=inspect_tree(image,2048)
            assert files["NOTE.TXT"]==b"hello world!\nsecond line"
            assert files["COPY.TXT"]==files["NOTE.TXT"]
            seed_apps(image,package) # Restore the removed app offline for transfer.
            data=bytearray(image.read_bytes());boot=2048*512
            root=2048+struct.unpack_from("<H",data,boot+14)[0]+data[boot+16]*struct.unpack_from("<H",data,boot+22)[0]
            entries=struct.unpack_from("<H",data,boot+17)[0]
            locked=next(root*512+i*32 for i in range(entries) if data[root*512+i*32:root*512+i*32+11]==b"GUARD   TXT")
            data[locked+11]|=1;image.write_bytes(data)
            # Cold boot proves editor saves are persistent, not RAM-only.
            destination=directory/"destination.img"
            with destination.open("xb") as stream:stream.truncate(16*1024*1024)
            blank=destination.read_bytes()
            guest=Guest(qemu,None,directory,disk=image,extra_args=["-drive",f"file={destination.as_posix()},format=raw,if=ide,index=1"])
            try:
                command(guest,"cat note.txt","hello world!\nsecond line")
                guest.command("edit guard.txt","Xinyi Editor");guest.type("blocked")
                guest.key("ctrl-s");guest.wait_for("Save failed: file is read-only")
                guest.key("ctrl-q");guest.wait_for("Unsaved changes");guest.key("d");guest.wait_for("User program exited with code 0.")
                command(guest,"cat guard.txt","keep this")
                command(guest,"disks","2:")
                command(guest,"install 99","Install refused: file not found")
                command(guest,"cat note.txt","hello world!")
                command(guest,"use 2","Drive: no supported FAT16 volume")
                command(guest,"cat note.txt","hello world!") # Failed switch kept source mounted.
                baseline=command(guest,"frames","Free now:")
                command(guest,"install 2","To install, enter: install confirm")
                command(guest,"echo cancel","\ncancel\n")
                after=command(guest,"frames","Free now:")
                import re
                assert re.search(r"Free now: +(\d+)",baseline)[1]==re.search(r"Free now: +(\d+)",after)[1],"Cancelled install leaked app staging"
                assert destination.read_bytes()==blank,"Cancelled install wrote to disk"
                command(guest,"use 1","Drive 1 ready")
                command(guest,"install 2","To install, enter: install confirm")
                guest.type("install confirm\n");guest.wait_for("Installed drive ready",timeout=45)
                command(guest,"apps","EDIT.BIN")
            finally:guest.close()
            assert destination.read_bytes()!=blank
            verify_payload(destination,kernel)
            files,_=inspect_tree(destination,2048)
            assert files==expected,(set(files),set(expected))
            guest=Guest(qemu,None,directory,disk=destination,disk_index=1)
            try:
                command(guest,"apps","EDIT.BIN")
                guest.command("edit fresh.txt","Xinyi Editor");guest.type("installed editor")
                guest.key("ctrl-s");guest.wait_for("Saved to disk");quit_editor(guest)
                command(guest,"cat fresh.txt","installed editor")
            finally:guest.close()
            if args.output:
                args.output.parent.mkdir(parents=True,exist_ok=True)
                with args.output.open("xb") as stream:stream.write(clean)
            print(f"PASS {kernel.parent.name}: real /APPS files, no embedded fallback, automatic mounting, editor save/undo/save-as/cold persistence, numbered drive refusal and app-copy installation/native boot",flush=True)
    finally:(output/f"system-{kernel.parent.name}-transcript.txt").write_text("\n\n".join(transcript))


if __name__=="__main__":main()
