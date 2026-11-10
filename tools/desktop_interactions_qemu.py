"""Private-disk checks for wheel routing, app-modal notifications and start-menu power."""
import argparse,json,re,shutil,tempfile,time,subprocess,sys
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/desktop-interactions';out.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-',dir=out) as tmp:
        d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
        pkg=d/'docs';(pkg/'apps').mkdir(parents=True);records=[]
        docs={'SCROLL.TXT':''.join(f'line{i:03} content\n' for i in range(70)).encode(),'ZZ.BAD':b'unknown'}
        docs.update({f'A{i:02}.DAT':b'fixture' for i in range(36)})
        for name,data in sorted(docs.items()):(pkg/'apps'/name.lower()).write_bytes(data);records.append({'file':name})
        (pkg/'applications.json').write_text(json.dumps(records));seed_apps(image,pkg,root_name='DOCS')
        subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(d/'font.exe')],check=True);font=subprocess.check_output([str(d/'font.exe')])
        helper=d/'helper';(helper/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/notice_check/main.rs'),'--format','elf','--output',str(helper/'apps/notice.elf')],check=True)
        (helper/'applications.json').write_text(json.dumps([{'file':'NOTICE.ELF'}]));seed_apps(image,helper,root_name='TOOLS')
        qemu=r'C:\Program Files\qemu\qemu-system-i386.exe';g=Guest(qemu,None,d,disk=image)
        try:
            def log():return (d/'debug.log').read_text(errors='replace')
            def until(fn,msg,seconds=45):
                end=time.monotonic()+seconds
                while True:
                    try:
                        if fn():return
                    except AssertionError:pass
                    assert time.monotonic()<end,msg+'\n'+log()[-1400:];time.sleep(.04)
            def tree():return inspect_tree(image,2048)[0]
            def window(pid):
                m=re.search(r'Multiwindow: open pid='+str(pid)+r'.*surface=([0-9a-f]+).*pos=(\d+),(\d+)',log());return int(m[1],16),int(m[2]),int(m[3])
            def launch(query):
                start=len(log());g.key('ctrl-esc');g.type(query+'\n');until(lambda:re.search(r'Desktop: launch pid=(\d+)',log()[start:]),'Launch '+query)
                pid=int(re.search(r'Desktop: launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid}' in log(),'Window '+query);return pid
            pointer=[400,300]
            def move(x,y):
                while pointer!=[x,y]:
                    dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
            def click(x,y):
                move(x,y)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.07)
            def wheel(down=True):
                for pressed in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'wheel-down' if down else 'wheel-up','down':pressed}}]});time.sleep(.1)
            def text(pid,x,y,label):
                surface,_,_=window(pid);f=d/'pixels.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {surface+y*640*4:#x} {640*16*4} "{f.as_posix()}"'});b=f.read_bytes()
                for i,c in enumerate(label):
                    for row in range(16):
                        for col in range(8):
                            if (int.from_bytes(b[(row*640+x+i*8+col)*4:(row*640+x+i*8+col+1)*4],'little')==0)!=bool(font[c*16+row]&(128>>col)):return False
                return True
            def notice(pid,kind,start):until(lambda:f'Dialog: pid={pid} kind={kind}' in log()[start:],'Notification kind '+str(kind))
            def dismiss(pid):
                start=len(log())
                for _ in range(5):
                    g.key('ret');time.sleep(.1)
                    if f'Dialog: dismissed pid={pid}' in log()[start:]:return
                raise AssertionError('Dialog did not dismiss')
            marker=len(log());g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log()[marker:],'Desktop')
            assert 'Mouse: IntelliMouse wheel enabled' in log(),'Wheel negotiation failed'
            note=launch('notepad');g.key('ctrl-o');g.type('/DOCS/SCROLL.TXT\n');until(lambda:text(note,8,32,b'ine000'),'Opened long document')
            _,x,y=window(note);move(x+200,y+150);wheel();until(lambda:text(note,0,32,b'line003'),'Notepad wheel viewport')
            g.type('X');start=len(log());g.key('ctrl-s');notice(note,0,start);until(lambda:tree()['DOCS/SCROLL.TXT'].startswith(b'Xline000'),'Wheel moved the caret')
            # Another app remains usable while Notepad owns an information dialog.
            counter=launch('counter');g.key('q');until(lambda:f'Scheduler: exit pid={counter}' in log(),'Other app blocked by modal')
            dismiss(note);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note}' in log(),'Notepad close')
            terminal=launch('terminal');start=len(log());g.type('run --isolated /TOOLS/NOTICE.ELF\n')
            until(lambda:re.search(r'Desktop: terminal launch pid=(\d+) name=/TOOLS/NOTICE.ELF',log()[start:]),'Restricted dialog helper')
            helper_pid=int(re.search(r'Desktop: terminal launch pid=(\d+)',log()[start:])[1]);notice(helper_pid,0,start)
            g.key('a');dismiss(helper_pid);until(lambda:f'Scheduler: exit pid={helper_pid} code=0' in log(),'Dialog boundary checks')
            start=len(log());g.type('run /APPS/EDIT/EDIT.BIN\n');until(lambda:'name=/APPS/EDIT/EDIT.BIN' in log()[start:],'Terminal child')
            def terminal_confirmation():
                surface,_,_=window(terminal);f=d/'confirm.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {surface+(64*656+24)*4:#x} 4 "{f.as_posix()}"'});return int.from_bytes(f.read_bytes(),'little')==0x94aabb
            g.key('alt-f4');until(terminal_confirmation,'Terminal termination confirmation');g.key('esc');time.sleep(.15);assert f'Scheduler: exit pid={terminal}' not in log()
            g.key('alt-f4');until(terminal_confirmation,'Terminal confirmation again');g.key('y');until(lambda:f'Scheduler: exit pid={terminal}' in log(),'Confirmed Terminal termination')
            terminal=launch('terminal');start=len(log());g.type('run /APPS/FILES/FILES.BIN /DOCS\n');until(lambda:re.search(r'Desktop: terminal launch pid=(\d+) name=/APPS/FILES/FILES.BIN',log()[start:]),'Files launch')
            files=int(re.search(r'Desktop: terminal launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={files}' in log(),'Files window')
            _,x,y=window(files);move(x+80,y+140);wheel();until(lambda:text(files,8,80,b'A03.DAT'),'Files wheel rows')
            g.key('end');start=len(log());g.key('ret');notice(files,2,start)
            before=log().count('Desktop: terminal launch pid=');g.key('a');wheel();time.sleep(.1);assert log().count('Desktop: terminal launch pid=')==before
            g.call('screendump',{'filename':str((out/'error-dialog.png').resolve()),'format':'png'})
            dismiss(files);start=len(log());g.key('ret');notice(files,2,start);g.key('esc');until(lambda:'Dialog: dismissed pid='+str(files) in log()[start:],'Escape dismissed error')
            assert f'Scheduler: exit pid={files}' not in log(),'Escape leaked through dialog'
            g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={files}' in log(),'Files close');g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={terminal}' in log(),'Terminal close')
            note=launch('notepad');start=len(log());g.key('ctrl-c');notice(note,1,start);dismiss(note);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note}' in log(),'Warning app close')
            # Start-menu controls retain captured release and explicit confirmation.
            g.key('ctrl-esc');click(280,104);g.key('esc');time.sleep(.2);assert g.process.poll() is None,'Cancel shut down the VM'
            click(410,104);g.call('screendump',{'filename':str((out/'restart-confirmation.png').resolve()),'format':'png'});g.key('y')
            until(lambda:g.process.poll() is not None,'Guest restart did not reset QEMU')
            assert 'Power: restart; disks synchronized' in log() and g.process.returncode==0
            shutil.copyfile(d/'debug.log',out/'before-restart.log');g.close()
            cold=d/'cold';cold.mkdir();g=Guest(qemu,None,cold,disk=image,start_desktop=True);pointer[:]=[400,300]
            g.key('ctrl-esc');click(280,104);g.key('y');until(lambda:g.process.poll() is not None,'Guest shutdown did not power off')
            assert g.process.returncode==0 and 'Power: disks synchronized' in (cold/'debug.log').read_text()
            print('PASS wheel negotiation, Notepad viewport/caret, Files wheel, app-modal info/error/warning, input isolation, repeat errors, power cancellation, synchronized reset, cold boot and shutdown')
        finally:
            g.close()
            if (d/'debug.log').exists():shutil.copyfile(d/'debug.log',out/'latest.log')
if __name__=='__main__':main()
