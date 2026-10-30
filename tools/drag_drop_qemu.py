"""Drag actual files between Files windows on a disposable boot image."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from seed_apps import seed_apps
from pathlib import Path
from smoke import Guest,ROOT
from folders_qemu import inspect_tree
from desktop_controls import task_button,SHELL_BUTTON

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);a=p.parse_args();out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='drag-drop-',dir=out) as tmp:
        d=Path(tmp);image=d/'test.img';shutil.copyfile(a.disk,image)
        package=d/'helper';(package/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/drag_check/main.rs'),'--output',str(package/'apps/dndchk.bin')],check=True)
        (package/'applications.json').write_text(json.dumps([{'file':'DNDCHK.BIN'}]));seed_apps(image,package)
        g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image)
        try:
            for cmd in ['mkdir /source','mkdir /dest','mkdir /dest/sub','mkdir /dest/multi','touch /source/a.txt','write /source/a.txt alpha','touch /source/b.txt','write /source/b.txt beta','mkdir /source/folder','touch /source/folder/inner.txt','write /source/folder/inner.txt nested']:
                g.command(cmd,'Created directory:' if cmd.startswith('mkdir') else ('Created:' if cmd.startswith('touch') else 'Wrote'))
            g.command('run --read-only dndchk','Restricted drag API checks passed!');g.wait_for('User program exited with code 0.')
            g.command('run --isolated dndchk','Restricted drag API checks passed!');g.wait_for('User program exited with code 0.')
            baseline=int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1]);g.command('cd /source','xinyi:/SOURCE>');g.type('desktop\n')
            def log():return (d/'debug.log').read_text(errors='replace')
            def until(fn,msg):
                end=time.monotonic()+35
                while True:
                    try:
                        if fn():return
                    except AssertionError:pass
                    assert time.monotonic()<end,msg+'\n'+log()[-1800:];time.sleep(.04)
            def tree():return inspect_tree(image,2048)[0]
            until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            def launch():
                start=len(log());g.key('ctrl-esc');g.type('files\n');until(lambda:'Multiwindow: open pid=' in log()[start:],'Files launch');m=re.search(r'Multiwindow: open pid=(\d+) .*client=(\d+)x(\d+) pos=(\d+),(\d+)',log()[start:]);return tuple(map(int,m.groups()))
            start=len(log());g.key('ctrl-esc');g.type('dndchk\n');until(lambda:re.search(r'Desktop: launch pid=(\d+)',log()[start:]),'API helper')
            helper=int(re.search(r'Desktop: launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Scheduler: exit pid={helper} code=0' in log()[start:],'trusted drag ABI checks')
            first=launch();second=launch();pointer=[400,300]
            def move(x,y):
                while pointer!=[x,y]:
                    dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.035)
            def button(down):g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.10)
            def click(x,y):move(x,y);button(True);button(False)
            def shift(down):g.call('input-send-event',{'events':[{'type':'key','data':{'down':down,'key':{'type':'qcode','data':'shift'}}}]});time.sleep(.08)
            x=first[3]+24;y=first[4]+34+88;dx=second[3]+second[1]-20;dy=second[4]+34+240
            # Second Files inherits /SOURCE. Root ends with auto-created DESKTOP; DEST precedes it.
            g.key('backspace');time.sleep(.2);g.key('end');g.key('up');g.key('ret');time.sleep(.3)
            def drag(row,to_x=dx,to_y=dy,cut=False,cancel=False):
                start=log().count('Drag: begin');move(x,y+row*16);button(True);move(x+20,y+row*16);until(lambda:log().count('Drag: begin')>start,'drag did not start')
                if cut:shift(True)
                move(to_x,to_y)
                if cancel:g.key('esc')
                button(False)
                if cut:shift(False)
            # Keep the normal clipboard independent of drag payloads.
            click(x,y);g.key('ctrl-c');drag(0);until(lambda:tree().get('DEST/A.TXT')==b'alpha','cross-window copy');assert tree()['SOURCE/A.TXT']==b'alpha'
            drag(1,cut=True);until(lambda:tree().get('DEST/B.TXT')==b'beta' and 'SOURCE/B.TXT' not in tree(),'Shift move')
            # Folder now occupies row one after B was moved. Drop it onto SUB.
            drag(1,to_y=second[4]+34+88);until(lambda:tree().get('DEST/SUB/FOLDER/INNER.TXT')==b'nested','folder drop target')
            # Preserve a multi-selection when starting a drag from a selected row.
            click(x,y);g.call('input-send-event',{'events':[{'type':'key','data':{'down':True,'key':{'type':'qcode','data':'ctrl'}}}]});click(x,y+16);g.call('input-send-event',{'events':[{'type':'key','data':{'down':False,'key':{'type':'qcode','data':'ctrl'}}}]});time.sleep(.1)
            drag(0,to_y=second[4]+34+104);until(lambda:tree().get('DEST/MULTI/A.TXT')==b'alpha' and tree().get('DEST/MULTI/FOLDER/INNER.TXT')==b'nested','multi-selection drag');click(x,y)
            drag(0,to_x=780,to_y=570,cancel=False);time.sleep(.2);assert 'SOURCE/A.TXT' in tree(),'outside drop removed source'
            drag(0,cancel=True);time.sleep(.2);assert tree()['SOURCE/A.TXT']==b'alpha'
            # Same-folder drop is a no-op. Self-descendant transfers must fail.
            drag(0,to_x=x+80,to_y=y+160);time.sleep(.2)
            drag(1,to_x=x+80,to_y=y+16);time.sleep(.3);assert tree()['SOURCE/FOLDER/INNER.TXT']==b'nested' and 'SOURCE/FOLDER/FOLDER/INNER.TXT' not in tree()
            # Clipboard still contains A after a move drag; conflict uses Paste As.
            click(*task_button(1));g.key('ctrl-v');time.sleep(.25);g.type('clip.txt\n');until(lambda:tree().get('DEST/CLIP.TXT')==b'alpha','clipboard independence')
            # Duplicate drag offers a new name and never overwrites A.
            drag(0);time.sleep(.25);g.type('other.txt\n');until(lambda:tree().get('DEST/OTHER.TXT')==b'alpha','drop collision rename');assert tree()['DEST/A.TXT']==b'alpha'
            drag(0);g.key('esc');time.sleep(.2)
            move(x,y);button(True);move(x+20,y);time.sleep(.2);move(dx,dy);g.call('screendump',{'filename':str(out/'drag-drop.png'),'format':'png'});g.key('alt-f4');button(False)
            # Closing source/target leaves no stuck drag capture.
            until(lambda:f'Scheduler: exit pid={first[0]} code=0' in log(),'close source during drag');g.key('alt-f4');time.sleep(.25)
            click(*SHELL_BUTTON);g.wait_for('Desktop finished.')
            assert int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])==baseline,'drag/drop frame leak'
            g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
        finally:
            g.call('screendump',{'filename':str(out/'drag-drop-last.png'),'format':'png'})
            g.close();shutil.copyfile(d/'debug.log',out/'drag-drop-debug.log')
    print('PASS drag/drop: cross-window copy, Shift move, folder targets, cancellation/outside/no-op/descendant refusal, clipboard preservation, collision rename, close cleanup, frame balance and fsck')

if __name__=='__main__':main()
