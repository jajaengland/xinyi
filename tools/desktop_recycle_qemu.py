"""Verify captured desktop recycling, confirmation, clipboard preservation and restore."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);a=p.parse_args();out=ROOT/'target/desktop-recycle';out.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='recycle-',dir=out) as tmp:
  d=Path(tmp);image=d/'test.img';shutil.copyfile(a.disk,image);pkg=d/'helper';(pkg/'apps').mkdir(parents=True)
  subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/trash_check/main.rs'),'--output',str(pkg/'apps/rcheck.bin'),'--force'],check=True)
  (pkg/'applications.json').write_text(json.dumps([{'file':'RCHECK.BIN'}]));seed_apps(image,pkg)
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image)
  try:
   g.command('run rcheck','Recycle checks passed!');g.wait_for('User program exited with code 0.')
   for path in ['/DESKTOP','/DESKTOP/FOLDER']:g.command('mkdir '+path,'Created directory:')
   for path,body in [('/DESKTOP/A.TXT','recover me'),('/DESKTOP/FOLDER/INNER.TXT','nested recovery')]:g.command('touch '+path,'Created:');g.command('write '+path+' '+body,'Wrote')
   g.command('cd /desktop','xinyi:/DESKTOP>');g.type('desktop\n')
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
   def launch(name):
    marker=len(log());g.key('ctrl-esc');g.type(name+'\n');until(lambda:'Multiwindow: open pid=' in log()[marker:],'launch '+name);time.sleep(.3)
   def close():
    marker=len(log());g.key('alt-f4');until(lambda:'Scheduler: exit pid=' in log()[marker:],'close')
   g.key('ctrl-a');g.key('ctrl-c');marker=len(log());g.key('delete')
   until(lambda:'nr=103 args=00000002/' in log()[marker:],'desktop recycle delivered');time.sleep(.3);g.key('n');assert tree()[0]['DESKTOP/A.TXT']==b'recover me';close()
   pointer=[400,300]
   def move(x,y):
    while pointer!=[x,y]:
     dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.04)
   def click(x,y,button='left'):
    move(x,y)
    for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':button,'down':down}}]});time.sleep(.12)
   marker=len(log());click(180,60,'right');click(200,195)
   until(lambda:'nr=103 args=00000002/' in log()[marker:],'menu recycle delivered');time.sleep(.3);g.key('y');until(lambda:'DESKTOP/A.TXT' not in tree()[0] and 'DESKTOP/FOLDER' not in tree()[1],'recycle selection')
   until(lambda:tree()[0].get('TRASH/T0000001/INFO.DAT',b'')[4:5]==bytes([1]),'ready records published')
   assert tree()[0]['TRASH/T0000001/ITEM']==b'recover me';assert tree()[0]['TRASH/T0000000/ITEM/INNER.TXT']==b'nested recovery'
   close();launch('trash');g.key('down')
   g.key('ret');g.key('n');assert 'DESKTOP/A.TXT' not in tree()[0]
   g.key('ret');g.key('y');until(lambda:tree()[0].get('DESKTOP/A.TXT')==b'recover me','restore file')
   g.key('ret');g.key('y');until(lambda:tree()[0].get('DESKTOP/FOLDER/INNER.TXT')==b'nested recovery','restore nested folder')
   close()
   # Use QMP relative pointer events from its initial location.
   while pointer!=[756,576]:
    dx=max(-60,min(60,756-pointer[0]));dy=max(-60,min(60,576-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.04)
   for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.1)
   g.wait_for('Desktop finished.');g.command('run rcheck clip','Desktop recycle clipboard preserved!');g.wait_for('User program exited with code 0.');g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
  finally:g.close();shutil.copyfile(d/'debug.log',out/'debug.log')
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image)
  try:g.command('cat /desktop/a.txt','recover me');g.command('cat /desktop/folder/inner.txt','nested recovery')
  finally:g.close()
 print('PASS desktop Recycle: keyboard/menu multi-selection, clipboard preservation, file/folder recycle and restore, cancel, original-name collision, unfinished/corrupt records preserved, stale volume refusal, cold boot and fsck')
if __name__=='__main__':main()
