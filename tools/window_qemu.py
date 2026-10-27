"""Verify ring-3 window ABI, graphical Notepad, input, saves and recovery."""
import argparse,json,re,shutil,struct,subprocess,sys,tempfile,time
from array import array
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel",type=Path,default=ROOT/"target/i686-unknown-none/debug/rust-os")
    parser.add_argument("--package",type=Path,default=ROOT/"target/window-package")
    parser.add_argument("--disk",type=Path)
    parser.add_argument("--vga",default="std")
    parser.add_argument("--usb",action="store_true")
    args=parser.parse_args();qemu=shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    output=ROOT/"target/smoke";output.mkdir(parents=True,exist_ok=True)
    subprocess.run(["rustc","--edition=2021",str(ROOT/"tools/font_dump.rs"),"-o",str(ROOT/"target/font-dump.exe")],check=True)
    font=subprocess.check_output([str(ROOT/"target/font-dump.exe")])
    glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
    with tempfile.TemporaryDirectory(prefix="window-",dir=output) as temporary:
        directory=Path(temporary);image=directory/"system.img"
        if args.disk:shutil.copyfile(args.disk,image)
        else:
            with image.open("xb") as stream:stream.truncate(16*1024*1024)
            guest=Guest(qemu,args.kernel,directory,disk=image)
            try:
                guest.command("install","To install, enter: install confirm");guest.type("install confirm\n");guest.wait_for("Installation complete:",timeout=45)
            finally:guest.close()
            seed_apps(image,args.package)
        check=directory/"checks";(check/"apps").mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/"tools/build_program.py"),str(ROOT/"programs/window_check/main.rs"),"--output",str(check/"apps/wcheck.bin")],check=True)
        (check/"applications.json").write_text(json.dumps([{"name":"wcheck","file":"WCHECK.BIN"}]))
        seed_apps(image,check)
        extra=["-vga",args.vga]
        if args.usb:extra += ["-drive",f"if=none,id=stick,file={image.as_posix()},format=raw","-device","piix3-usb-uhci,id=uhci","-device","usb-storage,bus=uhci.0,drive=stick,bootindex=1"]
        guest=Guest(qemu,None,directory,disk=image,attach_disk=not args.usb,extra_args=extra)
        try:
            def log():return (directory/"debug.log").read_text()
            def until(test,message):
                deadline=time.monotonic()+35
                while not test():assert time.monotonic()<deadline,message+"\n"+log()[-1400:];time.sleep(.04)
            def frames():return int(re.search(r"Free now: +(\d+)",guest.command("frames","Free now:"))[1])
            def memory(address,length):
                path=directory/"memory.bin";guest.call("human-monitor-command",{"command-line":f'pmemsave {address:#x} {length} "{path.as_posix()}"'});return path.read_bytes()
            def opened(command):
                offset=len(log());guest.type(command+"\n");until(lambda:"Window: opened" in log()[offset:],"Window did not open: "+command)
                return int(re.search(r"Window: opened handle=\d+ client=640x400 surface=([0-9a-f]+)",log()[offset:])[1],16)
            def text(surface,y):
                data=array('I');data.frombytes(memory(surface+y*640*4,640*16*4))
                return ''.join(glyphs.get(bytes(data[row*640+col*8+x]==0 for row in range(16) for x in range(8)),'?') for col in range(80))
            def status(surface,expected):until(lambda:expected in text(surface,384),"Notepad status: "+expected)
            def contents(name,expected):
                def complete():
                    try:return inspect_tree(image,2048)[0].get(name)==expected
                    except AssertionError:return False # Sampling during mirrored-FAT writes.
                until(complete,"Persisted contents: "+name)
            baseline=frames()
            surface=opened("run wcheck")
            until(lambda:memory(surface+(40*640+8)*4,4)==bytes.fromhex("0000ff00"),"Window probe did not render")
            assert memory(surface,4)==bytes.fromhex("ffffff00"),"Invalid batch partially changed the surface"
            guest.wait_for("Window API checks passed!",timeout=20);assert frames()==baseline,"Window probe leaked frames"
            guest.command("run wcheck fault","User program faulted: vector=6");assert frames()==baseline,"Faulted window leaked frames"
            surface=opened("notepad note.txt");status(surface,"New file")
            guest.type("hello from notepad\nsecond line");guest.key("ctrl-s");contents("NOTE.TXT",b"hello from notepad\nsecond line");status(surface,"Saved to disk")
            pos=[400,300]
            def move(x,y):
                while pos!=[x,y]:
                    dx=max(-80,min(80,x-pos[0]));dy=max(-80,min(80,y-pos[1]));guest.call("input-send-event",{"events":[{"type":"rel","data":{"axis":"x","value":dx}},{"type":"rel","data":{"axis":"y","value":dy}}]});pos[0]+=dx;pos[1]+=dy;time.sleep(.04)
            def button(down):guest.call("input-send-event",{"events":[{"type":"btn","data":{"button":"left","down":down}}]});time.sleep(.08)
            def click(x,y):move(x,y);button(True);button(False)
            click(120,162);guest.type("!");guest.key("ctrl-s");contents("NOTE.TXT",b"hello! from notepad\nsecond line")
            move(110,103);button(True);move(150,123);button(False)
            guest.key("shift-slash");click(164,158);contents("NOTE.TXT",b"hello!? from notepad\nsecond line")
            guest.call("screendump",{"filename":str(output/f"notepad-{args.vga}.png"),"format":"png"})
            click(750,123);guest.wait_for("User program exited with code 0.");assert frames()==baseline
            surface=opened("run notepad note.txt");status(surface,"Opened")
            guest.type("pending");guest.key("ctrl-q");status(surface,"Unsaved changes");guest.key("c");status(surface,"Cancelled")
            # A graphical close request must use the same unsaved-change dialog.
            pos[:]=[400,300];click(710,103);status(surface,"Unsaved changes");guest.key("d");guest.wait_for("User program exited with code 0.")
            assert inspect_tree(image,2048)[0]["NOTE.TXT"]==b"hello!? from notepad\nsecond line"
            surface=opened("notepad");guest.type("save as works");guest.key("ctrl-s");status(surface,"Save as")
            guest.type("copy.txt\n");contents("COPY.TXT",b"save as works");guest.key("ctrl-q");guest.wait_for("User program exited with code 0.")
            guest.command("touch guard.txt","Created:");guest.type("write guard.txt protected\n");guest.wait_for("Wrote",timeout=30)
        finally:
            guest.call("screendump",{"filename":str(output/f"window-last-{args.vga}.png"),"format":"png"})
            guest.close();shutil.copyfile(directory/"debug.log",output/f"window-{args.vga}-debug.log")
        raw=bytearray(image.read_bytes());bpb=2048*512;reserved=struct.unpack_from('<H',raw,bpb+14)[0];fats=raw[bpb+16];spf=struct.unpack_from('<H',raw,bpb+22)[0];root=(2048+reserved+fats*spf)*512
        for offset in range(root,root+512*32,32):
            if raw[offset:offset+11]==b'GUARD   TXT':raw[offset+11]|=1;break
        else:raise AssertionError('Missing readonly fixture')
        image.write_bytes(raw)
        guest=Guest(qemu,None,directory,disk=image,attach_disk=not args.usb,extra_args=extra)
        try:
            # Fresh VM verifies cold-boot persistence and GUI save errors.
            guest.command("cat note.txt","hello!? from notepad")
            baseline=frames();surface=opened("notepad guard.txt");status(surface,"Opened")
            guest.type("blocked");guest.key("ctrl-s");status(surface,"Save failed: file is read-only")
            guest.key("ctrl-q");status(surface,"Unsaved changes");guest.key("d");guest.wait_for("User program exited with code 0.");assert frames()==baseline
            assert inspect_tree(image,2048)[0]["GUARD.TXT"]==b"protected"
        finally:guest.close();shutil.copyfile(directory/"debug.log",output/f"window-{args.vga}-cold-debug.log")
        guest=Guest(qemu,args.kernel,directory,disk=image,memory_mib=4,extra_args=["-vga","std"])
        try:
            baseline=frames();guest.command("notepad","not enough memory");guest.wait_for("User program exited with code 1.");assert frames()==baseline,"Low-memory open leaked frames"
        finally:guest.close()
        guest=Guest(qemu,args.kernel,directory,disk=image,extra_args=["-vga","cirrus"])
        try:
            baseline=frames();guest.command("notepad","Cannot open graphical window.");guest.wait_for("User program exited with code 1.");assert frames()==baseline,"Failed open leaked memory"
            guest.command("delete /apps/notepad.bin","Deleted:");guest.command("notepad","Program: file not found")
        finally:guest.close()
    print(f"PASS ring-3 windows/Notepad ({args.vga}, usb={args.usb}): pointer validation, atomic invalid batches, handles, protected drawing, fault cleanup, file saves/cold persistence, client clicks, dragging, toolbar, unsaved close and readonly retention")


if __name__=="__main__":main()
