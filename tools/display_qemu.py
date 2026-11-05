"""Check saved desktop resolutions, full-screen geometry and hardware fallback."""
import argparse,re,shutil,struct,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from folders_qemu import inspect_tree

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--mode',default='1024x768',choices=['1024x768','1280x1024']);a=p.parse_args();width,height=map(int,a.mode.split('x'));work=(height-56)//16*16;client_width=(width-8)//8*8;client_height=(work-32)//16*16
    out=ROOT/'target/display';out.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-',dir=out) as temporary:
        d=Path(temporary);image=d/'system.img';shutil.copyfile(a.disk,image)
        qemu=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
        def guest(desktop=False):return Guest(qemu,None,d,disk=image,start_desktop=desktop,extra_args=['-vga',a.vga])
        g=guest()
        try:
            g.command('display '+a.mode,'Saved '+a.mode)
            assert inspect_tree(image,2048)[0]['DISPLAY.CFG']==b'XDP1'+struct.pack('<HH',width,height)
        finally:g.close()
        g=guest(True)
        try:
            def log():return (d/'debug.log').read_text()
            def until(test,message):
                deadline=time.monotonic()+45
                while not test():
                    assert time.monotonic()<deadline,message+'\n'+log()[-1800:];time.sleep(.05)
            def size():
                path=d/'screen.png';g.call('screendump',{'filename':str(path),'format':'png'});return struct.unpack('>II',path.read_bytes()[16:24])
            def colour(x,y):
                path=d/'pixels.ppm';g.call('screendump',{'filename':str(path)})
                with path.open('rb') as stream:
                    assert stream.readline().strip()==b'P6'
                    line=stream.readline()
                    while line.startswith(b'#'):line=stream.readline()
                    width,height=map(int,line.split());assert stream.readline().strip()==b'255'
                    pixels=stream.read();at=(y*width+x)*3;return tuple(pixels[at:at+3])
            assert size()==(width,height),'Saved resolution did not reach the real framebuffer'
            assert all(abs(actual-expected)<=7 for actual,expected in zip(colour(width-4,height-8),(0x14,0x26,0x37))),'Taskbar did not fill the physical lower-right edge'
            g.key('esc');g.wait_for('xinyi:/')
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            baseline=frames();start=len(log());g.type('desktop\n')
            until(lambda:'Desktop scheduler: ready' in log()[start:],'Desktop restart')
            g.key('ctrl-esc');g.type('files\n')
            until(lambda:re.search(r'Multiwindow: open pid=(\d+)',log()[start:]),'Files startup')
            m=re.search(r'Multiwindow: open pid=(\d+) .*pos=(\d+),(\d+)',log()[start:]);pid,wx,wy=map(int,m.groups())
            pointer=[400,300]
            def move(x,y):
                while pointer!=[x,y]:
                    dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
            def button(down):g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
            def click(x,y):move(x,y);button(True);button(False)
            move(wx+100,wy+15);button(True);move(width-24,wy+15);button(False)
            until(lambda:colour(width-180,400)!=(255,255,255) and colour(width-30,400)==(255,255,255),'Window drag remained constrained to the old screen width')
            start=len(log());move(width-24,wy+15);button(True);button(False);button(True);button(False)
            until(lambda:re.search(r'Multiwindow: resized pid='+str(pid)+rf' surface=[0-9a-f]+ client={client_width}x{client_height} pos=0,0 maximized=true',log()[start:]),'Files did not fill the selected work area')
            assert size()==(width,height)
            until(lambda:colour(width-30,work-80)==(255,255,255),'Client pixels beyond the old 800-pixel boundary use the wrong stride')
            # Close through the real far-right title button, then use the new far-right taskbar.
            click(width-16,12);until(lambda:f'Scheduler: exit pid={pid} code=0' in log(),'Far-right close button missed')
            click(width-44,work+32);g.wait_for('Desktop finished.');assert frames()==baseline,'High-resolution surfaces leaked frames'
            g.command('display 1920x1080','Unsupported size')
            assert inspect_tree(image,2048)[0]['DISPLAY.CFG']==b'XDP1'+struct.pack('<HH',width,height),'Invalid preference changed disk'
            g.command('display 1280x720','Saved 1280x720')
            shutil.copyfile(d/'debug.log',out/('geometry-'+a.vga+'-'+a.mode+'.log'))
        finally:g.close()
        g=guest(True)
        try:
            path=d/'last.png';g.call('screendump',{'filename':str(path),'format':'png'});dimensions=struct.unpack('>II',path.read_bytes()[16:24])
            if a.vga=='std':assert dimensions==(1280,720),dimensions
            else:
                # Cirrus firmware normally lacks 1280x720; either a supported mode or the explicit fallback is valid.
                assert dimensions in [(1280,720),(800,600)],dimensions
                if dimensions==(800,600):assert 'falling back to 800x600' in (d/'debug.log').read_text()
            shutil.copyfile(path,out/('resolution-'+a.vga+'-'+a.mode+'.png'))
            assert 'Xinyi panic:' not in (d/'debug.log').read_text()
            print(f'PASS display ({a.vga}): saved/cold-boot {a.mode}, actual framebuffer size, {client_width}x{client_height} maximise, far-edge pointer/title/taskbar input, restart/frame balance, invalid preference preservation and 1280x720/fallback {dimensions}',flush=True)
        finally:g.close();shutil.copyfile(d/'debug.log',out/('final-'+a.vga+'-'+a.mode+'.log'))

if __name__=='__main__':main()
