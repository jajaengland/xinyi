"""Exercise app folders, transactional install/remove, Files UI and system installation."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree
from install_qemu import verify_payload
from desktop_controls import SHELL_BUTTON,task_button

def fixtures(image,package,directory):
    inbox=directory/'inbox';(inbox/'apps').mkdir(parents=True)
    entries=[]
    for name,source,ext,access in [('SAMPLE','counter','BIN','trusted'),('BIGTEST','bigdemo','ELF','isolated'),('INVALID','hello','BIN','trusted')]:
        folder=inbox/'apps'/name.lower();folder.mkdir()
        shutil.copyfile(package/'apps'/source/(source+'.'+ext.lower()),folder/(name.lower()+'.'+ext.lower()))
        metadata=f'XINYI-APP 1\nexec={name}.{ext}\ntitle={name.title()}\ndescription=Installable sample application\nicon=application\naccess={access}\nhandles=\n'
        if name=='INVALID':metadata=metadata.replace(f'exec={name}.{ext}','exec=OTHER.BIN')
        (folder/(name.lower()+'.app')).write_text(metadata)
        entries.append({'directory':name,'file':name+'.'+ext,'manifest':name+'.APP'})
    (inbox/'applications.json').write_text(json.dumps(entries));return seed_apps(image,inbox,'INBOX')

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--package',type=Path,required=True);p.add_argument('--kernel',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='app-install-',dir=out) as tmp:
        d=Path(tmp);image=d/'test.img';shutil.copyfile(a.disk,image);fixtures(image,a.package,d)
        helper=d/'helper';(helper/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/install_check/main.rs'),'--format','elf','--output',str(helper/'apps/instchk.elf')],check=True)
        (helper/'applications.json').write_text(json.dumps([{'file':'INSTCHK.ELF'}]));seed_apps(image,helper,'TOOLS')
        qemu=r'C:\Program Files\qemu\qemu-system-i386.exe';g=Guest(qemu,None,d,disk=image)
        try:
            def check(argument,profile=''):
                g.command('run '+profile+'/TOOLS/INSTCHK.ELF '+argument,'App installation checks passed!');g.wait_for('User program exited with code 0.')
            baseline=int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            check('restricted','--read-only ');check('restricted','--isolated ');check('api');check('stage');check('install')
            files,dirs=inspect_tree(image,2048)
            for path,content in list(files.items()):
                if path.startswith('INBOX/BIGTEST/'):assert files[path.replace('INBOX/','APPS/',1)]==content
            assert len(files['APPS/BIGTEST/BIGTEST.ELF'])>65536
            assert 'APPS/SAMPLE' not in dirs and 'APPS/_STAGE' not in dirs and 'APPS/_TRASH' not in dirs
            g.command('run bigtest check','Large ELF checks passed!');g.wait_for('User program exited with code 0.')
            check('cleanup')
            assert int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])==baseline,'Install/remove leaked frames'
            g.command('cd /inbox','xinyi:/INBOX>');g.type('desktop\n')
            def log():return (d/'debug.log').read_text(errors='replace')
            def until(fn,message):
                end=time.monotonic()+45
                while True:
                    try:
                        if fn():return
                    except AssertionError:pass
                    assert time.monotonic()<end,message+'\n'+log()[-2200:];time.sleep(.04)
            until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            def launch(query):
                start=len(log());g.key('ctrl-esc');g.type(query+'\n');until(lambda:'Multiwindow: open pid=' in log()[start:],'launch '+query)
                return int(re.search(r'Multiwindow: open pid=(\d+)',log()[start:])[1])
            files_pid=launch('files');pointer=[400,300]
            def click(x,y):
                while pointer!=[x,y]:
                    dx=max(-80,min(80,x-pointer[0]));dy=max(-80,min(80,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
            m=re.search(r'Multiwindow: open pid='+str(files_pid)+r'.*client=(\d+)x(\d+) pos=(\d+),(\d+)',log());width,height,x,y=map(int,m.groups());x+=8;y+=34
            def install_button():click(x+4*(width//9)+25,y+46)
            def remove_button():click(x+5*(width//9)+25,y+46)
            g.key('home');install_button();g.key('n');time.sleep(.2);assert 'APPS/SAMPLE' not in inspect_tree(image,2048)[1],'Cancelled installation wrote an app'
            install_button();g.key('y');until(lambda:'APPS/SAMPLE/ASSETS/MORE/DATA.TXT' in inspect_tree(image,2048)[0],'Files install')
            assert inspect_tree(image,2048)[0]['APPS/SAMPLE/ASSETS/EMPTY.TXT']==b''
            sample=launch('sample')
            # Return Files to /APPS, select the installed folder and try removing a live app.
            click(*task_button(0));g.key('backspace');g.key('home');g.key('ret');time.sleep(.3);g.key('end');remove_button();g.key('y');time.sleep(.4)
            assert 'APPS/SAMPLE/SAMPLE.BIN' in inspect_tree(image,2048)[0],'Removed a running app'
            click(*task_button(1));g.key('alt-f4');until(lambda:f'Scheduler: exit pid={sample} ' in log(),'sample close')
            click(*task_button(0));remove_button();g.key('n');assert 'APPS/SAMPLE/SAMPLE.BIN' in inspect_tree(image,2048)[0]
            remove_button();g.key('y');until(lambda:'APPS/SAMPLE' not in inspect_tree(image,2048)[1],'Files removal')
            assert inspect_tree(image,2048)[0]['INBOX/SAMPLE/ASSETS/MORE/DATA.TXT']==b'keep this asset','Source assets were removed'
            g.call('screendump',{'filename':str(out/'app-installation.png'),'format':'png'})
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={files_pid} ' in log(),'Files close');click(*SHELL_BUTTON);g.wait_for('Desktop finished.')
            g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
            assert int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])==baseline,'GUI management leaked frames'
        finally:g.close();shutil.copyfile(d/'debug.log',out/'app-install-debug.log')
        # Whole-system installation must preserve app directories and all bundled files.
        destination=d/'installed.img'
        with destination.open('xb') as f:f.truncate(16*1024*1024)
        expected={k:v for k,v in inspect_tree(image,2048)[0].items() if k.startswith('APPS/')}
        g=Guest(qemu,None,d,disk=image,extra_args=['-drive',f'file={destination.as_posix()},format=raw,if=ide,index=1'])
        try:g.command('use 1','ready');g.command('install 2','To install, enter: install confirm');g.type('install confirm\n');g.wait_for('Installed drive ready',timeout=90)
        finally:g.close()
        verify_payload(destination,a.kernel);assert inspect_tree(destination,2048)[0]==expected,'System installation lost bundled apps'
        g=Guest(qemu,None,d,disk=destination,disk_index=1)
        try:g.command('apps','EDIT.BIN');g.command('run hello','User program exited with code 0.')
        finally:g.close()
    print('PASS app installation: folder packaging, malformed/refused duplicate installs, profiles, pointer/volume validation, staging recovery, large ELF, nested/empty assets, Files install/cancel/remove, running-app refusal, source preservation, frame balance, fsck and whole-system cold boot')

if __name__=='__main__':main()
