"""Check refreshed Settings controls, preview/save semantics and cleanup."""
import argparse, re, shutil, tempfile, time
from pathlib import Path
from smoke import Guest, ROOT
from folders_qemu import inspect_tree

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/settings';out.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-',dir=out) as temporary:
        d=Path(temporary);image=d/'system.img';shutil.copyfile(a.disk,image)
        qemu=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
        g=Guest(qemu,None,d,disk=image)
        try:g.command('display 800x600','Saved 800x600');g.command('uiscale 100','Saved UI scale 100%')
        finally:g.close()
        g=Guest(qemu,None,d,disk=image)
        try:
            def log():return (d/'debug.log').read_text()
            def until(check,message):
                deadline=time.monotonic()+30
                while not check():
                    assert time.monotonic()<deadline,message+'\n'+log()[-1000:];time.sleep(.05)
            def files():
                try:return inspect_tree(image,2048)[0]
                except AssertionError:return {}
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            baseline=frames();start=len(log());g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log()[start:],'Desktop')
            g.key('ctrl-esc');g.type('settings\n')
            until(lambda:re.search(r'Multiwindow: open pid=(\d+).*client=480x416 pos=(\d+),(\d+)',log()[start:]),'Settings')
            pid,wx,wy=map(int,re.search(r'Multiwindow: open pid=(\d+).*client=480x416 pos=(\d+),(\d+)',log()[start:]).groups())
            pointer=[400,300]
            def click(x,y):
                x+=wx+8;y+=wy+34
                while pointer!=[x,y]:
                    dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
            click(120,218);g.type('294f9c\n');click(50,80);click(120,218);g.type('526a83\n');click(160,356)
            until(lambda:files().get('GUI.CFG')==b'294F9C 526A83\n','Colour save')
            click(40,296);click(240,296);click(132,296);g.key('ctrl-s')
            until(lambda:files().get('SOUND.CFG')==b'SND1'+(10).to_bytes(4,'little'),'Volume/save controls')
            click(50,44);click(120,218);g.type('112233\n');click(50,356)
            assert files()['GUI.CFG']==b'294F9C 526A83\n','Apply unexpectedly persisted the preview'
            g.key('ctrl-s');until(lambda:files().get('GUI.CFG')==b'112233 526A83\n','Save preview')
            g.call('screendump',{'filename':str(out/'settings-preview.png'),'format':'png'})
            click(400,356);until(lambda:f'Scheduler: exit pid={pid} code=0' in log(),'Close button')
            g.key('esc');g.wait_for('Desktop finished.');assert frames()==baseline,'Settings leaked frames'
        finally:g.close();shutil.copyfile(d/'debug.log',out/'debug.log')
    print('PASS Settings: custom RGB, colour targets, flat buttons, volume/mute, Apply versus Save, Ctrl+S, close and frame balance')

if __name__=='__main__':main()
