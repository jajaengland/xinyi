"""Verify real timer preemption, per-task windows, sleep/wait, faults and cleanup."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from array import array
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel',type=Path,default=ROOT/'target/i686-unknown-none/debug/rust-os')
    parser.add_argument('--package',type=Path,default=ROOT/'target/multi-package')
    parser.add_argument('--disk',type=Path)
    parser.add_argument('--vga',default='std')
    parser.add_argument('--usb',action='store_true')
    args=parser.parse_args();qemu=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
    output=ROOT/'target/smoke';output.mkdir(parents=True,exist_ok=True)
    subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(ROOT/'target/font-dump.exe')],check=True)
    font=subprocess.check_output([str(ROOT/'target/font-dump.exe')]);glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
    with tempfile.TemporaryDirectory(prefix='multitask-',dir=output) as temporary:
        directory=Path(temporary);image=directory/'system.img'
        if args.disk:shutil.copyfile(args.disk,image)
        else:
            with image.open('xb') as stream:stream.truncate(16*1024*1024)
            guest=Guest(qemu,args.kernel,directory,disk=image)
            try:guest.command('install','To install, enter: install confirm');guest.type('install confirm\n');guest.wait_for('Installation complete:',timeout=45)
            finally:guest.close()
            seed_apps(image,args.package)
        extra=['-vga',args.vga]
        if args.usb:extra+=['-drive',f'if=none,id=stick,file={image.as_posix()},format=raw','-device','piix3-usb-uhci,id=uhci','-device','usb-storage,bus=uhci.0,drive=stick,bootindex=1']
        guest=Guest(qemu,None,directory,disk=image,attach_disk=not args.usb,extra_args=extra)
        try:
            def log():return (directory/'debug.log').read_text()
            def until(test,message):
                deadline=time.monotonic()+30
                while not test():assert time.monotonic()<deadline,message+'\n'+log()[-2200:];time.sleep(.03)
            def frames():return int(re.search(r'Free now: +(\d+)',guest.command('frames','Free now:'))[1])
            baseline=frames();offset=len(log());guest.command('multitask check','Multitasking demo finished.',);section=log()[offset:]
            exits=re.findall(r'Scheduler: exit pid=(\d+) code=(\d+) preemptions=(\d+)',section)
            assert len(exits)==2 and all(code=='0' for _,code,_ in exits),section
            assert all(int(p)>0 for _,_,p in exits),'Register checks were not timer preempted'
            assert frames()==baseline,'Register-check tasks leaked frames'
            def memory(address,length):
                path=directory/'memory.bin';guest.call('human-monitor-command',{'command-line':f'pmemsave {address:#x} {length} "{path.as_posix()}"'});return path.read_bytes()
            def windows(offset):
                matches=re.findall(r'Multiwindow: open pid=(\d+) handle=(\d+) surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log()[offset:])
                return sorted((int(pid),int(surface,16),int(w),int(h),int(x),int(y)) for pid,handle,surface,w,h,x,y in matches)
            def counter(window):
                pid,surface,width,height,x,y=window;data=array('I');data.frombytes(memory(surface+40*width*4,width*16*4))
                text=''.join(glyphs.get(bytes(data[row*width+8+col*8+dx]==0 for row in range(16) for dx in range(8)),'?') for col in range(35))
                match=re.search(r'Count: (\d+)',text);return int(match[1]) if match else -1
            offset=len(log());guest.type('multitask\n');until(lambda:len(windows(offset))==2,'Counter windows did not open');wins=windows(offset)
            until(lambda:all(counter(w)>=3 for w in wins),'Both counters did not advance')
            first=[counter(w) for w in wins];time.sleep(.5);assert all(counter(w)>value for w,value in zip(wins,first))
            guest.call('screendump',{'filename':str(output/f'multitask-{args.vga}.png'),'format':'png'})
            pos=[400,300]
            def move(x,y):
                while pos!=[x,y]:
                    dx=max(-80,min(80,x-pos[0]));dy=max(-80,min(80,y-pos[1]));guest.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pos[0]+=dx;pos[1]+=dy;time.sleep(.04)
            def button(down):guest.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
            def click(x,y):move(x,y);button(True);button(False)
            # Drag A across B while B continues, then close A normally.
            bx,by=wins[1][4:6]
            move(wins[0][4]+44,wins[0][5]+15);button(True);before=counter(wins[1]);move(bx+22,by+40);button(False);assert counter(wins[1])>before
            # Raising by a client click must repair covered pixels immediately.
            if args.vga=='std':
                dev=next(d for d in guest.call('query-pci')[0]['devices'] if d.get('id',{}).get('vendor')==0x1234)
                base=next(r['address'] for r in dev['regions'] if r['bar']==0);pitch=3200;size=4
            else:
                mode=re.findall(r'BIOS VBE mode [0-9a-f]+ base=([0-9a-f]+) pitch=(\d+) bpp=(\d+)',log())[-1]
                base=int(mode[0],16);pitch=int(mode[1]);size=int(mode[2])//8
            def pixel(x,y):return int.from_bytes(memory(base+y*pitch+x*size,size),'little')
            def native(rgb):return ((rgb>>19&31)<<11|(rgb>>10&63)<<5|(rgb>>3&31)) if size==2 else rgb
            click(bx+42,by+15);until(lambda:pixel(bx+292,by+130) in [native(0x7755aa),native(0xdddddd)],'Raising B did not repair overlap')
            click(bx-10,by+80);until(lambda:pixel(bx+292,by+130)==native(0xf4f4f4),'Client click did not raise A')
            click(bx+296,by+40);until(lambda:f'Scheduler: exit pid={wins[0][0]} code=0' in log()[offset:],'Close did not exit only A')
            before=counter(wins[1]);time.sleep(.4);assert counter(wins[1])>before
            guest.key('esc');guest.wait_for('Multitasking demo finished.');assert frames()==baseline
            # A CPU hog makes no calls, but B and the window manager must run.
            offset=len(log());guest.type('multitask hog\n');until(lambda:len(windows(offset))==2,'Hog prevented B from opening');wins=windows(offset)
            until(lambda:counter(wins[1])>=3,'CPU hog starved B');pos[:]=[400,300];click(wins[0][4]+44,wins[0][5]+56);guest.key('ctrl-alt-k');until(lambda:f'forced exit pid={wins[0][0]}' in log()[offset:],'Force-close did not stop hog')
            before=counter(wins[1]);time.sleep(.4);assert counter(wins[1])>before;guest.key('esc');guest.wait_for('Multitasking demo finished.');assert frames()==baseline
            # Waiting and crashing jobs cannot block the other task.
            for mode in ['wait','fault']:
                offset=len(log());guest.type('multitask '+mode+'\n');until(lambda:len(windows(offset))==2,'Missing '+mode+' windows');wins=windows(offset)
                until(lambda:counter(wins[1])>=6,'Other counter blocked by '+mode)
                if mode=='wait':
                    pos[:]=[400,300];click(wins[0][4]+44,wins[0][5]+56);guest.key('q');until(lambda:f'exit pid={wins[0][0]} code=0' in log()[offset:],'Waiting app did not receive its focused key')
                else:until(lambda:f'fault pid={wins[0][0]} vector=6' in log()[offset:],'Expected isolated user fault')
                guest.key('esc');guest.wait_for('Multitasking demo finished.');assert frames()==baseline
            guest.command('echo scheduler survived','\nscheduler survived\n')
            print(f'PASS multitasking ({args.vga}): timer preemption/GPR/DF/x87 isolation, separate address spaces/stacks, independent counters/windows, dragging, close, CPU hog, focus/wait wakeup, fault recovery and frame balance')
        finally:
            guest.call('screendump',{'filename':str(output/f'multitask-last-{args.vga}.png'),'format':'png'});guest.close();shutil.copyfile(directory/'debug.log',output/f'multitask-{args.vga}-debug.log');shutil.copyfile(directory/'qemu.stderr',output/f'multitask-{args.vga}-stderr.log')


if __name__=='__main__':main()
