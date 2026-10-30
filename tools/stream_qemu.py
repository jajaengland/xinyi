"""Exercise positional file ABI in foreground and queued desktop contexts."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from folders_qemu import inspect_tree
from seed_apps import seed_apps
from smoke import Guest,ROOT

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);a=p.parse_args()
    output=ROOT/'target/smoke';output.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='stream-',dir=output) as tmp:
        d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
        extra=d/'extra';(extra/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/stream_check/main.rs'),'--format','elf','--output',str(extra/'apps/stream.elf')],check=True)
        (extra/'applications.json').write_text(json.dumps([{'name':'stream','file':'STREAM.ELF'}]));seed_apps(image,extra)
        qemu=r'C:\Program Files\qemu\qemu-system-i386.exe'
        g=Guest(qemu,None,d,disk=image)
        expected=bytes((i*17+i//512)%251 for i in range(98317))
        try:
            # Native IDE avoids expensive BIOS copies for the mutation stress test.
            g.command('use 1','ready')
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            base=frames();g.command('run stream.elf','Streaming file checks passed!');g.wait_for('User program exited with code 0.');assert frames()==base
            files,_=inspect_tree(image,2048);assert files['STREAM.BIN']==expected
            g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
            def log():return (d/'debug.log').read_text()
            def until(fn,message):
                end=time.monotonic()+90
                while not fn():
                    assert time.monotonic()<end,message+'\n'+log()[-1500:];time.sleep(.04)
            g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'desktop startup')
            names=sorted(k.split('/')[-1][:-4] for k in files if k.startswith('APPS/') and k.endswith(('.BIN','.ELF')))
            def launch(name):
                start=len(log());g.key('ctrl-esc');g.key('home')
                for _ in range(names.index(name)):g.key('down')
                g.key('ret');until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name,log()[start:]),'launch '+name)
                pid=int(re.search(r'Desktop: launch pid=(\d+) ',log()[start:])[1]);return pid,start
            counter,_=launch('COUNTER');until(lambda:f'Multiwindow: open pid={counter} ' in log(),'counter window')
            m=re.search(r'Multiwindow: open pid='+str(counter)+r' handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+)',log())
            surface=int(m[1],16);width=int(m[2])
            def counter_pixels():
                target=d/'counter.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {surface+width*92*4:#x} {width*18*4} "{target.as_posix()}"'});return target.read_bytes()
            before=counter_pixels();stream,start=launch('STREAM')
            until(lambda:counter_pixels()!=before,'Counter stopped during streaming calls')
            assert f'Scheduler: exit pid={stream} code=0' not in log()[start:],'Counter advanced only after streaming finished'
            until(lambda:f'Scheduler: exit pid={stream} code=0' in log()[start:],'queued streaming checks')
            assert re.search(r'Disk I/O: start pid='+str(stream)+r' nr=9[1-4]',log()[start:]),'new ABI bypassed worker'
            assert re.search(r'Multiwindow: open pid='+str(counter),log()),'Counter disappeared'
            viewer,start=launch('VIEWER');until(lambda:f'Multiwindow: open pid={viewer} ' in log()[start:],'viewer window')
            g.type('/stream.bin\n');until(lambda:re.search(r'Disk I/O: done pid='+str(viewer)+r'.*result=256',log()[start:]),'viewer read')
            start=len(log());g.key('end');until(lambda:re.search(r'Disk I/O: done pid='+str(viewer)+r'.*result=13',log()[start:]),'viewer end')
            g.call('screendump',{'filename':str(output/'stream-viewer.png'),'format':'png'})
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={viewer} code=0' in log(),'viewer exit')
            # The console test creates a Terminal wrapper for its final status message.
            wrappers=[int(p) for p in re.findall(r'Multiwindow: open pid=(\d+) ',log()) if int(p) not in (viewer,counter)]
            for pid in list(reversed(wrappers))+[counter]:
                g.key('alt-f4');until(lambda:f'Scheduler: exit pid={pid} code=0' in log(),'close remaining window '+str(pid))
            time.sleep(.3)
            g.key('esc')
            g.wait_for('Desktop finished.');assert frames()==base,'Streaming API/viewer leaked frames'
            assert inspect_tree(image,2048)[0]['STREAM.BIN']==expected
            g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
        finally:
            g.close();shutil.copyfile(d/'debug.log',output/'stream-debug.log')
        g=Guest(qemu,None,d,disk=image)
        try:
            # Cold boot returns to BIOS storage; reads cross the previous 64 KiB limit.
            g.command('run stream.elf again','Streaming file checks passed!');g.wait_for('User program exited with code 0.')
            assert inspect_tree(image,2048)[0]['STREAM.BIN']==expected
        finally:g.close()
    print('PASS streaming: native writes, BIOS cold reads, >64 KiB data, queued desktop calls, viewer paging, pointer/volume/size bounds, fsck and frame balance')

if __name__=='__main__':main()
