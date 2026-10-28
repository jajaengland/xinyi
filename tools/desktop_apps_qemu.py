"""Exercise concurrent desktop apps, task switching and Notepad persistence."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from seed_apps import seed_apps
from pathlib import Path
from array import array
from smoke import Guest,ROOT
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args()
 output=ROOT/'target/smoke';output.mkdir(exist_ok=True)
 subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(ROOT/'target/font-dump.exe')],check=True)
 font=subprocess.check_output([str(ROOT/'target/font-dump.exe')]);glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
 with tempfile.TemporaryDirectory(prefix='desktop-apps-',dir=output) as temp:
  directory=Path(temp);image=directory/'system.img';shutil.copyfile(a.disk,image)
  package=directory/'extra';(package/'apps').mkdir(parents=True)
  subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/launcher_check/main.rs'),'--output',str(package/'apps/custom.bin')],check=True)
  (package/'apps/crash.bin').write_bytes(bytes.fromhex('0f0b'));(package/'apps/spin.bin').write_bytes(bytes.fromhex('ebfe'))
  (package/'applications.json').write_text(json.dumps([{'name':n,'file':n.upper()+'.BIN'} for n in ['custom','crash','spin']]))
  seed_apps(image,package)
  extra=['-vga',a.vga]
  if a.usb:extra+=['-drive',f'if=none,id=stick,file={image.as_posix()},format=raw','-device','piix3-usb-uhci,id=uhci','-device','usb-storage,bus=uhci.0,drive=stick,bootindex=1']
  qemu=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
  g=Guest(qemu,None,directory,disk=image,attach_disk=not a.usb,extra_args=extra)
  try:
   def log():return (directory/'debug.log').read_text()
   def until(test,message):
    end=time.monotonic()+30
    while not test():
     assert time.monotonic()<end,message+'\n'+log()[-1800:];time.sleep(.03)
   def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
   g.command('touch /apps/bad.bin','Created:');g.command('touch custom.bin','Created:')
   baseline=frames();g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'Desktop missing')
   pos=[400,300]
   def move(x,y):
    while pos!=[x,y]:
     dx=max(-80,min(80,x-pos[0]));dy=max(-80,min(80,y-pos[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pos[0]+=dx;pos[1]+=dy;time.sleep(.025)
   def click(x,y):
    move(x,y)
    for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.06)
   def select(name):
    names=sorted(k.split('/')[-1][:-4] for k in inspect_tree(image,2048)[0] if k.startswith('APPS/') and k.count('/')==1 and k.endswith(('.BIN','.ELF')))
    offset=len(log());click(40,11);click(40,30+4+names.index(name)*24+12);return offset
   def launch(name,window=True):
    offset=select(name)
    until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/'+name+r'\.BIN',log()[offset:]),'Launch '+name)
    pid=int(re.search(r'Desktop: launch pid=(\d+)',log()[offset:])[1])
    if window:until(lambda:f'Multiwindow: open pid={pid} ' in log()[offset:],'Missing window '+name)
    return pid
   def task(slot):click(110+slot*148,11)
   def exited(pid):return f'Scheduler: exit pid={pid} code=0' in log()
   def file(name):
    try:return inspect_tree(image,2048)[0].get(name)
    except AssertionError:return None
   def memory(address,length):
    path=directory/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {address:#x} {length} "{path.as_posix()}"'});return path.read_bytes()
   def count(pid):
    m=re.search(r'Multiwindow: open pid='+str(pid)+r' handle=\d+ surface=([0-9a-f]+) client=(\d+)',log());surface=int(m[1],16);width=int(m[2]);data=array('I');data.frombytes(memory(surface+40*width*4,width*16*4))
    text=''.join(glyphs.get(bytes(data[row*width+8+col*8+dx]==0 for row in range(16) for dx in range(8)),'?') for col in range(30));m=re.search(r'Count: (\d+)',text);return int(m[1]) if m else -1
   counter=launch('COUNTER');until(lambda:count(counter)>=3,'Counter stopped')
   offset=select('BAD');until(lambda:'Desktop: launch failed /APPS/BAD.BIN' in log()[offset:],'Empty program did not fail cleanly')
   first=launch('NOTEPAD');g.type('hello desktop');g.key('ctrl-s');g.type('desk.txt\n');until(lambda:file('DESK.TXT')==b'hello desktop','First Notepad save failed')
   before=count(counter);second=launch('NOTEPAD');g.type('second instance');g.key('ctrl-s');g.type('two.txt\n');until(lambda:file('TWO.TXT')==b'second instance','Second Notepad save failed');assert count(counter)>before
   task(1);g.type('!');g.key('ctrl-s');until(lambda:file('DESK.TXT')==b'hello desktop!','Task switch lost editor state');assert file('TWO.TXT')==b'second instance'
   third=launch('COUNTER');until(lambda:count(third)>=2,'Fourth app stopped')
   # Full desktop must reject a fifth launch without closing any running app.
   before_launches=log().count('Desktop: launch pid=');select('COUNTER');time.sleep(.25);assert log().count('Desktop: launch pid=')==before_launches
   g.call('screendump',{'filename':str(output/f'desktop-apps-{a.vga}.png'),'format':'png'})
   task(3);g.key('q');until(lambda:exited(third),'Close fourth app')
   task(1);g.key('ctrl-q');until(lambda:exited(first),'First Notepad did not close')
   task(2);g.type(' unsaved');click(768,11);time.sleep(.1);assert 'Desktop finished.' not in g.screen(),'Exit discarded running apps'
   task(2);g.key('ctrl-q');time.sleep(.15);g.key('esc');time.sleep(.15);assert not exited(second),'Escape did not cancel close prompt'
   g.key('ctrl-q');g.key('d');until(lambda:exited(second),'Second Notepad discard/close failed')
   before=count(counter)
   spin=launch('SPIN',False);task(1);g.key('ctrl-alt-k');until(lambda:f'Scheduler: forced exit pid={spin}' in log(),'Windowless app could not be stopped');until(lambda:count(counter)>before,'Windowless hog starved counter')
   crash=launch('CRASH',False);until(lambda:f'Scheduler: fault pid={crash} vector=6' in log(),'Windowless fault did not recover')
   def text_app(name,title=None):
    start=len(log());pid=launch(name,False);until(lambda:re.search(r'Desktop: text attached pid='+str(pid)+r' terminal=(\d+)',log()[start:]),'Text terminal '+name)
    terminal=int(re.search(r'Desktop: text attached pid='+str(pid)+r' terminal=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={terminal}' in log()[start:],'Terminal window '+name)
    if name=='SNAKE':g.key('esc')
    until(lambda:exited(pid),'Text exit '+name);g.key('ctrl-q');until(lambda:exited(terminal),'Terminal close '+name)
   text_app('CUSTOM');assert file('APPS/ADDED.BIN')==file('APPS/HELLO.BIN');text_app('ADDED');text_app('HELLO');text_app('SNAKE');until(lambda:count(counter)>before,'Counter stopped during text programs')
   task(0);g.key('c');until(lambda:f'Scheduler: fault pid={counter} vector=6' in log(),'Counter fault did not recover');click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'Desktop apps leaked frames'
   assert file('DESK.TXT')==b'hello desktop!' and file('TWO.TXT')==b'second instance'
   g.command('echo desktop survived','\ndesktop survived\n')
   print(f'PASS concurrent desktop ({a.vga}, usb={a.usb}): dynamic launch, taskbar focus, two independent Notepads/saves, counter progress, four-app limit, safe Exit/Escape, close/fault isolation, text/game terminal windows and frame balance',flush=True)
  finally:
   g.call('screendump',{'filename':str(output/f'desktop-apps-last-{a.vga}.png'),'format':'png'});g.close();shutil.copyfile(directory/'debug.log',output/f'desktop-apps-{a.vga}-debug.log')
  # A new VM verifies that files reached the filesystem across a cold boot.
  g=Guest(qemu,None,directory,disk=image,attach_disk=not a.usb,extra_args=extra)
  try:g.command('cat desk.txt','hello desktop!');g.command('cat two.txt','second instance')
  finally:g.close()

if __name__=='__main__':main()
