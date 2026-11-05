"""Check Applications paging, launches, folder inspection and confirmed removal."""
import argparse,json,re,shutil,subprocess,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--package',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/app-manager';out.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-',dir=out) as temporary:
        d=Path(temporary);image=d/'system.img';shutil.copyfile(a.disk,image)
        pkg=d/'fixture';(pkg/'apps/zztest').mkdir(parents=True);shutil.copyfile(a.package/'apps/counter/counter.bin',pkg/'apps/zztest/zztest.bin')
        (pkg/'apps/zztest/zztest.app').write_text('XINYI-APP 1\nexec=ZZTEST.BIN\ntitle=Test bundle\ndescription=Private manager test\nicon=application\naccess=trusted\nhandles=\nversion=7\n')
        (pkg/'applications.json').write_text(json.dumps([{'directory':'ZZTEST','file':'ZZTEST.BIN','manifest':'ZZTEST.APP'}]));seed_apps(image,pkg)
        subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(d/'font.exe')],check=True);font=subprocess.check_output([str(d/'font.exe')])
        qemu=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe';g=Guest(qemu,None,d,disk=image)
        try:
            def log():return (d/'debug.log').read_text()
            def until(check,message):
                deadline=time.monotonic()+45
                while True:
                    try:
                        if check():return
                    except AssertionError:pass
                    assert time.monotonic()<deadline,message+'\n'+log()[-1500:];time.sleep(.05)
            def tree():return inspect_tree(image,2048)[0]
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            g.command('touch keep.txt','Created:');g.command('write keep.txt personal document','Wrote');baseline=frames()
            start=len(log());g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log()[start:],'Desktop')
            g.key('ctrl-esc');g.type('appmgr\n');until(lambda:re.search(r'Multiwindow: open pid=(\d+)',log()[start:]),'Applications launch')
            m=re.search(r'Multiwindow: open pid=(\d+).*surface=([0-9a-f]+).*pos=(\d+),(\d+)',log()[start:]);pid=int(m[1]);surface=int(m[2],16);wx,wy=int(m[3]),int(m[4])
            def text(address,width,x,y,label):
                path=d/'pixels.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {address+y*width*4:#x} {width*16*4} "{path.as_posix()}"'});data=path.read_bytes()
                for i,byte in enumerate(label):
                    for row in range(16):
                        for col in range(8):
                            ink=int.from_bytes(data[(row*width+x+i*8+col)*4:(row*width+x+i*8+col+1)*4],'little')==0
                            if ink!=bool(font[byte*16+row]&(128>>col)):return False
                return True
            until(lambda:text(surface,640,300,112,b'APPMGR'),'Initial selection/metadata')
            g.key('delete');assert 'APPS/APPMGR/APPMGR.ELF' in tree(),'Self removal was allowed'
            pointer=[400,300]
            def click(x,y):
                while pointer!=[x,y]:
                    dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
            click(wx+8+200,wy+34+356);until(lambda:text(surface,640,300,112,b'VIEWER'),'Next page control')
            g.key('end');until(lambda:text(surface,640,300,112,b'ZZTEST') and text(surface,640,380,136,b'7'),'End/version')
            start=len(log());g.key('ret');until(lambda:re.search(r'Desktop: (?:terminal )?launch pid=(\d+) (?:path|name)=/APPS/ZZTEST/ZZTEST.BIN',log()[start:]),'Launch selected app')
            child=int(re.search(r'Desktop: (?:terminal )?launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={child}' in log(),'Counter window')
            g.key('alt-tab');g.key('delete');g.key('n');assert 'APPS/ZZTEST/ZZTEST.BIN' in tree(),'Cancel removed a bundle'
            g.key('delete');g.key('y');time.sleep(.3);assert 'APPS/ZZTEST/ZZTEST.BIN' in tree(),'Running bundle was removed'
            g.key('alt-tab');g.key('q');until(lambda:f'Scheduler: exit pid={child}' in log(),'Counter close')
            g.key('delete');g.key('y');until(lambda:'APPS/ZZTEST/ZZTEST.BIN' not in tree(),'Confirmed removal')
            assert tree()['KEEP.TXT']==b'personal document','Removal changed a document'
            g.key('end');until(lambda:text(surface,640,300,112,b'VIEWER'),'Selection after removal')
            start=len(log());click(wx+8+160,wy+34+54);until(lambda:re.search(r'Multiwindow: open pid=(\d+)',log()[start:]),'Open folder')
            m=re.search(r'Multiwindow: open pid=(\d+).*surface=([0-9a-f]+)',log()[start:]);folder_pid=int(m[1]);folder_surface=int(m[2],16)
            # The Files address field proves the exact folder argument, not just a launch.
            def folder_address():
                path=d/'address.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {folder_surface+40*640*4:#x} {640*16*4} "{path.as_posix()}"'});data=path.read_bytes()
                for i,byte in enumerate(b'/APPS/VIEWER'):
                    for row in range(16):
                        for col in range(8):
                            if (int.from_bytes(data[(row*640+224+i*8+col)*4:(row*640+224+i*8+col+1)*4],'little')==0x20384d)!=bool(font[byte*16+row]&(128>>col)):return False
                return True
            until(folder_address,'Files opened the wrong app folder')
            g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={folder_pid}' in log(),'Files close');g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={pid}' in log(),'Manager close')
            g.key('esc');g.wait_for('Desktop finished.');assert frames()==baseline,'Manager leaked frames'
            g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
            print('PASS Applications: metadata/version, page controls, launch, exact folder, self/running refusal, removal/cancel, document preservation, cleanup and fsck')
        finally:g.close();shutil.copyfile(d/'debug.log',out/'debug.log')

if __name__=='__main__':main()
