"""Permission enforcement against raw legacy/new calls, including shell bypasses."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from folders_qemu import inspect_tree
from seed_apps import seed_apps
from smoke import Guest,ROOT

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='permissions-',dir=out) as tmp:
        d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
        package=d/'extra';(package/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/permission_check/main.rs'),'--format','elf','--output',str(package/'apps/access.elf')],check=True)
        (package/'applications.json').write_text(json.dumps([{'name':'access','file':'ACCESS.ELF'}]));seed_apps(image,package)
        qemu=r'C:\Program Files\qemu\qemu-system-i386.exe'
        g=Guest(qemu,None,d,disk=image)
        try:
            g.command('use 1','ready');g.command('touch keep.txt','Created:');g.command('write keep.txt immutable','Wrote')
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            baseline=frames();before=image.read_bytes()
            for mode,label,argument in [('--read-only','Read-only','ro'),('--isolated','Isolated','none')]:
                g.command('run '+mode+' access.elf '+argument,label+' permission checks passed!')
                g.wait_for('User program exited with code 0.');assert frames()==baseline
                assert image.read_bytes()==before,mode+' changed disk bytes'
            g.command('run access.elf trusted','Trusted permission checks passed!');g.wait_for('User program exited with code 0.')
            files,_=inspect_tree(image,2048);assert files['KEEP.TXT']==b'immutable';assert files['ALLOW.TXT']==b'trusted write'
            assert frames()==baseline
            def log():return (d/'debug.log').read_text()
            def until(fn,message):
                deadline=time.monotonic()+45
                while not fn():
                    assert time.monotonic()<deadline,message+'\n'+log()[-1500:];time.sleep(.04)
            g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            names=sorted(k.split('/')[-1][:-4] for k in files if k.startswith('APPS/') and k.endswith(('.BIN','.ELF')))
            def launch(name):
                start=len(log());g.key('ctrl-esc');g.key('home')
                for _ in range(names.index(name)):g.key('down')
                g.key('ret');until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name,log()[start:]),'launch '+name)
                pid=int(re.search(r'Desktop: launch pid=(\d+) ',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log(),'window '+name)
                return pid
            counter=launch('COUNTER');terminal=launch('TERMINAL')
            # Kernel shell parsing carries the selected restriction into the queued child.
            for mode,label,argument in [('--read-only','read-only','ro'),('--isolated','isolated','none'),('','trusted','trusted')]:
                start=len(log());g.type('run '+mode+' access.elf '+argument+'\n')
                until(lambda:re.search(r'Desktop: terminal launch pid=(\d+) name=access.elf',log()[start:]),'terminal child '+label)
                pid=int(re.search(r'Desktop: terminal launch pid=(\d+) ',log()[start:])[1])
                until(lambda:f'Scheduler: exit pid={pid} code=0' in log()[start:],'permission probe '+label)
                section=log()[start:]
                if label!='trusted':
                    assert f'Permission denied: pid={pid} nr=71 access={label}' in section
                    assert not re.search(r'Disk I/O: start pid='+str(pid)+r' nr=(58|80|81|89|92|94)\b',section),'denied mutation was queued'
                    assert not re.search(r'Desktop: terminal launch pid=\d+ name=terminal\b',section),'restricted app launched a privileged child'
                if label=='read-only':assert f'Disk I/O: start pid={pid} nr=91' in section,'read did not use worker'
                if label=='isolated':assert not re.search(r'Disk I/O: start pid='+str(pid)+r' ',section),'isolated process reached disk worker'
                assert inspect_tree(image,2048)[0]['KEEP.TXT']==b'immutable'
                time.sleep(.15)
            viewer=launch('VIEWER');g.type('/keep.txt\n');until(lambda:re.search(r'Disk I/O: done pid='+str(viewer)+r'.*result=9',log()),'default read-only viewer')
            g.call('screendump',{'filename':str(out/'permissions-viewer.png'),'format':'png'})
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={viewer} code=0' in log(),'viewer close')
            # Isolated terminal cannot open a kernel shell session to bypass its restriction.
            start=len(log());g.type('run --isolated terminal\n')
            until(lambda:re.search(r'Desktop: terminal launch pid=(\d+) name=terminal',log()[start:]),'isolated terminal launch')
            pid=int(re.search(r'Desktop: terminal launch pid=(\d+) ',log()[start:])[1])
            until(lambda:re.search(r'Scheduler: exit pid='+str(pid)+r' code=',log()[start:]),'isolated terminal exit')
            assert f'Permission denied: pid={pid} nr=72 access=isolated' in log()[start:]
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={terminal} code=0' in log(),'terminal close')
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={counter} code=0' in log(),'counter close');time.sleep(.3)
            g.key('esc')
            g.wait_for('Desktop finished.');assert frames()==baseline,'permission/window frame leak'
            assert inspect_tree(image,2048)[0]['KEEP.TXT']==b'immutable'
        finally:g.close();shutil.copyfile(d/'debug.log',out/'permissions-debug.log')
    print('PASS permissions: raw legacy/new denials before I/O, read-only reads, zero isolated disk access, clipboard/PTY/child-launch bypass refusal, trusted compatibility, default Viewer, process switching and frame balance')

if __name__=='__main__':main()
