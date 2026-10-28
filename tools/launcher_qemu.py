"""Test dynamic Apps menus and synchronous desktop-to-program return."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
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
    with tempfile.TemporaryDirectory(prefix="launcher-",dir=output) as temporary:
        directory=Path(temporary);image=directory/"system.img"
        def boot():
            extra=["-vga",args.vga]
            if args.usb:extra += ["-drive",f"if=none,id=stick,file={image.as_posix()},format=raw","-device","piix3-usb-uhci,id=uhci","-device","usb-storage,bus=uhci.0,drive=stick,bootindex=1"]
            return Guest(qemu,None,directory,disk=image,attach_disk=not args.usb,extra_args=extra)
        if args.disk:shutil.copyfile(args.disk,image)
        else:
            with image.open("xb") as stream:stream.truncate(16*1024*1024)
            guest=Guest(qemu,args.kernel,directory,disk=image)
            try:
                guest.command("install","To install, enter: install confirm");guest.type("install confirm\n");guest.wait_for("Installation complete:",timeout=45)
            finally:guest.close()
            # Empty/missing Apps must remain a usable desktop.
            guest=boot()
            try:
                guest.type("windowtest\n");deadline=time.monotonic()+20
                while "Desktop: frame ready" not in (directory/"debug.log").read_text():
                    assert time.monotonic()<deadline;time.sleep(.02)
                guest.key("esc");guest.wait_for("Returned to text mode.")
            finally:guest.close()
            seed_apps(image,args.package)
        extra=directory/"extra";(extra/"apps").mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/"tools/build_program.py"),str(ROOT/"programs/launcher_check/main.rs"),"--output",str(extra/"apps/custom.bin")],check=True)
        (extra/"apps/crash.bin").write_bytes(bytes.fromhex('0f0b'))
        (extra/"applications.json").write_text(json.dumps([{"name":"custom","file":"CUSTOM.BIN"},{"name":"crash","file":"CRASH.BIN"}]))
        seed_apps(image,extra)
        guest=boot()
        try:
            def log():return (directory/"debug.log").read_text()
            def until(test,message):
                deadline=time.monotonic()+30
                while not test():assert time.monotonic()<deadline,message+"\n"+log()[-1000:];time.sleep(.02)
            def frames():return int(re.search(r"Free now: +(\d+)",guest.command("frames","Free now:"))[1])
            guest.command("touch /apps/bad.bin","Created:")
            guest.command("touch /apps/readme.txt","Created:")
            guest.command("mkdir /apps/tools.bin","Created directory:")
            guest.command("mkdir docs","Created directory:")
            guest.command("cd docs","xinyi:/DOCS>")
            guest.command("touch custom.bin","Created:") # Must not shadow /APPS/CUSTOM.BIN.
            baseline=frames();guest.type("windowtest\n");until(lambda:"Desktop: frame ready" in log(),"Desktop startup")
            pos=[400,300]
            def move(x,y):
                while pos!=[x,y]:
                    dx=max(-80,min(80,x-pos[0]));dy=max(-80,min(80,y-pos[1]));guest.call("input-send-event",{"events":[{"type":"rel","data":{"axis":"x","value":dx}},{"type":"rel","data":{"axis":"y","value":dy}}]});pos[0]+=dx;pos[1]+=dy;time.sleep(.04)
            def button(down):guest.call("input-send-event",{"events":[{"type":"btn","data":{"button":"left","down":down}}]});time.sleep(.08)
            def click(x,y):move(x,y);button(True);button(False)
            move(140,155);button(True);move(240,135);button(False);click(240,220)
            until(lambda:"pos=200,120;300,210" in log() and "clicks=1/0" in log(),"Desktop setup state")
            names=['BAD','COUNTER','CRASH','CUSTOM','EDIT','FILES','HELLO','MEMDEMO','NOTEPAD','SETTINGS','SNAKE','TERMINAL','TETRIS']
            def launch(name):
                menu_offset=len(log());click(40,11);until(lambda:"apps=true" in log()[menu_offset:],"Apps menu did not open");top=30
                guest.call("screendump",{"filename":str(output/f"apps-menu-{args.vga}.png"),"format":"png"})
                offset=len(log());click(40,top+4+names.index(name)*24+12)
                until(lambda:f"Desktop: launch /APPS/{name}.BIN" in log()[offset:],"Launch "+name)
                return offset
            def returned(offset):
                until(lambda:"Desktop: frame ready" in log()[offset:],"Desktop did not resume")
                assert "pos=200,120;300,210 clicks=1/0" in log()[offset:],"Desktop state was lost"
            offset=launch('CUSTOM');returned(offset)
            names.insert(0,'ADDED')
            assert inspect_tree(image,2048)[0]['APPS/ADDED.BIN']==inspect_tree(image,2048)[0]['APPS/HELLO.BIN']
            offset=launch('ADDED');returned(offset)
            offset=launch('COUNTER');until(lambda:'client=320x144' in log()[offset:],'Sized foreground app did not open');guest.key('q');returned(offset)
            offset=launch('NOTEPAD');until(lambda:'Window: opened' in log()[offset:],'Graphical app did not open')
            until(lambda:'nr=61' in log()[log().index('Window: opened',offset):],'Notepad did not wait for input')
            guest.type('launched from desktop');guest.key('ctrl-s');guest.type('launch.txt\n')
            def saved():
                try:return inspect_tree(image,2048)[0].get('DOCS/LAUNCH.TXT')==b'launched from desktop'
                except AssertionError:return False
            until(saved,'Notepad did not save in the current directory')
            guest.key('ctrl-q');returned(offset)
            offset=launch('EDIT');guest.wait_for('Xinyi Editor');guest.key('ctrl-q');returned(offset)
            offset=launch('CRASH');returned(offset);assert 'Invalid Opcode' in log()[offset:]
            offset=launch('BAD');returned(offset)
            guest.call("screendump",{"filename":str(output/f"desktop-return-{args.vga}.png"),"format":"png"})
            guest.key('esc');guest.wait_for('Returned to text mode.');assert frames()==baseline,'Desktop launch/return leaked frames'
            guest.command('pwd','/DOCS');guest.command('cat launch.txt','launched from desktop')
        finally:
            guest.close();shutil.copyfile(directory/'debug.log',output/f'launcher-{args.vga}-debug.log')
    print(f'PASS Apps launcher ({args.vga}): runtime catalog/refresh, explicit paths/CWD, graphical/text app launch-return, saved files, crash/empty-file recovery, persistent desktop state and frame balance')


if __name__=='__main__':main()
