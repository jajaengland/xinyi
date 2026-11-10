"""Verify reversible Files deletion, recovery safeguards and cold-boot storage."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);a=p.parse_args();out=ROOT/'target/recycle-bin';out.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='recycle-',dir=out) as tmp:
  d=Path(tmp);image=d/'test.img';shutil.copyfile(a.disk,image);pkg=d/'helper';(pkg/'apps').mkdir(parents=True)
  subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/trash_check/main.rs'),'--output',str(pkg/'apps/rcheck.bin'),'--force'],check=True)
  (pkg/'applications.json').write_text(json.dumps([{'file':'RCHECK.BIN'}]));seed_apps(image,pkg)
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,extra_args=['-audiodev','none,id=pcm','-device','AC97,audiodev=pcm'])
  try:
   g.command('run rcheck','Recycle checks passed!');g.wait_for('User program exited with code 0.')
   for path in ['/DOCS','/DOCS/FOLDER']:g.command('mkdir '+path,'Created directory:')
   for path,body in [('/DOCS/A.TXT','recover me'),('/DOCS/FOLDER/INNER.TXT','nested recovery')]:g.command('touch '+path,'Created:');g.command('write '+path+' '+body,'Wrote')
   g.command('cd /docs','xinyi:/DOCS>');g.type('desktop\n')
   def log():return (d/'debug.log').read_text(errors='replace')
   def tree():return inspect_tree(image,2048)
   def until(fn,msg):
    end=time.monotonic()+40
    while True:
     try:
      if fn():return
     except AssertionError:pass
     assert time.monotonic()<end,msg+'\n'+log()[-1500:];time.sleep(.05)
   until(lambda:'Desktop scheduler: ready' in log(),'desktop')
   current=0
   def launch(name):
    nonlocal current
    marker=len(log());g.key('ctrl-esc');g.type(name+'\n');until(lambda:'Multiwindow: open pid=' in log()[marker:],'launch '+name);current=int(re.search(r'Multiwindow: open pid=(\d+)',log()[marker:])[1]);time.sleep(.3)
   def close():
    g.dismiss_dialogs(current,wait=True);marker=len(log());g.key('alt-f4');until(lambda:'Scheduler: exit pid=' in log()[marker:],'close')
   pointer=[400,300]
   def click(x,y):
    while pointer!=[x,y]:
     dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
    for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
   def win():
    m=re.search(r'Multiwindow: open pid='+str(current)+r'.*surface=([0-9a-f]+).*pos=(\d+),(\d+)',log());return int(m[1],16),int(m[2]),int(m[3])
   def confirmation():
    addr,_,_=win();path=d/'point.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {addr+(96*640+24)*4:#x} 4 "{path.as_posix()}"'});return int.from_bytes(path.read_bytes(),'little')==0x94aabb
   launch('files');g.key('ctrl-n');until(confirmation,'New file dialog');g.type('NOFILE.TXT');_,wx,wy=win();click(wx+8+556,wy+34+324);until(lambda:not confirmation(),'Cancel new file')
   click(wx+8+318,wy+34+17);until(confirmation,'New folder dialog');g.type('NOMADE');click(wx+8+556,wy+34+324);until(lambda:not confirmation(),'Cancel new folder');assert 'DOCS/NOMADE' not in tree()[1]
   g.key('home');g.key('f2');until(confirmation,'Rename dialog');g.key('esc');until(lambda:not confirmation(),'Cancel rename');assert 'DOCS/FOLDER' in tree()[1]
   g.key('ctrl-a');sound_start=len(log());g.key('delete');until(confirmation,'Centered recycle confirmation')
   until(lambda:'Sound: play warning' in log()[sound_start:],'Recycle confirmation sound')
   g.call('screendump',{'filename':str((out/'recycle-confirmation.png').resolve()),'format':'png'})
   _,wx,wy=win();click(wx+8+556,wy+34+324);until(lambda:not confirmation(),'Captured Cancel button');assert tree()[0]['DOCS/A.TXT']==b'recover me'

   g.key('delete');until(confirmation,'Recycle confirmation again');_,wx,wy=win();click(wx+8+452,wy+34+324);until(lambda:'DOCS/A.TXT' not in tree()[0] and 'DOCS/FOLDER' not in tree()[1],'recycle selection')
   until(lambda:tree()[0].get('TRASH/T0000001/INFO.DAT',b'')[4:5]==bytes([1]),'ready records published')
   assert tree()[0]['TRASH/T0000001/ITEM']==b'recover me';assert tree()[0]['TRASH/T0000000/ITEM/INNER.TXT']==b'nested recovery'
   g.dismiss_dialogs(current,wait=True);g.key('backspace')
   subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(d/'font.exe')],check=True);font=subprocess.check_output([str(d/'font.exe')])
   def row_is(row,value):
    addr,_,_=win();path=d/'row.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {addr+(80+row*16)*640*4:#x} {640*16*4} "{path.as_posix()}"'});data=path.read_bytes()
    for i,c in enumerate(value):
     for y in range(16):
      for x in range(8):
       ink=int.from_bytes(data[(y*640+8+i*8+x)*4:(y*640+8+i*8+x+1)*4],'little')==0
       if ink!=bool(font[c*16+y]&(128>>x)):return False
    return True
   until(lambda:any(row_is(i,b'TRASH') for i in range(16)),'Root Recycle Bin folder')
   row=next(i for i in range(16) if row_is(i,b'TRASH'));g.key('home')
   for _ in range(row):g.key('down')
   before={k:v for k,v in tree()[0].items() if k.startswith('TRASH/')};time.sleep(1.2);sound_start=len(log());g.key('delete')
   until(lambda:'The Recycle Bin and its contents cannot be recycled.' in log()[sound_start:],'Protected bin warning dialog')
   until(lambda:'Sound: play warning' in log()[sound_start:],'Protected bin warning sound')
   assert before=={k:v for k,v in tree()[0].items() if k.startswith('TRASH/')},'Protected recycle mutated bin data'
   g.dismiss_dialogs(current);marker=len(log());g.key('alt-f4');until(lambda:'Scheduler: exit pid=' in log()[marker:],'Close Files after warning')
   launch('trash');g.key('down')
   g.key('ret');g.key('n');assert 'DOCS/A.TXT' not in tree()[0]
   g.key('ret');g.key('y');until(lambda:tree()[0].get('DOCS/A.TXT')==b'recover me','restore file')
   g.dismiss_dialogs(current,wait=True);g.key('ret');g.key('y');until(lambda:tree()[0].get('DOCS/FOLDER/INNER.TXT')==b'nested recovery','restore nested folder')
   g.dismiss_dialogs(current,wait=True);time.sleep(1.2);sound_start=len(log());g.key('delete')
   until(lambda:'text=Recycle Bin is empty.' in log()[sound_start:],'Empty bin information dialog')
   until(lambda:'Sound: play info' in log()[sound_start:],'Empty bin information sound')
   close()
   # Use QMP relative pointer events from its initial location.
   while pointer!=[756,576]:
    dx=max(-60,min(60,756-pointer[0]));dy=max(-60,min(60,576-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.04)
   for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.1)
   g.wait_for('Desktop finished.');g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
  finally:g.close();shutil.copyfile(d/'debug.log',out/'debug.log')
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image)
  try:g.command('cat /docs/a.txt','recover me');g.command('cat /docs/folder/inner.txt','nested recovery')
  finally:g.close()
 print('PASS Files dialogs and Recycle Bin: create/rename cancel, captured recycle confirmation, warning audio, protected-bin refusal and audio, file/folder recycle and restore, cancel, original-name collision, unfinished/corrupt records preserved, stale volume refusal, cold boot and fsck')
if __name__=='__main__':main()
