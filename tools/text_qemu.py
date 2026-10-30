"""Verify proportional text ABI pixels, validation, reflow, maximize and legacy compatibility."""
from desktop_controls import APPS_BUTTON,SHELL_BUTTON,app_card,task_button
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/text_reference.rs'),'-o',str(ROOT/'target/text-reference.exe')],check=True)
    expected=subprocess.check_output([str(ROOT/'target/text-reference.exe')])
    with tempfile.TemporaryDirectory(prefix='text-',dir=out) as tmp:
        d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image);extra=d/'extra';(extra/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/text_check/main.rs'),'--format','elf','--output',str(extra/'apps/textchk.elf')],check=True)
        (extra/'applications.json').write_text(json.dumps([{'name':'textchk','file':'TEXTCHK.ELF'}]));seed_apps(image,extra)
        g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,extra_args=['-vga',a.vga])
        try:
            def log():return (d/'debug.log').read_text(errors='replace')
            def until(fn,msg):
                end=time.monotonic()+30
                while not fn():
                    assert time.monotonic()<end,msg+'\n'+log()[-2000:];time.sleep(.04)
            def mem(address,n):
                f=d/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {address:#x} {n} "{f.as_posix()}"'});return f.read_bytes()
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            baseline=frames();g.type('run --isolated textchk\n')
            until(lambda:'Window: opened handle=' in log(),'foreground validation window')
            address=int(re.search(r'Window: opened handle=\d+ client=320x160 surface=([0-9a-f]+)',log())[1],16)
            until(lambda:mem(address,len(expected))==expected,'foreground text pixels or atomic invalid-batch rejection')
            g.key('esc');g.wait_for('User program exited with code 0.');assert frames()==baseline
            g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            names=sorted(k.split('/')[-1][:-4] for k in inspect_tree(image,2048)[0] if k.startswith('APPS/') and k.endswith(('.BIN','.ELF')))
            def launch(name):
                start=len(log());g.key('ctrl-esc');g.key('home')
                for _ in range(names.index(name)):g.key('down')
                g.key('ret');until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name,log()[start:]),'launch '+name)
                pid=int(re.search(r'Desktop: launch pid=(\d+) ',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log(),'open '+name);return pid
            def current(pid):
                m=re.findall(r'Multiwindow: (?:open|resized) pid='+str(pid)+r' (?:handle=\d+ )?surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log())[-1]
                return (int(m[0],16),*(int(v) for v in m[1:]))
            counter=launch('COUNTER');check=launch('TEXTCHK');address=current(check)[0]
            until(lambda:mem(address,len(expected))==expected,'desktop text pixels or atomic validation')
            g.key('esc');until(lambda:f'Scheduler: exit pid={check} code=0' in log(),'desktop text-check exit')
            fonts=launch('FONTS')
            def pixel(x,y):
                addr,w,_,_,_=current(fonts);return int.from_bytes(mem(addr+(y*w+x)*4,4),'little')
            until(lambda:pixel(264,161)==0x3273bb,'initial wrap boundary')
            addr,cw,_,_,_=current(counter);before=mem(addr+cw*92*4,cw*18*4)
            g.key('spc');until(lambda:pixel(264,161)==0xffffff and pixel(624,161)==0x3273bb,'wide reflow')
            until(lambda:mem(addr+cw*92*4,cw*18*4)!=before,'Counter stopped while text reflowed')
            position=[400,300]
            def click(x,y):
                while position!=[x,y]:
                    dx=max(-80,min(80,x-position[0]));dy=max(-80,min(80,y-position[1]))
                    g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});position[0]+=dx;position[1]+=dy;time.sleep(.02)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.07)
            _,_,_,x,y=current(fonts);click(x+60,y+15);click(x+60,y+15)
            until(lambda:current(fonts)[1:3]==(792,512),'maximize acknowledgement')
            until(lambda:pixel(776,161)==0x3273bb,'maximized full-width wrap box')
            g.call('screendump',{'filename':str(out/f'fonts-{a.vga}.png'),'format':'png'})
            _,_,_,x,y=current(fonts);click(x+60,y+15);click(x+60,y+15)
            until(lambda:current(fonts)[1:3]==(640,432),'restore acknowledgement')
            until(lambda:pixel(624,161)==0x3273bb,'restored wrap box')
            g.key('esc');until(lambda:f'Scheduler: exit pid={fonts} code=0' in log(),'Fonts close');g.key('q');until(lambda:f'Scheduler: exit pid={counter} code=0' in log(),'Counter close')
            click(*SHELL_BUTTON);g.wait_for('Desktop finished.');assert frames()==baseline,'text surfaces leaked frames'
        finally:g.close();shutil.copyfile(d/'debug.log',out/f'text-{a.vga}-debug.log')
    print('PASS text: actual foreground/desktop pixels, invalid-pointer/overflow/ASCII/batch validation, fixed-cell compatibility, live reflow, maximize/restore and frame balance ('+a.vga+')')

if __name__=='__main__':main()
