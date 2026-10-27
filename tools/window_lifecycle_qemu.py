"""Verify automatic terminal handoff, off-screen composition and minimizing."""
import argparse,json,re,shutil,tempfile,time
from seed_apps import seed_apps
from pathlib import Path
from array import array
from smoke import Guest,ROOT
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args()
 out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='window-life-',dir=out) as tmp:
  d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
  package=d/'extra';(package/'apps').mkdir(parents=True);(package/'apps/spin.bin').write_bytes(bytes.fromhex('ebfe'));(package/'applications.json').write_text(json.dumps([{'name':'spin','file':'SPIN.BIN'}]));seed_apps(image,package)
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
   baseline=frames();g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'Desktop')
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
   def rect(pid):
    m=re.search(r'Multiwindow: open pid='+str(pid)+r' .*pos=(\d+),(\d+)',log());return int(m[1]),int(m[2])
   def button(down):g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.1)
   def drag(x,y,tx,ty):move(x,y);button(True);move(tx,ty);button(False);time.sleep(.2)
   counter=launch('COUNTER');pad=launch('NOTEPAD');g.type('keep this unsaved');x,y=rect(pad);click(x+615,y+15);until(lambda:f'Multiwindow: minimized pid={pad}' in log(),'Minimize editor');assert f'Scheduler: exit pid={pad}' not in log()
   click(258,11);until(lambda:f'Multiwindow: restored pid={pad}' in log(),'Restore editor');g.key('ctrl-s');g.type('kept.txt\n');until(lambda:inspect_tree(image,2048)[0].get('KEPT.TXT')==b'keep this unsaved','Minimize lost document')
   x,y=rect(pad);drag(x+100,y+15,799,599);g.call('screendump',{'filename':str(out/'window-offscreen-right.png'),'format':'png'});assert 'Xinyi panic:' not in log()
   click(258,11);until(lambda:f'Multiwindow: restored pid={pad} pos=138,156' in log(),'Recover offscreen window');drag(138+500,156+15,0,120);g.call('screendump',{'filename':str(out/'window-offscreen-left.png'),'format':'png'});click(258,11);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={pad}' in log(),'Close editor')
   # A minimized counter keeps updating its retained surface, without keyboard focus.
   x,y=rect(counter);click(110,11);click(x+295,y+15);until(lambda:f'Multiwindow: minimized pid={counter}' in log(),'Minimize Counter');address,w,h=window(counter);before=mem(address,w*h*4);time.sleep(.35);assert before!=mem(address,w*h*4),'Minimized app stopped';click(110,11)
   # No program-name guesses: the first console call of any app opens a PTY.
   for name in ['HELLO','SNAKE','EDIT']:
    start=len(log());names=sorted(k.split('/')[-1][:-4] for k in inspect_tree(image,2048)[0] if k.startswith('APPS/') and k.count('/')==1 and k.endswith('.BIN'));click(40,11);click(40,46+names.index(name)*24)
    until(lambda:'Desktop: text attached' in log()[start:],'No terminal for '+name);terminal=int(re.search(r'Desktop: text attached pid=\d+ terminal=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={terminal}' in log()[start:],'No terminal window');assert 'Desktop: text foreground' not in log()
    if name=='SNAKE':g.key('esc')
    if name=='EDIT':g.key('ctrl-q')
    until(lambda:re.search(r'Scheduler: exit pid=\d+ code=0',log()[start:]),'Text app did not exit '+name);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={terminal}' in log(),'Terminal did not close '+name)
   click(110,11);g.key('q');until(lambda:f'Scheduler: exit pid={counter}' in log(),'Counter close');click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'Window lifecycle leaked frames';g.command('cat kept.txt','keep this unsaved');print('PASS desktop lifecycle: Apps to PTY, offscreen clipping/recovery, minimize/restore retains unsaved state, background progress and frame balance')
  finally:
   g.call('screendump',{'filename':str(out/'window-life-last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'window-life-debug.log')

if __name__=='__main__':main()
