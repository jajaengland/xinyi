"""Real >64 KiB ELF execution, rejection, isolation, installation and Snake rendering."""
import argparse,json,re,shutil,struct,tempfile,time
from pathlib import Path
from folders_qemu import inspect_tree
from seed_apps import seed_apps
from smoke import Guest,ROOT

def corrupt_flat_magic(image):
    b=bytearray(image.read_bytes());start=struct.unpack_from('<I',b,454)[0];boot=start*512
    reserved,entries,spf=(struct.unpack_from('<H',b,boot+i)[0] for i in (14,17,22));spc=b[boot+13]
    root=start+reserved+b[boot+16]*spf;first=root+(entries*32+511)//512
    # APPS is in the fixed root. Its directory may occupy more than one cluster.
    slot=next(p for p in range(root*512,(root*512)+entries*32,32) if b[p:p+11]==b'APPS       ')
    cluster=struct.unpack_from('<H',b,slot+26)[0];fat=(start+reserved)*512
    while cluster<0xfff8:
        base=(first+(cluster-2)*spc)*512
        for p in range(base,base+spc*512,32):
            if b[p:p+11]==b'FLATBIG ELF':
                c=struct.unpack_from('<H',b,p+26)[0];p=(first+(c-2)*spc)*512;b[p:p+4]=b'NOPE';image.write_bytes(b);return
        cluster=struct.unpack_from('<H',b,fat+cluster*2)[0]
    raise AssertionError('flat limit fixture missing')

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True);qemu=r'C:\Program Files\qemu\qemu-system-i386.exe'
    with tempfile.TemporaryDirectory(prefix='large-apps-',dir=out) as tmp:
        d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
        files,_=inspect_tree(image,2048);payload=files['APPS/BIGDEMO/BIGDEMO.ELF'];assert 65536<len(payload)<=1048576
        package=d/'extra';(package/'apps').mkdir(parents=True)
        bad=bytearray(payload);ph=struct.unpack_from('<I',bad,28)[0];struct.pack_into('<I',bad,ph+24,7)
        fixtures={'BAD.ELF':bad,'SHORT.ELF':payload[:-1],'FLATBIG.ELF':b'\x7fELF'+bytes(65533)}
        for name,data in fixtures.items():(package/'apps'/name.lower()).write_bytes(data)
        (package/'applications.json').write_text(json.dumps([{'name':k.split('.')[0].lower(),'file':k} for k in fixtures]));seed_apps(image,package);corrupt_flat_magic(image)
        g=Guest(qemu,None,d,disk=image)
        try:
            g.command('use 1','ready')
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            baseline=frames()
            g.command('run bigdemo check','Large ELF checks passed!');g.wait_for('User program exited with code 0.');assert frames()==baseline
            g.command('run bigdemo fault','User program faulted:');assert 'error=0x7' in g.screen(),g.screen();assert frames()==baseline
            for name,expected in [('bad.elf','Invalid or unsupported ELF executable.'),('short.elf','Invalid or unsupported ELF executable.'),('flatbig.elf','Flat program exceeds 64 KiB')]:
                g.command('run '+name,expected);assert frames()==baseline
            g.command('run snake','S N A K E');g.key('p');assert '@@' not in g.screen();assert '[]' in g.screen()
            g.call('screendump',{'filename':str(out/'snake-no-eyes.png'),'format':'png'});g.key('esc');g.wait_for('User program exited with code 0.');assert frames()==baseline
            for name in fixtures:g.command('delete /APPS/'+name,'Deleted:')
            def log():return (d/'debug.log').read_text()
            def until(fn,message):
                end=time.monotonic()+45
                while not fn():
                    assert time.monotonic()<end,message+'\n'+log()[-1200:];time.sleep(.04)
            files,_=inspect_tree(image,2048);names=sorted(k.split('/')[-1][:-4] for k in files if k.startswith('APPS/') and k.endswith(('.BIN','.ELF')))
            g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            def launch(name):
                start=len(log());g.key('ctrl-esc');g.key('home')
                for _ in range(names.index(name)):g.key('down')
                g.key('ret');until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name,log()[start:]),'launch '+name)
                pid=int(re.search(r'Desktop: launch pid=(\d+) ',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log()[start:],'window '+name);return pid
            counter=launch('COUNTER');first=launch('BIGDEMO');second=launch('BIGDEMO')
            g.key('right');time.sleep(.1);g.key('alt-tab');g.key('right');time.sleep(.1);g.key('alt-tab');g.key('left');time.sleep(.1)
            assert 'Scheduler: fault' not in log(),'large concurrent process fault'
            g.call('screendump',{'filename':str(out/'large-apps.png'),'format':'png'})
            # Return to each instance before closing; key-triggered validation checks private globals/heap.
            for pid in [second,first,counter]:
                for _ in range(4):
                    # Closing the current front window normally restores the last focused one.
                    if f'Scheduler: exit pid={pid} code=0' in log():break
                    g.key('alt-f4');time.sleep(.15)
                until(lambda:f'Scheduler: exit pid={pid} code=0' in log(),'close '+str(pid))
            time.sleep(.3);g.key('esc')
            g.wait_for('Desktop finished.');assert frames()==baseline,'large loader/app frames leaked'
        finally:g.close();shutil.copyfile(d/'debug.log',out/'large-apps-debug.log')
        # Copy the actual large app through Xinyi's installer, then cold-boot it.
        destination=d/'installed.img'
        with destination.open('xb') as f:f.truncate(16*1024*1024)
        g=Guest(qemu,None,d,disk=image,extra_args=['-drive',f'file={destination.as_posix()},format=raw,if=ide,index=1'])
        try:
            g.command('use 1','ready');g.command('install 2','To install, enter: install confirm')
            g.type('install confirm\n');g.wait_for('Installed drive ready',timeout=60)
        finally:g.close()
        installed,_=inspect_tree(destination,2048);assert installed['APPS/BIGDEMO/BIGDEMO.ELF']==payload
        g=Guest(qemu,None,d,disk=destination,disk_index=1)
        try:
            g.command('run bigdemo check','Large ELF checks passed!');g.wait_for('User program exited with code 0.')
            g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
        finally:g.close()
    print('PASS large apps: real >64 KiB ELF data/BSS/heap, read-only page fault, malformed/flat rejection, concurrent private state, installer/cold BIOS boot, Snake head and frame balance')

if __name__=='__main__':main()
