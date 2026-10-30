"""Exercise Files navigation, creation, deletion and opening actual filesystem apps."""
from desktop_controls import APPS_BUTTON,SHELL_BUTTON,app_card,task_button
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from seed_apps import seed_apps
from pathlib import Path
from array import array
from smoke import Guest,ROOT
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args()
 out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
 subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(ROOT/'target/font-dump.exe')],check=True);font=subprocess.check_output([str(ROOT/'target/font-dump.exe')]);glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
 with tempfile.TemporaryDirectory(prefix='files-',dir=out) as tmp:
  d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
  package=d/'extra';(package/'apps').mkdir(parents=True)
  subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/directory_check/main.rs'),'--output',str(package/'apps/dcheck.bin')],check=True);(package/'applications.json').write_text(json.dumps([{'name':'dcheck','file':'DCHECK.BIN'}]));seed_apps(image,package)
  extra=['-vga',a.vga]
  if a.usb:extra+=['-drive',f'if=none,id=stick,file={image.as_posix()},format=raw','-device','piix3-usb-uhci,id=uhci','-device','usb-storage,bus=uhci.0,drive=stick,bootindex=1']
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,attach_disk=not a.usb,extra_args=extra)
  try:
   def log():return (d/'debug.log').read_text()
   def until(fn,msg):
    end=time.monotonic()+30
    while True:
     # Inspecting a live FAT image may catch the two mirror writes in flight.
     # Require a consistent snapshot satisfying the predicate before continuing.
     try:
      if fn():return
     except AssertionError:pass
     assert time.monotonic()<end,msg+'\n'+log()[-1800:]
     time.sleep(.04)
   def mem(address,n):
    f=d/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {address:#x} {n} "{f.as_posix()}"'});return f.read_bytes()
   def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
   g.type('run dcheck\n');g.wait_for('Directory API checks passed!',timeout=180);g.wait_for('User program exited with code 0.');baseline=frames();g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'Desktop')
   pos=[400,300]
   def move(x,y):
    while pos!=[x,y]:
     dx=max(-80,min(80,x-pos[0]));dy=max(-80,min(80,y-pos[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pos[0]+=dx;pos[1]+=dy;time.sleep(.025)
   def click(x,y):
    move(x,y)
    for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.07)
   def launch(name):
    names=sorted(k.split('/')[-1][:-4] for k in inspect_tree(image,2048)[0] if k.startswith('APPS/') and k.count('/') in (1,2) and k.endswith(('.BIN','.ELF')));start=len(log());click(*APPS_BUTTON);click(*app_card(names.index(name)))
    until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name+r'\.BIN',log()[start:]),'Launch '+name);pid=int(re.search(r'Desktop: launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log()[start:],'Window '+name);return pid
   def window(pid):
    m=re.search(r'Multiwindow: open pid='+str(pid)+r' handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log());return int(m[1],16),int(m[2]),int(m[3])
   def status(pid):
    address,w,h=window(pid);data=array('I');data.frombytes(mem(address+w*384*4,w*16*4));return ''.join(glyphs.get(bytes(data[dy*w+col*8+dx]==0 for dy in range(16) for dx in range(8)),'?') for col in range(78))
   def selected(pid):
    address,w,h=window(pid)
    return any(int.from_bytes(mem(address+((80+row*16)*w+8)*4,4),'little')==0x153e8e for row in range(18))
   files=launch('FILES')
   # Files lists the actual FAT directory, not a static app catalog.
   def tree():return inspect_tree(image,2048)[0]
   def coords(pid):
    m=re.search(r'Multiwindow: open pid='+str(pid)+r' .*pos=(\d+),(\d+)',log());return int(m[1])+8,int(m[2])+34
   x,y=coords(files)
   def tool(n):click(x+n*70+24,y+12)
   until(lambda:selected(files),'Initial selection missing');click(x+500,y+300);until(lambda:not selected(files),'Empty space did not clear selection')
   g.key('delete');g.key('ret');g.key('f2');g.key('ctrl-c');until(lambda:'Select ' in status(files),'Deselected actions were enabled');assert log().count('Multiwindow: open')==1 and 'APPS' in inspect_tree(image,2048)[1]
   g.key('f5');until(lambda:not selected(files),'Refresh reselected a file');click(x+32,y+88);until(lambda:selected(files),'Could not reselect');click(x+636,y+88);until(lambda:not selected(files),'Gutter click did not clear selection');g.key('down');until(lambda:selected(files),'Keyboard did not restore selection');click(x+500,y+300);until(lambda:not selected(files),'Second blank click did not clear')
   tool(3);g.type('docs\n');until(lambda:'DOCS' in inspect_tree(image,2048)[1],'Create folder')
   # A folder used by another Files process must not be renamed underneath it.
   other=launch('FILES');ox,oy=coords(other);click(ox+32,oy+104);g.key('ret');until(lambda:'Folder opened.' in status(other),'Second Files folder');click(*task_button(0));click(x+32,y+104);g.key('f2');g.type('busy\n');until(lambda:'Folder is in use' in status(files),'Busy folder rename refusal')
   assert 'DOCS' in inspect_tree(image,2048)[1] and 'BUSY' not in inspect_tree(image,2048)[1],'Renamed an active folder'
   click(*task_button(1));g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={other}' in log(),'Close second Files')
   click(*task_button(0));g.key('f2');g.type('notes\n');until(lambda:'NOTES' in inspect_tree(image,2048)[1],'Rename folder');g.key('f2');g.type('docs\n');until(lambda:'DOCS' in inspect_tree(image,2048)[1],'Rename folder back')
   # Root contains APPS and DOCS; select DOCS and enter.
   g.key('ret');time.sleep(.2)
   tool(2);g.type('note.txt\n');until(lambda:'Desktop: terminal launch' in log(),'New file launch');until(lambda:log().count('Multiwindow: open')==3,'Notepad window')
   note=int(re.findall(r'Multiwindow: open pid=(\d+)',log())[-1]);g.type('from files');g.key('ctrl-s');until(lambda:tree().get('DOCS/NOTE.TXT')==b'from files','Notepad save/cwd');g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note}' in log(),'Notepad close')
   click(*task_button(0));g.key('f5');time.sleep(.2);click(x+32,y+88);tool(1);until(lambda:log().count('Multiwindow: open')==4,'Reopen text file');note=int(re.findall(r'Multiwindow: open pid=(\d+)',log())[-1]);g.key('ctrl-end');g.type('!');g.key('ctrl-s');until(lambda:tree().get('DOCS/NOTE.TXT')==b'from files!','Reopen content');g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note}' in log(),'Second Notepad close')
   # Rename, duplicate with Paste As, then move the duplicate into the parent.
   click(*task_button(0));g.key('f2');g.key('esc');assert tree().get('DOCS/NOTE.TXT')==b'from files!'
   g.key('f2');g.type('new.txt\n');until(lambda:tree().get('DOCS/NEW.TXT')==b'from files!' and 'DOCS/NOTE.TXT' not in tree(),'Rename file')
   g.key('ctrl-c');g.key('ctrl-v');g.type('copy.txt\n');until(lambda:tree().get('DOCS/COPY.TXT')==b'from files!','Paste As copy')
   click(x+32,y+104);g.key('ctrl-x');g.key('backspace');g.key('ctrl-v');until(lambda:tree().get('COPY.TXT')==b'from files!' and 'DOCS/COPY.TXT' not in tree(),'Move to parent')
   click(x+32,y+104);g.key('ret');time.sleep(.2);click(x+32,y+88);g.key('f2');g.type('note.txt\n');until(lambda:tree().get('DOCS/NOTE.TXT')==b'from files!','Rename back')
   click(*task_button(0));tool(4);g.key('n');assert tree().get('DOCS/NOTE.TXT')==b'from files!';tool(4);g.key('y');until(lambda:'DOCS/NOTE.TXT' not in tree(),'Delete file');tool(0);time.sleep(.2);click(x+32,y+104);tool(4);g.key('y');until(lambda:'DOCS' not in inspect_tree(image,2048)[1],'Remove folder')
   # Opening an executable starts a terminal, which hosts the text program.
   click(x+32,y+88);g.key('ret');time.sleep(.2)
   click(x+32,y+88);g.key('ret');time.sleep(.2);click(x+32,y+88);tool(1)
   until(lambda:'PTY: open' in log(),'Executable terminal');until(lambda:re.search(r'Desktop: terminal launch pid=\d+ name=HELLO.BIN',log()),'Executable child');until(lambda:log().count('Scheduler: exit')>=3,'Hello return');terminal=int(re.findall(r'Multiwindow: open pid=(\d+)',log())[-1]);time.sleep(.2);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={terminal}' in log(),'Terminal close');click(*task_button(0))
   # Drag the title bar right up to the taskbar; it must stop at y=24.
   move(x+180,y-18)
   g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':True}}]});time.sleep(.15);move(x+180,0);g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':False}}]});time.sleep(.2)
   g.call('screendump',{'filename':str(out/'files.png'),'format':'png'});g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={files} code=0' in log(),'Files close');click(*SHELL_BUTTON);g.wait_for('Desktop finished.');assert frames()==baseline,'Files frame leak';print('PASS Files: folders, create/open/save, rename/copy/move, empty-space deselection, confirmed delete, executable terminal and frame balance')
  finally:
   g.call('screendump',{'filename':str(out/'files-last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'files-debug.log')
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,attach_disk=not a.usb,extra_args=extra)
  try:g.command('cat copy.txt','from files!');assert inspect_tree(image,2048)[0]['COPY.TXT']==b'from files!'
  finally:g.close()

if __name__=='__main__':main()
