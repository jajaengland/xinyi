"""Exercise shared multi-selection transfers, context menus, folders and appearance persistence."""
import argparse,json,re,shutil,tempfile,time
from seed_apps import seed_apps
from pathlib import Path
from array import array
import subprocess,sys
from smoke import Guest,ROOT
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args()
 out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
 subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(ROOT/'target/font-dump.exe')],check=True);font=subprocess.check_output([str(ROOT/'target/font-dump.exe')]);glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
 with tempfile.TemporaryDirectory(prefix='window-life-',dir=out) as tmp:
  d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
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
   for i in range(24):g.command("touch f"+str(i)+".txt","Created:")
   baseline=frames();g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'Desktop')
   pos=[400,300]
   def move(x,y):
    while pos!=[x,y]:
     dx=max(-80,min(80,x-pos[0]));dy=max(-80,min(80,y-pos[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pos[0]+=dx;pos[1]+=dy;time.sleep(.025)
   def click(x,y):
    move(x,y)
    for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.07)
   def launch(name):
    names=sorted(k.split('/')[-1][:-4] for k in inspect_tree(image,2048)[0] if k.startswith('APPS/') and k.count('/')==1 and k.endswith(('.BIN','.ELF')));start=len(log());click(40,11);click(40,46+names.index(name)*24)
    until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/'+name+r'\.BIN',log()[start:]),'Launch '+name);pid=int(re.search(r'Desktop: launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log()[start:],'Window '+name);return pid
   def window(pid):
    m=re.search(r'Multiwindow: open pid='+str(pid)+r' handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log());return int(m[1],16),int(m[2]),int(m[3])
   def rect(pid):
    m=re.search(r'Multiwindow: open pid='+str(pid)+r' .*pos=(\d+),(\d+)',log());return int(m[1]),int(m[2])
   def button(down):g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.1)
   def drag(x,y,tx,ty):move(x,y);button(True);move(tx,ty);button(False);time.sleep(.2)
   def status(pid):
    address,w,h=window(pid);data=array('I');data.frombytes(mem(address+w*384*4,w*16*4));return ''.join(glyphs.get(bytes(data[dy*w+col*8+dx]==0 for dy in range(16) for dx in range(8)),'?') for col in range(78))
   def coords(pid):
    x,y=rect(pid);return x+8,y+34
   def tree():return inspect_tree(image,2048)[0]
   def chosen(pid,row):
    addr,w,h=window(pid);return int.from_bytes(mem(addr+((80+row*16)*w+8)*4,4),'little')==0x153e8e
   def modifier(name,down):g.call('input-send-event',{'events':[{'type':'key','data':{'down':down,'key':{'type':'qcode','data':name}}}]});time.sleep(.08)
   def right(x,y):
    move(x,y)
    for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'right','down':down}}]});time.sleep(.08)

   def current(pid):
    matches=re.findall(r'Multiwindow: resized pid='+str(pid)+r' surface=([0-9a-f]+) client=(\d+)x(\d+)',log())
    if matches:
     m=matches[-1];return int(m[0],16),int(m[1]),int(m[2])
    return window(pid)
   def position_top(pid):
    rx,ry=rect(pid);drag(rx+40,ry+12,40,36);time.sleep(.15)
   def resize(pid,width,height):
    _,w,h=current(pid);move(w+10,24+h+32);button(True);move(width+10,24+height+32);time.sleep(.1)
    assert current(pid)[1:]==(w,h),'Surface changed before resize acknowledgement'
    button(False);until(lambda:current(pid)[1:]==(width,height),'Resize '+str((width,height)))
   def row(pid,y,n):
    address,w,h=current(pid);data=array('I');data.frombytes(mem(address+y*w*4,w*16*4));return ''.join(glyphs.get(bytes(data[dy*w+col*8+dx]==0 for dy in range(16) for dx in range(8)),'?') for col in range(n))
   note=launch('NOTEPAD');g.type('x'*100);position_top(note);resize(note,768,528);until(lambda:row(note,32,94)=='x'*94,'Notepad wide wrapping');g.key('ctrl-s');g.type('wide.txt\n');until(lambda:tree().get('WIDE.TXT')==b'x'*100,'Save after resize');resize(note,640,400);until(lambda:row(note,32,78)=='x'*78,'Notepad shrink wrapping');g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note} code=0' in log(),'Notepad close')
   files=launch('FILES');position_top(files);resize(files,768,528);click(40,24+34+80+20*16+8);addr,w,h=current(files);until(lambda:int.from_bytes(mem(addr+((80+20*16)*w+8)*4,4),'little')==0x153e8e,'Files did not expose additional rows');resize(files,640,400);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={files} code=0' in log(),'Files close')
   term=launch('TERMINAL');position_top(term);g.type('help\nhelp\n');time.sleep(.3);resize(term,656,528);g.type('help\n');time.sleep(.3);addr,w,h=current(term)
   move(8,24+34+32*16+8);button(True);move(48,24+34+32*16+8);button(False);until(lambda:int.from_bytes(mem(addr+32*16*w*4,4),'little')==0x0000aa,'Terminal last-row selection did not render');g.key('ctrl-shift-c');resize(term,656,400);g.type('echo resized\n');time.sleep(.2)
   g.call('screendump',{'filename':str(out/'resize-terminal.png'),'format':'png'});g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={term} code=0' in log(),'Terminal close');click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'Resize leaked frames';g.command('cat wide.txt','x'*60)
   print('PASS resize: deferred allocation, Notepad reflow/save, Files extra rows, taller terminal including row 33, shrink/selection and frame balance',flush=True)
  finally:
   g.call('screendump',{'filename':str(out/'resize-last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'resize-debug.log')
if __name__=='__main__':main()
