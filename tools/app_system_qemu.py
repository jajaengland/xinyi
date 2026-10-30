"""Verify disk manifests, restrictive launch policy, associated opens and installer copying."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from array import array
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree
from install_qemu import verify_payload
from desktop_controls import task_button,SHELL_BUTTON

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--kernel',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(ROOT/'target/font-dump.exe')],check=True)
    font=subprocess.check_output([str(ROOT/'target/font-dump.exe')]);glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
    with tempfile.TemporaryDirectory(prefix='app-system-',dir=out) as tmp:
        d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image);package=d/'extra';(package/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/app_check/main.rs'),'--format','elf','--output',str(package/'apps/metahelp.elf')],check=True)
        shutil.copyfile(package/'apps/metahelp.elf',package/'apps/aptest.elf');(package/'applications.json').write_text(json.dumps([{'file':'METAHELP.ELF'},{'file':'APTEST.ELF'}]));seed_apps(image,package)
        qemu=r'C:\Program Files\qemu\qemu-system-i386.exe';g=Guest(qemu,None,d,disk=image,extra_args=['-audiodev','none,id=pcm','-device','AC97,audiodev=pcm'])
        try:
            def log():return (d/'debug.log').read_text(errors='replace')
            def until(fn,msg):
                end=time.monotonic()+40
                while True:
                    try:
                        if fn():return
                    except AssertionError:pass
                    assert time.monotonic()<end,msg+'\n'+log()[-2000:];time.sleep(.04)
            def check(command):g.command(command,'Manifest checks passed!');g.wait_for('User program exited with code 0.')
            for command in ['run metahelp make-ro','run aptest ro','run --isolated aptest none','run metahelp make-media','run aptest media','run --read-only aptest ro','run metahelp make-trusted','run --isolated aptest none','run metahelp make-bad']:
                check(command)
            g.command('run aptest trusted','Program: Invalid application manifest.')
            for command in ['run metahelp remove','run aptest trusted','run metahelp api','run metahelp ambiguous','run metahelp ambiguous-check','run metahelp clear','run metahelp docs','run metahelp title','run metahelp player-trusted']:check(command)
            assert '.APP' not in g.command('apps','EDIT.BIN') # CLI lists executable artifacts.
            baseline=int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1]);g.command('cd /docs','xinyi:/DOCS>');g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            def window(pid):
                m=re.search(r'Multiwindow: open pid='+str(pid)+r' handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log());return (int(m[1],16),*(int(v) for v in m.groups()[1:]))
            def launch(query,name):
                start=len(log());g.key('ctrl-esc');g.type(query+'\n');until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name,log()[start:]),'launcher query '+query)
                pid=int(re.search(r'Desktop: launch pid=(\d+) ',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid}' in log(),'window '+name);return pid
            renamed=launch('notebook','NOTEPAD');g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={renamed} code=0' in log(),'renamed app close')
            files=launch('files','FILES');pointer=[400,300]
            def click(x,y):
                while pointer!=[x,y]:
                    dx=max(-80,min(80,x-pointer[0]));dy=max(-80,min(80,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.07)
            def mem(address,n):
                f=d/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {address:#x} {n} "{f.as_posix()}"'});return f.read_bytes()
            def text(pid,y,length=78):
                addr,w,_,_,_=window(pid);pixels=array('I');pixels.frombytes(mem(addr+y*w*4,w*16*4));fg=0xffffff if pixels[8]==0x153e8e else 0
                return ''.join(glyphs.get(bytes(pixels[dy*w+8+col*8+dx]==fg for dy in range(16) for dx in range(8)),'?') for col in range(length)).rstrip()
            def open_file(name):
                until(lambda:any(text(files,80+i*16,12)==name for i in range(8)),'file list '+name)
                row=next(i for i in range(8) if text(files,80+i*16,12)==name);_,_,_,x,y=window(files);click(x+40,y+34+80+row*16+8);start=len(log());g.key('ret');return start
            def associated(name,start):
                until(lambda:re.search(r'Desktop: terminal launch pid=(\d+) name=/APPS/(?:[A-Z0-9_]+/)?'+name,log()[start:]),'associated '+name)
                pid=int(re.search(r'Desktop: terminal launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid}' in log(),'associated window '+name);return pid
            start=open_file('README.TXT');note=associated('NOTEPAD/NOTEPAD.BIN',start);g.key('ctrl-end');g.type(' updated');g.key('ctrl-s')
            until(lambda:inspect_tree(image,2048)[0].get('DOCS/README.TXT')==b'manifest document updated','associated text app lost its working directory')
            g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note} code=0' in log(),'Notepad close')
            start=open_file('TONE.WAV');player=associated('PLAYER/PLAYER.ELF',start);until(lambda:f'Audio: open pid={player}' in log()[start:],'WAV association did not play')
            assert 'App manifest: path=/APPS/PLAYER/PLAYER.ELF access=media' in log()[start:]
            g.call('screendump',{'filename':str(out/'application-associations.png'),'format':'png'});g.key('alt-f4');until(lambda:f'Scheduler: exit pid={player} code=0' in log(),'Player close')
            start=open_file('DATA.DAT');viewer=associated('VIEWER/VIEWER.BIN',start)
            assert 'App manifest: path=/APPS/VIEWER/VIEWER.BIN access=read-only' in log()[start:]
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={viewer} code=0' in log(),'Viewer close')
            count=log().count('Desktop: terminal launch pid=');open_file('UNKNOWN.ZZZ');until(lambda:'No installed application handles' in text(files,384),'unknown-file message')
            assert log().count('Desktop: terminal launch pid=')==count,'Unknown binary opened as text'
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={files} code=0' in log(),'Files close');click(*SHELL_BUTTON);g.wait_for('Desktop finished.')
            assert int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])==baseline,'Manifest/app buffers leaked'
            g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
        finally:g.close();shutil.copyfile(d/'debug.log',out/'app-system-debug.log')
        # Installer must copy the executable sidecars as well as large binaries.
        destination=d/'installed.img'
        with destination.open('xb') as f:f.truncate(16*1024*1024)
        expected={k:v for k,v in inspect_tree(image,2048)[0].items() if k.startswith('APPS/')}
        g=Guest(qemu,None,d,disk=image,extra_args=['-drive',f'file={destination.as_posix()},format=raw,if=ide,index=1'])
        try:g.command('use 1','ready');g.command('install 2','To install, enter: install confirm');g.type('install confirm\n');g.wait_for('Installed drive ready',timeout=90)
        finally:g.close()
        verify_payload(destination,a.kernel);assert inspect_tree(destination,2048)[0]==expected,'Installer dropped manifests'
        g=Guest(qemu,None,d,disk=destination,disk_index=1)
        try:check('run aptest trusted');g.command('apps','EDIT.BIN')
        finally:g.close()
    print('PASS app system: metadata titles, malformed-manifest refusal, profile intersection, legacy compatibility, stale-volume launch denial, ambiguous/unknown handlers, Files TXT/WAV routing, Media playback, cwd, installer sidecars, cold boot, fsck and frame balance')

if __name__=='__main__':main()
