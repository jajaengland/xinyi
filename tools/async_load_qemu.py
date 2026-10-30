"""Throttled real reads prove GUI progress, cancellation and launch lifetime safety."""
from desktop_controls import APPS_BUTTON,SHELL_BUTTON,app_card,task_button
import argparse,json,re,shutil,struct,tempfile,time
from array import array
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='async-load-',dir=out) as tmp:
        d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
        files,_=inspect_tree(image,2048);payload=files['APPS/BIGDEMO/BIGDEMO.ELF']
        package=d/'extra';(package/'apps').mkdir(parents=True)
        (package/'apps/slow.elf').write_bytes(payload+bytes(1048576-len(payload)))
        bad=bytearray(payload);struct.pack_into('<I',bad,struct.unpack_from('<I',bad,28)[0]+24,7)
        (package/'apps/bad.elf').write_bytes(bad)
        (package/'applications.json').write_text(json.dumps([{'name':'slow','file':'SLOW.ELF'},{'name':'bad','file':'BAD.ELF'}]));seed_apps(image,package)
        g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image)
        try:
            def log():return (d/'debug.log').read_text()
            def until(fn,message,timeout=45):
                end=time.monotonic()+timeout
                while not fn():
                    assert time.monotonic()<end,message+'\n'+log()[-1800:];time.sleep(.035)
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            def memory(address,n):
                f=d/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {address:#x} {n} "{f.as_posix()}"'});return f.read_bytes()
            baseline=frames();g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            files,_=inspect_tree(image,2048);names=sorted(k.split('/')[-1][:-4] for k in files if k.startswith('APPS/') and k.endswith(('.BIN','.ELF')))
            def menu(name):
                marker=len(log());g.key('ctrl-esc');g.key('home')
                for _ in range(names.index(name)):g.key('down')
                g.key('ret');return marker
            def launch(name):
                marker=menu(name);until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name,log()[marker:]),'launch '+name)
                pid=int(re.search(r'Desktop: launch pid=(\d+) ',log()[marker:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log(),'window '+name);return pid
            def window(pid):
                m=re.search(r'Multiwindow: open pid='+str(pid)+r' handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+)',log());return int(m[1],16),int(m[2]),int(m[3])
            counter=launch('COUNTER');terminal=launch('TERMINAL')
            font=memory(int(re.search(r'PTY: open .*font=([0-9a-f]+)',log())[1],16),4096)
            glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
            def screen():
                pointer,w,h=window(terminal);pixels=array('I');pixels.frombytes(memory(pointer,w*h*4))
                return '\n'.join(''.join(glyphs.get(bytes(pixels[(row*16+y)*w+col*8+x]!=0 for y in range(16) for x in range(8)),'?') for col in range(80)).rstrip() for row in range(25))
            position=[400,300]
            def click(x,y):
                while position!=[x,y]:
                    dx=max(-80,min(80,x-position[0]));dy=max(-80,min(80,y-position[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});position[0]+=dx;position[1]+=dy;time.sleep(.02)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.06)
            def focus_terminal():click(300,11)
            block=g.call('query-block')[0]['device']
            def throttle(rate):g.call('block_set_io_throttle',{'device':block,'bps':0,'bps_rd':rate,'bps_wr':0,'iops':0,'iops_rd':0,'iops_wr':0})
            throttle(262144)
            def started(marker,path='/APPS/SLOW.ELF'):until(lambda:'App load: start path='+path in log()[marker:],'load start '+path)
            def done(marker):return 'App load: done' in log()[marker:]
            marker=menu('SLOW');started(marker)
            pointer,w,_=window(counter);before=memory(pointer+w*92*4,w*18*4)
            focus_terminal();g.type('echo LOAD-LIVE\n');until(lambda:'LOAD-LIVE' in screen(),'terminal input stalled during load')
            assert not done(marker),'slow fixture finished before progress could be checked'
            assert memory(pointer+w*92*4,w*18*4)!=before,'Counter froze during executable read'
            g.key('ctrl-alt-esc');until(lambda:done(marker),'cancel did not finish')
            assert 'cancelled=true' in log()[marker:] and 'Desktop: launch pid=' not in log()[marker:]
            # A queued filesystem command resumes only after the cancelled read unwinds.
            g.type('touch after.txt\n');until(lambda:'AFTER.TXT' in inspect_tree(image,2048)[0],'file operations did not recover after cancellation')
            marker=len(log());g.type('run --isolated slow.elf\n');started(marker,'slow.elf')
            g.key('ctrl-alt-k');until(lambda:f'Scheduler: forced exit pid={terminal}' in log()[marker:],'parent close')
            until(lambda:done(marker),'closed parent load did not unwind');assert 'Desktop: terminal launch pid=' not in log()[marker:]
            terminal=launch('TERMINAL')
            # Capture an Apps request on the old volume, then defer unmount until read completion.
            marker=menu('SLOW');started(marker);g.key('ctrl-esc');click(*app_card(names.index('BIGDEMO')))
            until(lambda:'App load: queued path=/APPS/BIGDEMO/BIGDEMO.ELF' in log()[marker:],'Apps request was not queued during read');focus_terminal();g.type('unmount\n')
            until(lambda:'Waiting for disk...' in screen(),'unmount bypassed active loader')
            g.key('ctrl-alt-esc');until(lambda:'App source volume changed' in log()[marker:],'stale queued launch was not rejected')
            assert not re.search(r'Desktop: launch pid=\d+ path=/APPS/BIGDEMO/BIGDEMO.ELF',log()[marker:]),'stale Apps launch executed'
            g.type('mount\n');until(lambda:'Mounted FAT16' in screen(),'mount after stale launch')
            throttle(0)
            marker=menu('BAD');started(marker,'/APPS/BAD.ELF');until(lambda:done(marker),'malformed load completion')
            assert 'Invalid or unsupported ELF executable.' in log()[marker:] and 'Desktop: launch pid=' not in log()[marker:]
            focus_terminal();marker=len(log());g.type('run --isolated slow.elf\n');started(marker,'slow.elf')
            until(lambda:re.search(r'Desktop: terminal launch pid=(\d+) name=slow.elf',log()[marker:]),'successful isolated load')
            child=int(re.search(r'Desktop: terminal launch pid=(\d+) ',log()[marker:])[1]);until(lambda:f'Multiwindow: open pid={child} ' in log(),'loaded GUI child')
            g.call('screendump',{'filename':str(out/'async-loader.png'),'format':'png'})
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={child} code=0' in log(),'loaded child close')
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={terminal} code=0' in log(),'terminal close')
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={counter} code=0' in log(),'counter close');time.sleep(.3)
            click(*SHELL_BUTTON);g.wait_for('Desktop finished.');assert frames()==baseline,'cancelled/failed/successful loaders leaked frames'
            g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
        finally:g.close();shutil.copyfile(d/'debug.log',out/'async-loader-debug.log')
    print('PASS yielding loader: real throttled reads, live Counter/Terminal, cancellation, closed parent, stale volume, malformed rejection, isolated completion, serialized disk access and frame balance')

if __name__=='__main__':main()
