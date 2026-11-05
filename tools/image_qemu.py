"""Check pixel-buffer isolation and BMP viewing/associations using disposable disks."""
import argparse,json,re,shutil,struct,subprocess,sys,tempfile,time
from pathlib import Path
from array import array
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def bitmap(width,height,bits=24,top=False):
    stride=(width*(bits//8)+3)&~3;b=bytearray(54+stride*height)
    struct.pack_into('<2sIHHI',b,0,b'BM',len(b),0,0,54)
    struct.pack_into('<IiiHHIIiiII',b,14,40,width,-height if top else height,1,bits,0,stride*height,0,0,0,0)
    for y in range(height):
        for x in range(width):
            if width==2:colour=([(255,0,0),(0,255,0)],[(0,0,255),(255,255,255)])[y][x]
            else:colour=(255,0,0) if x<width//2 else (0,255,0)
            at=54+(y if top else height-1-y)*stride+x*(bits//8)
            b[at:at+3]=bytes(reversed(colour))
            if bits==32:b[at+3]=77
    return bytes(b)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');a=p.parse_args()
    out=ROOT/'target/images';out.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-',dir=out) as temporary:
        d=Path(temporary);image=d/'system.img';shutil.copyfile(a.disk,image)
        helper=d/'helper';(helper/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/pixel_check/main.rs'),'--format','elf','--output',str(helper/'apps/pxchk.elf')],check=True)
        (helper/'apps/pxchk.app').write_text('XINYI-APP 1\nexec=PXCHK.ELF\ntitle=Pixel boundary check\ndescription=Private test fixture\nicon=application\naccess=read-only\nhandles=\nversion=1\n')
        (helper/'applications.json').write_text(json.dumps([{'file':'PXCHK.ELF','manifest':'PXCHK.APP'}]));seed_apps(image,helper,root_name='TOOLS');seed_apps(image,helper)
        fixture=d/'fixture';(fixture/'apps').mkdir(parents=True)
        samples={'A.BMP':bitmap(2,2),'WIDE.BMP':bitmap(1024,8),'TOP.BMP':bitmap(2,2,32,True),'BAD.BMP':b'BM'+b'\0'*68}
        for name,body in samples.items():(fixture/'apps'/name.lower()).write_bytes(body)
        (fixture/'applications.json').write_text(json.dumps([{'file':name} for name in samples]));seed_apps(image,fixture,root_name='DESKTOP')
        subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(d/'font.exe')],check=True)
        font=subprocess.check_output([str(d/'font.exe')]);glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
        qemu=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
        g=Guest(qemu,None,d,disk=image,extra_args=['-vga',a.vga])
        try:
            def log():return (d/'debug.log').read_text()
            def until(test,message):
                deadline=time.monotonic()+45
                while not test():
                    assert time.monotonic()<deadline,message+'\n'+log()[-2000:];time.sleep(.04)
            def memory(address,n):
                f=d/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {address:#x} {n} "{f.as_posix()}"'});return f.read_bytes()
            def pixel(address,width,x,y):return int.from_bytes(memory(address+(y*width+x)*4,4),'little')
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            baseline=frames()
            def pixels_ok(address):
                return pixel(address,640,639,543)==0xabcdef and [pixel(address,640,x,y) for x,y in [(10,10),(11,10),(10,11),(11,11)]]==[0xff0000,0x00ff00,0x0000ff,0xffffff]
            for profile in ['--read-only','--isolated']:
                start=len(log());g.type('run '+profile+' /tools/pxchk.elf\n')
                pattern=r'Window: opened handle=\d+ client=640x544 surface=([0-9a-f]+)'
                until(lambda:re.search(pattern,log()[start:]),'Foreground pixel window')
                address=int(re.search(pattern,log()[start:])[1],16);until(lambda:pixels_ok(address),'Pixel bounds/atomic rejection or tall foreground rendering')
                g.key('esc');g.wait_for('User program exited with code 0.');assert frames()==baseline
            g.command('cd /desktop','xinyi:/DESKTOP>');start=len(log());g.type('desktop\n')
            until(lambda:'Desktop scheduler: ready' in log()[start:],'Desktop')
            def launch(name):
                start=len(log());g.key('ctrl-esc');g.type(name+'\n')
                until(lambda:re.search(r'Multiwindow: open pid=(\d+)',log()[start:]),'Launch '+name)
                return int(re.search(r'Multiwindow: open pid=(\d+)',log()[start:])[1])
            def current(pid):
                m=re.findall(r'Multiwindow: (?:open|resized) pid='+str(pid)+r' (?:handle=\d+ )?surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log())[-1]
                return int(m[0],16),*map(int,m[1:])
            def close(pid):g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={pid} code=0' in log(),'Image exit')
            counter=launch('counter');probe=launch('pxchk');probe_address=current(probe)[0]
            until(lambda:pixels_ok(probe_address),'Desktop pixel copy or foreign-window refusal')
            close(probe);close(counter)
            files=launch('files')
            def open_selected():
                start=len(log());g.key('ret');pattern=r'Desktop: (?:terminal )?launch pid=(\d+) (?:path|name)=/APPS/IMAGE/IMAGE.ELF';until(lambda:re.search(pattern,log()[start:]),'BMP association did not launch Image viewer')
                pid=int(re.search(pattern,log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log()[start:],'Image window');return pid
            def check_small(pid):
                def ready():
                    address,w,h,_,_=current(pid);x=8+(w-16-2)//2;y=64+(h-96-2)//2
                    return [pixel(address,w,x+dx,y+dy) for dx,dy in [(0,0),(1,0),(0,1),(1,1)]]==[0xff0000,0x00ff00,0x0000ff,0xffffff]
                until(ready,'BMP orientation or RGB channels incorrect')
                address,w,h,wx,wy=current(pid);x=wx+8+8+(w-16-2)//2;y=wy+34+64+(h-96-2)//2
                if a.vga=='std':
                    device=next(dev for dev in g.call('query-pci')[0]['devices'] if dev.get('id',{}).get('vendor')==0x1234)
                    base=next(region['address'] for region in device['regions'] if region.get('bar')==0);pitch=3200;size=4
                else:
                    mode=re.findall(r'BIOS VBE mode [0-9a-f]+ base=([0-9a-f]+) pitch=(\d+) bpp=(\d+)',log())[-1]
                    base,pitch,size=int(mode[0],16),int(mode[1]),int(mode[2])//8
                def screen_colour(px,py):
                    value=int.from_bytes(memory(base+py*pitch+px*size,size),'little')
                    if size==2:return ((value>>11&31)*255//31)<<16|((value>>5&63)*255//63)<<8|(value&31)*255//31
                    return value&0xffffff
                until(lambda:[screen_colour(x+dx,y+dy) for dx,dy in [(0,0),(1,0),(0,1),(1,1)]]==[0xff0000,0x00ff00,0x0000ff,0xffffff],'Presented framebuffer colours differ from the client surface')
            g.key('home');viewer=open_selected();check_small(viewer);close(viewer)
            g.key('home');g.key('down');viewer=open_selected();address,w,h,wx,wy=current(viewer)
            until(lambda:pixel(address,w,8,214)==0xff0000 and pixel(address,w,631,214)==0x00ff00,'Fit scaling failed')
            g.key('1');until(lambda:pixel(address,w,508,212)==0xff0000,'Actual-size mode failed');g.key('right');until(lambda:pixel(address,w,508,212)==0x00ff00,'Actual-size panning failed');g.key('f')
            pointer=[400,300]
            def move(x,y):
                while pointer!=[x,y]:
                    dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
            def button(down):g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
            move(wx+8+180,wy+34+20);button(True);move(wx+8+350,wy+34+45);button(False)
            assert pixel(address,w,631,214)==0x00ff00,'Cancelled toolbar press changed Fit mode'
            move(wx+8+180,wy+34+20);button(True);button(False)
            until(lambda:pixel(address,w,508,212)==0xff0000,'1:1 toolbar button failed')
            move(wx+8+100,wy+34+20);button(True);button(False)
            until(lambda:pixel(address,w,631,214)==0x00ff00,'Fit toolbar button failed')
            start=len(log());move(wx+100,wy+15);button(True);button(False);button(True);button(False)
            until(lambda:'maximized=true' in log()[start:],'Image maximise')
            address,w,h,wx,wy=current(viewer);until(lambda:pixel(address,w,8,269)==0xff0000 and pixel(address,w,783,269)==0x00ff00,'Maximised image did not reflow')
            start=len(log());move(wx+100,wy+15);button(True);button(False);button(True);button(False);until(lambda:'maximized=false' in log()[start:],'Image restore')
            move(170,60);button(True);move(190,60);move(480,300);button(False);check_small(viewer);close(viewer)
            g.key('home');g.key('down');g.key('down');viewer=open_selected();check_small(viewer);close(viewer)
            g.key('end');viewer=open_selected();address,w,h,_,_=current(viewer)
            def message():
                data=array('I');data.frombytes(memory(address+(h-16)*w*4,w*16*4))
                return ''.join(glyphs.get(bytes(data[dy*w+8+col*8+dx]==0xffffff for dy in range(16) for dx in range(8)),'?') for col in range(40))
            until(lambda:'Unsupported or damaged BMP' in message(),'Malformed BMP was not rejected');close(viewer);close(files)
            start=len(log());move(170,60);button(True);button(False)
            pattern=r'Desktop: (?:terminal )?launch pid=(\d+) (?:path|name)=/APPS/IMAGE/IMAGE.ELF'
            until(lambda:re.search(pattern,log()[start:]),'Desktop BMP association')
            viewer=int(re.search(pattern,log()[start:])[1]);until(lambda:f'Multiwindow: open pid={viewer} ' in log()[start:],'Desktop image window');check_small(viewer);close(viewer)
            g.key('esc');g.key('esc');g.wait_for('Desktop finished.');assert frames()==baseline,'Image buffers or tiles leaked frames'
            for name,body in samples.items():assert inspect_tree(image,2048)[0]['DESKTOP/'+name]==body
            assert 'Scheduler: fault' not in log() and 'Xinyi panic:' not in log()
            print('PASS images: pixel bounds/foreign handles/atomic rejection, read-only and isolated foreground calls, tall surfaces, BMP RGB/orientation, fit/pan/maximise, file/desktop associations, drops, malformed rejection, source preservation and frame balance',flush=True)
        finally:g.close();shutil.copyfile(d/'debug.log',out/('debug-'+a.vga+'.log'));shutil.copyfile(d/'qemu.stderr',out/('qemu-'+a.vga+'.stderr'))

if __name__=='__main__':main()
