"""Check bounded file APIs and heap-backed Notepad against a disposable disk."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args()
 out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='large-files-',dir=out) as tmp:
  d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
  package=d/'extra';(package/'apps').mkdir(parents=True)
  subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/large_file_check/main.rs'),'--format','elf','--output',str(package/'apps/large.elf')],check=True)
  (package/'applications.json').write_text(json.dumps([{'name':'large','file':'LARGE.ELF'}]));seed_apps(image,package)
  extra=['-vga',a.vga]
  if a.usb:extra+=['-drive',f'if=none,id=stick,file={image.as_posix()},format=raw','-device','piix3-usb-uhci,id=uhci','-device','usb-storage,bus=uhci.0,drive=stick,bootindex=1']
  def guest():return Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,attach_disk=not a.usb,extra_args=extra)
  g=guest()
  try:
   def log():return (d/'debug.log').read_text()
   def until(fn,msg):
    end=time.monotonic()+30
    while True:
     try:
      if fn():return
     except AssertionError:pass
     assert time.monotonic()<end,msg+'\n'+log()[-2000:]
     time.sleep(.04)
   def tree():return inspect_tree(image,2048)[0]
   def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
   baseline=frames();g.command('run large.elf','Large file API checks passed!');g.wait_for('User program exited with code 0.');assert frames()==baseline,'File API frame leak'
   original=tree()['BIG.TXT'];assert len(original)==65536
   g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'Desktop')
   pos=[400,300]
   def move(x,y):
    while pos!=[x,y]:
     dx=max(-80,min(80,x-pos[0]));dy=max(-80,min(80,y-pos[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pos[0]+=dx;pos[1]+=dy;time.sleep(.025)
   def click(x,y):
    move(x,y)
    for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
   def launch():
    names=sorted(k.split('/')[-1][:-4] for k in tree() if k.startswith('APPS/') and k.count('/')==1 and k.endswith(('.BIN','.ELF')));start=len(log());click(40,11);click(40,46+names.index('NOTEPAD')*24)
    until(lambda:re.search(r'Multiwindow: open pid=(\d+) ',log()[start:]),'Notepad open')
    m=re.search(r'Multiwindow: open pid=(\d+) handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log()[start:]);return int(m[1]),int(m[2],16),int(m[3]),int(m[5])+8,int(m[6])+34
   pid,addr,w,x,y=launch()
   def content():
    f=d/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {addr+w*32*4:#x} {w*352*4} "{f.as_posix()}"'});return f.read_bytes()
   def open_file(name):
    start=len(log());g.key('ctrl-o');time.sleep(.15);g.type('/'+name+'\n')
    # BIOS USB I/O can outlast a fixed delay; wait for the completed read/repaint.
    until(lambda:re.search(r'Syscall frame: nr=88 .*Syscall frame: nr=63 ',log()[start:],re.S),'Open/repaint '+name)
   def save():
    start=len(log());g.key('ctrl-s')
    until(lambda:re.search(r'Syscall frame: nr=89 .*Syscall frame: nr=63 ',log()[start:],re.S),'Save/repaint')
   open_file('big.txt');first=content();g.key('ctrl-end');time.sleep(.2);last=content();assert last!=first,'Large document did not navigate to EOF'
   g.type('x');save();assert tree()['BIG.TXT']==original,'Full document silently changed'
   g.key('backspace');g.type('z');save();until(lambda:tree()['BIG.TXT']==original[:-1]+b'z','64 KiB save')
   g.key('ctrl-z');g.key('ctrl-z');g.key('ctrl-y');g.key('ctrl-y');save();assert tree()['BIG.TXT']==original[:-1]+b'z','Multi-step undo/redo'
   # Manually scrolling must not snap back to the insertion point.
   click(x+632,y+40);time.sleep(.15);scrolled=content();assert scrolled!=last,'Scrollbar did not move';time.sleep(.2);assert content()==scrolled,'Scrollbar snapped back to caret'
   g.key('ctrl-end');time.sleep(.15);assert content()!=scrolled,'Keyboard did not follow caret'
   start=len(log());g.key('ctrl-shift-s');g.type('/copy.txt\n');until(lambda:re.search(r'Syscall frame: nr=89 .*Syscall frame: nr=63 ',log()[start:],re.S),'Save As/repaint');assert tree().get('COPY.TXT')==original[:-1]+b'z','Large Save As'
   open_file('binary.txt');save();assert tree()['COPY.TXT']==original[:-1]+b'z','Binary open discarded document'
   open_file('tabs.txt');save();assert tree()['COPY.TXT']==original[:-1]+b'z','Expanded oversized open discarded document'
   open_file('empty.txt');g.type('abc');g.key('ctrl-z');g.key('ctrl-z');g.key('ctrl-y');save();until(lambda:tree()['EMPTY.TXT']==b'ab','Empty file/edit history')
   g.call('screendump',{'filename':str(out/'large-notepad.png'),'format':'png'});g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={pid} code=0' in log(),'Notepad exit')
   click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'Notepad heap/surface frame leak'
   expected=tree()['COPY.TXT']
  finally:
   g.close();shutil.copyfile(d/'debug.log',out/'large-files-debug.log')
  g=guest()
  try:assert tree()['COPY.TXT']==expected and len(expected)==65536,'Cold boot persistence'
  finally:g.close()
  print('PASS 64 KiB file APIs, pointer/volume bounds, legacy compatibility, Notepad capacity/save/scroll/undo/redo, failed-open retention, cold persistence and frame balance',flush=True)
if __name__=='__main__':main()
