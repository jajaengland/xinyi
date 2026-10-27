"""Exercise Files navigation, creation, deletion and opening actual filesystem apps."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from seed_apps import seed_apps
from pathlib import Path
from array import array
from smoke import Guest,ROOT
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args()
 out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
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
    names=sorted(k.split('/')[-1][:-4] for k in inspect_tree(image,2048)[0] if k.startswith('APPS/') and k.count('/')==1 and k.endswith('.BIN'));start=len(log());click(40,11);click(40,46+names.index(name)*24)
    until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/'+name+r'\.BIN',log()[start:]),'Launch '+name);pid=int(re.search(r'Desktop: launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log()[start:],'Window '+name);return pid
   def window(pid):
    m=re.search(r'Multiwindow: open pid='+str(pid)+r' handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log());return int(m[1],16),int(m[2]),int(m[3])
   files=launch('FILES')
   # Files lists the actual FAT directory, not a static app catalog.
   def tree():return inspect_tree(image,2048)[0]
   def coords(pid):
    m=re.search(r'Multiwindow: open pid='+str(pid)+r' .*pos=(\d+),(\d+)',log());return int(m[1])+8,int(m[2])+34
   x,y=coords(files)
   def tool(n):click(x+n*70+24,y+12)
   tool(3);g.type('docs\n');until(lambda:'DOCS' in inspect_tree(image,2048)[1],'Create folder')
   # Root contains APPS and DOCS; select DOCS and enter.
   click(x+32,y+104);g.key('ret');time.sleep(.2)
   tool(2);g.type('note.txt\n');until(lambda:'Desktop: terminal launch' in log(),'New file launch');until(lambda:log().count('Multiwindow: open')==2,'Notepad window')
   note=int(re.findall(r'Multiwindow: open pid=(\d+)',log())[-1]);g.type('from files');g.key('ctrl-s');until(lambda:tree().get('DOCS/NOTE.TXT')==b'from files','Notepad save/cwd');g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note}' in log(),'Notepad close')
   click(110,11);g.key('f5');time.sleep(.2);tool(1);until(lambda:log().count('Multiwindow: open')==3,'Reopen text file');note=int(re.findall(r'Multiwindow: open pid=(\d+)',log())[-1]);g.key('ctrl-end');g.type('!');g.key('ctrl-s');until(lambda:tree().get('DOCS/NOTE.TXT')==b'from files!','Reopen content');g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note}' in log(),'Second Notepad close')
   click(110,11);tool(4);g.key('n');assert tree().get('DOCS/NOTE.TXT')==b'from files!';tool(4);g.key('y');until(lambda:'DOCS/NOTE.TXT' not in tree(),'Delete file');tool(0);time.sleep(.2);click(x+32,y+104);tool(4);g.key('y');until(lambda:'DOCS' not in inspect_tree(image,2048)[1],'Remove folder')
   # Opening an executable starts a terminal, which hosts the text program.
   click(x+32,y+88);g.key('ret');time.sleep(.2)
   names=[k.split('/')[-1] for k in tree() if k.startswith('APPS/') and k.count('/')==1];index=names.index('HELLO.BIN');click(x+32,y+80+index*16+8);tool(1)
   until(lambda:'PTY: open' in log(),'Executable terminal');until(lambda:re.search(r'Desktop: terminal launch pid=\d+ name=HELLO.BIN',log()),'Executable child');until(lambda:log().count('Scheduler: exit')>=3,'Hello return');terminal=int(re.findall(r'Multiwindow: open pid=(\d+)',log())[-1]);time.sleep(.2);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={terminal}' in log(),'Terminal close');click(110,11)
   # Drag the title bar right up to the taskbar; it must stop at y=24.
   move(x+180,y-18)
   g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':True}}]});time.sleep(.15);move(x+180,0);g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':False}}]});time.sleep(.2)
   g.call('screendump',{'filename':str(out/'files.png'),'format':'png'});g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={files} code=0' in log(),'Files close');click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'Files frame leak';print('PASS Files: folders, create/open/save, confirmed delete, executable terminal and frame balance')
  finally:
   g.call('screendump',{'filename':str(out/'files-last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'files-debug.log')

if __name__=='__main__':main()
