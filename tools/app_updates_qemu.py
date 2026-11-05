"""Check a paged launcher and staged versioned updates on a private disk."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--package',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/app-updates';out.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-',dir=out) as temporary:
        d=Path(temporary);image=d/'system.img';shutil.copyfile(a.disk,image)
        counter=(a.package/'apps/counter/counter.bin').read_bytes()
        def package(root,entries):
            folder=d/root;(folder/'apps').mkdir(parents=True)
            artifacts=[]
            for name,version,access,code,asset in entries:
                dest=folder/'apps'/name.lower();dest.mkdir()
                (dest/(name.lower()+'.bin')).write_bytes(code);(dest/'data.txt').write_bytes(asset)
                manifest=f'XINYI-APP 1\nexec={name}.BIN\ntitle={name}\ndescription=Update and pagination check\nicon=application\naccess={access}\nhandles=\n'
                if version is not None:manifest+=f'version={version}\n'
                (dest/(name.lower()+'.app')).write_text(manifest)
                artifacts.extend([{'directory':name,'file':name+'.BIN','manifest':name+'.APP'},{'directory':name,'file':'DATA.TXT'}])
            (folder/'applications.json').write_text(json.dumps(artifacts));seed_apps(image,folder,root_name=root)
        code=lambda status:bytes.fromhex('b801000000bb')+status.to_bytes(4,'little')+bytes.fromhex('cd800f0b')
        package('BASE',[('SAMPLE',1,'read-only',code(37),b'old asset'),('RUNBOX',None,'read-only',counter,b'old asset')])
        package('NEW',[('SAMPLE',2,'read-only',code(38),b'new asset'),('RUNBOX',2,'read-only',counter,b'new asset')])
        package('LOW',[('SAMPLE',1,'read-only',code(38),b'wrong')])
        package('ESC',[('SAMPLE',2,'trusted',code(38),b'wrong')])
        package('BAD',[('SAMPLE',3,'read-only',b'\x7fELFbad',b'wrong')])
        package('APPS',[(f'P{i:02}',1,'read-only',counter,b'fixture') for i in range(40)])
        helper=d/'helper';(helper/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/update_check/main.rs'),'--output',str(helper/'apps/upcheck.bin')],check=True)
        (helper/'applications.json').write_text(json.dumps([{'file':'UPCHECK.BIN'}]));seed_apps(image,helper,root_name='TOOLS')
        qemu=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
        g=Guest(qemu,None,d,disk=image)
        try:
            def log():return (d/'debug.log').read_text()
            def until(test,message):
                deadline=time.monotonic()+90
                while not test():
                    assert time.monotonic()<deadline,message+'\n'+log()[-2200:];time.sleep(.05)
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            g.command('use 1','Drive 1 ready');baseline=frames()
            def check(mode):
                g.command('run /tools/upcheck.bin '+mode,'App update checks passed!');g.wait_for('User program exited with code 0.');assert frames()==baseline,'Update helper leaked frames'
            g.command('run --read-only /tools/upcheck.bin restricted','App update checks passed!');g.wait_for('User program exited with code 0.')
            check('suite');g.command('run /apps/sample/sample.bin','User program exited with code 38.')
            g.command('cd /new','xinyi:/NEW>');before=len(log());g.type('desktop\n')
            until(lambda:'Desktop scheduler: ready' in log()[before:],'Desktop did not start')
            pointer=[400,300]
            def move(x,y):
                while pointer!=[x,y]:
                    dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
            def click(x,y,button='left'):
                move(x,y)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':button,'down':down}}]});time.sleep(.08)
            def launched(name,start):
                pattern=r'Desktop: launch pid=(\d+) path=/APPS/'+name+'/'+name+r'\.BIN'
                until(lambda:re.search(pattern,log()[start:]),'Launch '+name);pid=int(re.search(pattern,log()[start:])[1])
                until(lambda:f'Multiwindow: open pid={pid} ' in log()[start:],'Window '+name);return pid
            def close(pid):
                g.key('q');until(lambda:f'Scheduler: exit pid={pid} code=0' in log(),'Close '+str(pid))
            names=sorted({path.split('/')[-1][:-4] for path in inspect_tree(image,2048)[0] if path.startswith('APPS/') and path.endswith(('.BIN','.ELF'))})
            start=len(log());g.key('ctrl-esc');index=names.index('P39')
            for _ in range(index//16):click(444,530)
            click(48+(index%2)*228,188+((index%16)//2)*44)
            close(launched('P39',start))
            start=len(log());g.key('ctrl-esc');g.key('home')
            for _ in range(names.index('P09')):g.key('down')
            g.key('ret');close(launched('P09',start))
            start=len(log());g.key('ctrl-esc');g.key('pgdn');g.key('pgdn');g.type('p39\n');close(launched('P39',start))
            start=len(log());g.key('ctrl-esc');g.type('runbox\n');busy=launched('RUNBOX',start)
            start=len(log());g.key('ctrl-esc');g.type('terminal\n');terminal=launched('TERMINAL',start)
            start=len(log());g.type('run /tools/upcheck.bin busy\n')
            until(lambda:re.search(r'Desktop: terminal launch pid=(\d+) name=/tools/upcheck.bin',log()[start:]),'Busy update probe did not launch')
            pid=int(re.search(r'Desktop: terminal launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Scheduler: exit pid={pid} code=0' in log()[start:],'Running app update was not refused')
            g.key('alt-shift-tab');close(busy);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={terminal} code=0' in log(),'Terminal close')
            start=len(log());g.key('ctrl-esc');g.type('files\n');files_pid=launched('FILES',start)
            pos=re.search(r'Multiwindow: open pid='+str(files_pid)+r' .*pos=(\d+),(\d+)',log()[start:]);fx,fy=map(int,pos.groups())
            g.key('home');g.key('down') # NEW contains SAMPLE, then RUNBOX.
            def update_menu():
                click(fx+8+60,fy+34+104,'right');click(fx+8+140,fy+34+339)
            update_menu();g.key('n')
            assert inspect_tree(image,2048)[0]['APPS/RUNBOX/DATA.TXT']==b'old asset','Cancelled Files update wrote data'
            update_menu();g.key('y')
            def updated():
                try:
                    entries=inspect_tree(image,2048)[0]
                    return entries.get('APPS/RUNBOX/DATA.TXT')==b'new asset' and entries.get('APPS/_UPD.ID')==b'RUNBOX'
                except AssertionError:return False
            until(updated,'Confirmed Files update did not publish replacement')
            g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={files_pid} code=0' in log(),'Files close')
            g.key('esc');g.wait_for('Desktop finished.');assert frames()==baseline,'Paged apps or busy probe leaked frames'
            check('finish')
            files,dirs=inspect_tree(image,2048)
            assert not any(path.startswith(('APPS/_STAGE','APPS/_BACKUP','APPS/_TRASH','APPS/_UPD.ID')) for path in files|dict.fromkeys(dirs))
            assert files['BASE/SAMPLE/DATA.TXT']==b'old asset' and files['NEW/SAMPLE/DATA.TXT']==b'new asset'
            assert 'Scheduler: fault' not in log() and 'Xinyi panic:' not in log()
            print('PASS app updates: 55+ app catalogue, page clicks/keyboard/global search, versions, permission expansion and downgrade refusal, malformed image/stage preservation, rollback/finalize, running-app refusal, Files update confirmation/cancel, recovery of an unpublished app, exact assets and frame balance',flush=True)
        finally:
            g.close();shutil.copyfile(d/'debug.log',out/'debug.log');shutil.copyfile(d/'qemu.stderr',out/'qemu.stderr')

if __name__=='__main__':main()
