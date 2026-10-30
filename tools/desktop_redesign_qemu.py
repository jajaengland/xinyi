"""Exercise desktop shortcuts, search, clock, sound popup and compact task buttons."""
import argparse,json,re,shutil,subprocess,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from folders_qemu import inspect_tree
from desktop_controls import APPS_BUTTON,SHELL_BUTTON,task_button

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(ROOT/'target/font-dump.exe')],check=True)
    font=subprocess.check_output([str(ROOT/'target/font-dump.exe')])
    expected=set();advance=0
    for b in b'12:34':
        rows=font[b*16:b*16+16];bits=0
        for v in rows:bits|=v
        left=min(i for i in range(8) if bits&(0x80>>i));right=max(i for i in range(8) if bits&(0x80>>i))+1
        for y,row in enumerate(rows):
            for x in range(left,right):
                if row&(0x80>>x):expected.add((advance+x-left,y))
        advance+=7 if b in b'0123456789' else right-left+1
    with tempfile.TemporaryDirectory(prefix='desktop-redesign-',dir=out) as tmp:
        d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
        extra=['-vga',a.vga,'-rtc','base=2026-10-29T12:34:00,clock=vm']
        if a.usb:extra+=['-drive',f'if=none,id=stick,file={image.as_posix()},format=raw','-device','piix3-usb-uhci,id=uhci','-device','usb-storage,bus=uhci.0,drive=stick,bootindex=1']
        g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,attach_disk=not a.usb,extra_args=extra)
        try:
            def log():return (d/'debug.log').read_text(errors='replace')
            def until(fn,msg):
                end=time.monotonic()+30
                while True:
                    try:
                        if fn():return
                    except AssertionError:pass # A live snapshot can catch a FAT mirror write.
                    assert time.monotonic()<end,msg+'\n'+log()[-1600:];time.sleep(.04)
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            baseline=frames();g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            if a.vga=='std':
                dev=next(v for v in g.call('query-pci')[0]['devices'] if v.get('id',{}).get('vendor')==0x1234);base=next(v['address'] for v in dev['regions'] if v['bar']==0);pitch=3200;size=4
            else:
                m=re.findall(r'BIOS VBE mode [0-9a-f]+ base=([0-9a-f]+) pitch=(\d+) bpp=(\d+)',log())[-1];base,pitch,size=int(m[0],16),int(m[1]),int(m[2])//8
            def mem(address,n):
                f=d/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {address:#x} {n} "{f.as_posix()}"'});return f.read_bytes()
            def clock():
                data=mem(base+558*pitch,pitch*16);white=0xffff if size==2 else 0xffffff
                return {(x,y) for y in range(16) for x in range(72) if int.from_bytes(data[y*pitch+(640+x)*size:y*pitch+(641+x)*size],'little')==white}
            until(lambda:clock()==expected,'RTC clock did not render 12:34')
            pointer=[400,300]
            def click(x,y):
                while pointer!=[x,y]:
                    dx=max(-80,min(80,x-pointer[0]));dy=max(-80,min(80,y-pointer[1]))
                    g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.07)
            def launched(name,start):
                until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name,log()[start:]),'launch '+name)
                pid=int(re.search(r'Desktop: launch pid=(\d+) ',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log(),'window '+name);return pid
            def search(name):
                start=len(log());g.key('ctrl-esc');g.type(name.lower()+'\n');return launched(name,start)
            start=len(log());click(68,64);files=launched('FILES',start)
            before=log().count('Desktop: launch pid=');click(68,168);time.sleep(.2)
            assert log().count('Desktop: launch pid=')==before,'Shortcut clicked through an application'
            g.key('ctrl-esc');g.type('zzzz\n');time.sleep(.15);assert log().count('Desktop: launch pid=')==before,'No-result search launched a stale item'
            terminal=search('TERMINAL');counter=search('COUNTER')
            for target,expected_volume in [((704,512),0),((650,512),10)]:
                click(598,576);click(*target)
                start=len(log());click(*task_button(1));time.sleep(.12)
                assert f'Multiwindow: restored pid={terminal}' not in log()[start:],'Popup outside click passed through'
                click(*task_button(1));g.type('sounds save\n')
                until(lambda:inspect_tree(image,2048)[0].get('SOUND.CFG')==b'SND1'+expected_volume.to_bytes(4,'little'),'Sound popup did not change global volume')
            settings=search('SETTINGS');g.key('ctrl-esc')
            g.call('screendump',{'filename':str(out/f'desktop-redesign-{a.vga}.png'),'format':'png'})
            g.key('esc');g.key('alt-f4');until(lambda:f'Scheduler: exit pid={settings} code=0' in log(),'Settings close')
            click(*task_button(1));g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={terminal} code=0' in log(),'Terminal close')
            until(lambda:f'Taskbar: pids={files},{counter},0,0' in log(),'Taskbar did not compact after removing a middle task')
            click(*task_button(1));g.key('q');until(lambda:f'Scheduler: exit pid={counter} code=0' in log(),'Compacted task button did not focus Counter')
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={files} code=0' in log(),'Files close')
            click(*SHELL_BUTTON);g.wait_for('Desktop finished.');assert frames()==baseline,'Desktop leaked private frames'
        finally:
            g.close();shutil.copyfile(d/'debug.log',out/f'desktop-redesign-{a.vga}-debug.log')
    print('PASS redesigned desktop: exact RTC glyphs, search/no-results, shortcuts/occlusion, popup capture, real volume persistence, compact task focus and frame balance ('+a.vga+')')

if __name__=='__main__':main()
