"""Check Applications paging, launches, folder inspection and confirmed removal."""
import argparse,json,re,shutil,subprocess,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree
from package_app import build as package_app

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--package',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/app-manager';out.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-',dir=out) as temporary:
        d=Path(temporary);image=d/'system.img';shutil.copyfile(a.disk,image)
        pkg=d/'fixture';(pkg/'apps/zztest').mkdir(parents=True);shutil.copyfile(a.package/'apps/counter/counter.bin',pkg/'apps/zztest/zztest.bin')
        (pkg/'apps/zztest/zztest.app').write_text('XINYI-APP 1\nexec=ZZTEST.BIN\ntitle=Test bundle\ndescription=Private manager test\nicon=application\naccess=trusted\nhandles=\nversion=7\n')
        (pkg/'applications.json').write_text(json.dumps([{'directory':'ZZTEST','file':'ZZTEST.BIN','manifest':'ZZTEST.APP'}]));seed_apps(image,pkg)
        for root,version in [('BASE',1),('NEW',2),('LOW',1)]:
            fixture=d/root;(fixture/'apps/sample').mkdir(parents=True)
            shutil.copyfile(a.package/'apps/counter/counter.bin',fixture/'apps/sample/sample.bin')
            (fixture/'apps/sample/sample.app').write_text(f'XINYI-APP 1\nexec=SAMPLE.BIN\ntitle=Sample\ndescription=Manager workflow test\nicon=application\naccess=trusted\nhandles=\nversion={version}\n')
            (fixture/'applications.json').write_text(json.dumps([{'directory':'SAMPLE','file':'SAMPLE.BIN','manifest':'SAMPLE.APP'}]));seed_apps(image,fixture,root_name=root)
        archives=d/'archives';(archives/'apps').mkdir(parents=True);artifacts=[]
        for name,version in [('OLD',1),('NEW',2),('INVALID',3)]:
            folder=d/name/'PKGTEST';(folder/'DATA').mkdir(parents=True);(folder/'EMPTY').mkdir()
            (folder/'PKGTEST.BIN').write_bytes((a.package/'apps/counter/counter.bin').read_bytes() if name!='INVALID' else b'\x7fELFbad')
            (folder/'PKGTEST.APP').write_text(f'XINYI-APP 1\nexec=PKGTEST.BIN\ntitle=Packaged test\ndescription=Offline package workflow\nicon=application\naccess=trusted\nhandles=\nversion={version}\n')
            (folder/'DATA'/'TEST.DAT').write_bytes(bytes([version])*20000)
            (archives/'apps'/(name.lower()+'.xap')).write_bytes(package_app(folder));artifacts.append({'file':name+'.XAP'})
        bad=bytearray((archives/'apps/old.xap').read_bytes());bad[-1]^=1;(archives/'apps/bad.xap').write_bytes(bad);artifacts.append({'file':'BAD.XAP'})
        (archives/'applications.json').write_text(json.dumps(artifacts));seed_apps(image,archives,root_name='PACKS')
        subprocess.run(['rustc' ,'--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(d/'font.exe')],check=True);font=subprocess.check_output([str(d/'font.exe')])
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
            def snapshot():
                deadline=time.monotonic()+10
                while True:
                    try:return inspect_tree(image,2048)
                    except AssertionError:
                        if time.monotonic()>=deadline:raise
                        time.sleep(.03)
            def tree():return snapshot()[0]
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
            g.dismiss_dialogs(pid);g.key('delete');assert 'APPS/APPMGR/APPMGR.ELF' in tree(),'Self removal was allowed'
            pointer=[400,300]
            def click(x,y):
                while pointer!=[x,y]:
                    dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
                for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
            g.dismiss_dialogs(pid);click(wx+8+200,wy+34+356);until(lambda:text(surface,640,300,112,b'VIEWER'),'Next page control')
            g.dismiss_dialogs(pid);g.key('end');until(lambda:text(surface,640,300,112,b'ZZTEST') and text(surface,640,380,136,b'7'),'End/version')
            start=len(log());g.key('ret');until(lambda:re.search(r'Desktop: (?:terminal )?launch pid=(\d+) (?:path|name)=/APPS/ZZTEST/ZZTEST.BIN',log()[start:]),'Launch selected app')
            child=int(re.search(r'Desktop: (?:terminal )?launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={child}' in log(),'Counter window')
            g.key('alt-tab');g.dismiss_dialogs(pid);g.key('delete');g.key('n');assert 'APPS/ZZTEST/ZZTEST.BIN' in tree(),'Cancel removed a bundle'
            g.dismiss_dialogs(pid);g.key('delete');g.key('y');time.sleep(.3);assert 'APPS/ZZTEST/ZZTEST.BIN' in tree(),'Running bundle was removed'
            g.key('alt-tab');g.key('q');until(lambda:f'Scheduler: exit pid={child}' in log(),'Counter close')
            g.dismiss_dialogs(pid);g.key('delete');g.key('y');until(lambda:'APPS/ZZTEST/ZZTEST.BIN' not in tree(),'Confirmed removal')
            assert tree()['KEEP.TXT']==b'personal document','Removal changed a document'
            g.dismiss_dialogs(pid);g.key('end');until(lambda:text(surface,640,300,112,b'VIEWER'),'Selection after removal')
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
            g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={folder_pid}' in log(),'Files close')
            def dialog_open():
                path=d/'point.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {surface+(76*640+40)*4:#x} 4 "{path.as_posix()}"'})
                return int.from_bytes(path.read_bytes(),'little')==0x94aabb
            def workflow(button,path,confirm=True):
                g.dismiss_dialogs(pid)
                until(lambda:not dialog_open(),'Previous dialog close')
                click(wx+8+[316,392,468,566][button],wy+34+356)
                until(dialog_open,'Workflow dialog')
                if button<2:
                    until(lambda:text(surface,640,62,154,b'/'),'Folder field ready')
                    g.key('ctrl-a')
                    for i,character in enumerate(path):
                        g.type(character);until(lambda:text(surface,640,62,154,path[:i+1].encode()),'Source folder input')
                    review_start=len(log());g.key('ret')
                    if path in ['/LOW/SAMPLE','/PACKS/BAD.XAP']:
                        until(lambda:f'Dialog: pid={pid}' in log()[review_start:],'Review refusal');g.dismiss_dialogs(pid);return
                    until(lambda:text(surface,640,56,176,b'New version:'),'Bundle review')
                completion=len(log());g.key('ret' if confirm else 'esc')
                if confirm:
                    until(lambda:f'Dialog: pid={pid}' in log()[completion:],'Operation result dialog');g.dismiss_dialogs(pid)
                until(lambda:not dialog_open(),'Confirmation/cancellation')
            def version(n):return f'version={n}'.encode() in tree().get('APPS/SAMPLE/SAMPLE.APP',b'').splitlines()
            workflow(0,'/BASE/SAMPLE',False);assert not version(1),'Cancelled install wrote files'
            workflow(0,'/BASE/SAMPLE');until(lambda:version(1),'Manager install')
            workflow(1,'/NEW/SAMPLE',False);assert version(1),'Cancelled update wrote files'
            workflow(1,'/NEW/SAMPLE');until(lambda:version(2) and 'APPS/_UPD.ID' in tree(),'Manager update')
            workflow(2,'');until(lambda:version(1) and 'APPS/_UPD.ID' not in tree(),'Manager restore')
            workflow(1,'/LOW/SAMPLE');g.key('esc');assert version(1),'Downgrade was accepted'
            workflow(1,'/NEW/SAMPLE');until(lambda:version(2),'Second update')
            workflow(3,'',False);assert 'APPS/_UPD.ID' in tree(),'Cancelled discard deleted backup'
            workflow(3,'');until(lambda:version(2) and 'APPS/_UPD.ID' not in tree(),'Manager discard')
            assert 'BASE/SAMPLE/SAMPLE.BIN' in tree() and tree()['KEEP.TXT']==b'personal document'
            def packaged(n):return f'version={n}'.encode() in tree().get('APPS/PKGTEST/PKGTEST.APP',b'').splitlines()
            workflow(0,'/PACKS/OLD.XAP',False);assert 'APPS/PKGTEST/PKGTEST.BIN' not in tree()
            workflow(0,'/PACKS/BAD.XAP');g.key('esc');assert 'APPS/PKGTEST/PKGTEST.BIN' not in tree()
            invalid_start=len(log());workflow(0,'/PACKS/INVALID.XAP')
            until(lambda:'result=-8' in log()[invalid_start:],'Invalid executable rejection')
            until(lambda:'PKGIN' not in snapshot()[1],'Invalid executable extraction cleanup')
            assert 'APPS/PKGTEST/PKGTEST.BIN' not in tree()
            workflow(0,'/PACKS/OLD.XAP');until(lambda:packaged(1),'Package installation')
            assert tree()['APPS/PKGTEST/DATA/TEST.DAT']==bytes([1])*20000
            workflow(1,'/PACKS/NEW.XAP');until(lambda:packaged(2),'Package update')
            assert tree()['APPS/PKGTEST/DATA/TEST.DAT']==bytes([2])*20000
            workflow(2,'');until(lambda:packaged(1) and 'APPS/_UPD.ID' not in tree(),'Package rollback')
            workflow(1,'/PACKS/NEW.XAP');until(lambda:packaged(2),'Second package update')
            workflow(3,'');until(lambda:'APPS/_UPD.ID' not in tree(),'Package backup discard')
            files,folders=snapshot();assert 'APPS/PKGTEST/EMPTY' in folders
            assert 'PKGIN' not in folders and files['PACKS/OLD.XAP']==(archives/'apps/old.xap').read_bytes()

            g.key('home')
            ids=sorted({p.split('/')[1] for p in tree() if p.startswith('APPS/') and p.endswith(('.BIN','.ELF')) and not p.split('/')[1].startswith('_')})
            for _ in range(ids.index('NOTEPAD')):g.key('down')
            until(lambda:text(surface,640,300,112,b'NOTEPAD'),'Defaults selected app')
            g.dismiss_dialogs(pid);g.key('ctrl-d');until(dialog_open,'Defaults dialog');g.key('esc');until(lambda:not dialog_open(),'Cancel defaults')
            assert 'DEFAULTS.CFG' not in tree(),'Cancel wrote default preferences'
            g.dismiss_dialogs(pid);g.key('ctrl-d');until(dialog_open,'Defaults dialog');g.key('ret')
            until(lambda:b'TXT=NOTEPAD\n' in tree().get('DEFAULTS.CFG',b''),'Save default')
            g.dismiss_dialogs(pid);g.key('ctrl-d');until(dialog_open,'Saved defaults dialog');until(lambda:text(surface,640,136,224,b'NOTEPAD'),'Current preference')
            click(wx+8+264,wy+34+330);until(lambda:not dialog_open(),'Reset default')
            assert b'TXT=' not in tree()['DEFAULTS.CFG'],'Reset kept mapping'
            g.dismiss_dialogs(pid);g.key('ctrl-d');until(dialog_open,'Defaults resave');g.key('ret')
            until(lambda:b'TXT=NOTEPAD\n' in tree().get('DEFAULTS.CFG',b''),'Resave default')
            g.dismiss_dialogs(pid);g.key('ctrl-p');until(dialog_open,'Permissions dialog');g.key('esc');until(lambda:not dialog_open(),'Cancel permissions')
            assert 'ACCESS.CFG' not in tree(),'Cancel wrote access restrictions'
            g.dismiss_dialogs(pid);g.key('ctrl-p');until(dialog_open,'Permissions choice');g.key('1')
            until(lambda:b'NOTEPAD=R\n' in tree().get('ACCESS.CFG',b''),'Read-only permission')
            g.dismiss_dialogs(pid);g.key('ctrl-p');until(dialog_open,'Permissions reset');g.key('0')
            until(lambda:b'NOTEPAD=' not in tree().get('ACCESS.CFG',b''),'Reset declared access')
            g.dismiss_dialogs(pid);g.key('end');until(lambda:text(surface,640,300,112,b'VIEWER'),'Restricted manifest selection')
            g.dismiss_dialogs(pid);g.key('ctrl-p');until(dialog_open,'Restricted manifest permissions');g.key('2');time.sleep(.2);g.dismiss_dialogs(pid);g.key('esc');until(lambda:not dialog_open(),'Reject excess permission')
            assert b'VIEWER=' not in tree()['ACCESS.CFG'],'UI allowed access exceeding the declaration'
            g.key('home')
            for _ in range(ids.index('NOTEPAD')):g.key('down')
            until(lambda:text(surface,640,300,112,b'NOTEPAD'),'Persist permission selection')
            g.dismiss_dialogs(pid);g.key('ctrl-p');until(dialog_open,'Persist permissions');g.key('1')
            until(lambda:b'NOTEPAD=R\n' in tree().get('ACCESS.CFG',b''),'Persist read-only choice')
            g.dismiss_dialogs(pid);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={pid}' in log(),'Manager close')
            g.key('esc');g.wait_for('Desktop finished.');assert frames()==baseline,'Manager leaked frames'
            g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
            g.close();cold=d/'cold';cold.mkdir();g=Guest(qemu,None,cold,disk=image)
            g.command('cat /defaults.cfg','TXT=NOTEPAD');g.command('cat /access.cfg','NOTEPAD=R')
            print('PASS Applications: saved permissions/cancel/reset, excess-access refusal and persistence, default selection/cancel/reset/current preference and cold-boot persistence, metadata/version, page controls, launch, exact folder, self/running refusal, removal/cancel, offline packages, corruption and invalid executables, nested/empty assets, install/update review and cancellation, downgrade refusal, restore/discard, document preservation, cleanup and fsck')
        finally:g.close();shutil.copyfile(d/'debug.log',out/'debug.log')

if __name__=='__main__':main()
