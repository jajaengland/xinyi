"""Desktop files persist; wallpaper transfers reuse the Files collision-safe receiver."""
import argparse,re,shutil,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);a=p.parse_args();out=ROOT/'target/desktop-final';out.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='desktop-',dir=out) as tmp:
  d=Path(tmp);image=d/'test.img';shutil.copyfile(a.disk,image)
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image)
  try:
   for folder in ['/DESKTOP','/DESKTOP/FOLDER','/SOURCE','/SOURCE/OUT']:g.command('mkdir '+folder,'Created directory:')
   for path,body in [('/DESKTOP/NOTE.TXT','desktop note'),('/SOURCE/A.TXT','source document')]:
    g.command('touch '+path,'Created:');g.command('write '+path+' '+body,'xinyi:')
   g.command('cd /source','xinyi:/SOURCE>');g.type('desktop\n')
   def log():return (d/'debug.log').read_text(errors='replace')
   def tree():return inspect_tree(image,2048)
   def until(fn,msg):
    end=time.monotonic()+35
    while True:
     if fn():return
     assert time.monotonic()<end,msg+'\n'+log()[-1600:];time.sleep(.05)
   until(lambda:'Desktop scheduler: ready' in log(),'desktop ready')
   pointer=[400,300]
   def move(x,y):
    while pointer!=[x,y]:
     dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.04)
   def button(down):g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.12)
   def click(x,y):move(x,y);button(True);button(False)
   def launch(name):
    start=len(log());g.key('ctrl-esc');g.type(name+'\n');until(lambda:'Multiwindow: open pid=' in log()[start:],'launch '+name);time.sleep(.2)
   def close():
    count=log().count('Scheduler: exit');g.key('alt-f4');until(lambda:log().count('Scheduler: exit')>count,'close');time.sleep(.2)
   launch('files')
   # Source OUT folder first, A.TXT second. Drop onto exposed wallpaper.
   move(80,236);button(True);move(103,236);move(740,50);button(False)
   until(lambda:tree()[0].get('DESKTOP/A.TXT')==b'source document','wallpaper copy')
   assert tree()[0]['SOURCE/A.TXT']==b'source document'
   close() # automatic desktop transfer Files
   # Lower original Files to uncover A.TXT on wallpaper.
   move(300,112);button(True);move(300,310);button(False)
   time.sleep(1.2)
   # Move desktop A.TXT into OUT folder in original Files.
   g.call('input-send-event',{'events':[{'type':'key','data':{'down':True,'key':{'type':'qcode','data':'shift'}}}]})
   move(180,172);button(True);move(204,172);move(80,418);button(False)
   g.call('input-send-event',{'events':[{'type':'key','data':{'down':False,'key':{'type':'qcode','data':'shift'}}}]})
   until(lambda:tree()[0].get('SOURCE/OUT/A.TXT')==b'source document' and 'DESKTOP/A.TXT' not in tree()[0],'desktop Shift move to Files folder')
   close();time.sleep(1.2)
   # Desktop document opens associated Notepad, with transient Files launcher exiting.
   click(180,172)
   until(lambda:'path=/APPS/NOTEPAD/NOTEPAD.BIN' in log(),'desktop document association')
   until(lambda:log().count('Multiwindow: open pid=')>=4,'Notepad window')
   g.key('ctrl-end');g.type(' edited');g.key('ctrl-s')
   until(lambda:tree()[0].get('DESKTOP/NOTE.TXT')==b'desktop note edited','desktop Notepad edit/save');close()
   # Folder icon opens Files in that folder; create a file there.
   click(180,60);time.sleep(.6);g.key('ctrl-n');g.type('NEW.TXT\n');until(lambda:log().count('Multiwindow: open pid=')>=6,'new document editor');g.type('folder file');g.key('ctrl-s');time.sleep(.4)
   until(lambda:'DESKTOP/FOLDER/NEW.TXT' in tree()[0],'desktop folder navigation');close();close()
   time.sleep(1.2);g.call('screendump',{'filename':str(out/'desktop.png'),'format':'png'})
   click(756,576);g.wait_for('Desktop finished.');g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
  finally:
   g.call('screendump',{'filename':str(out/'last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'desktop-debug.log')
  # Reboot same disk and verify persistent data through mounted filesystem.
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image)
  try:g.command('cat /desktop/note.txt','desktop note edited');assert 'DESKTOP/FOLDER/NEW.TXT' in tree()[0]
  finally:g.close()
 print('PASS desktop files: wallpaper copy, desktop Shift-move into Files folder, document association/edit/save, folder navigation, reboot persistence, fsck')
if __name__=='__main__':main()
