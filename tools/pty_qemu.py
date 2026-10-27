"""Verify the shared shell and text programs inside graphical terminal sessions."""
import argparse,json,re,shutil,tempfile,time
from seed_apps import seed_apps
from pathlib import Path
from array import array
from smoke import Guest,ROOT
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args()
 out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='pty-',dir=out) as tmp:
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
   counter=launch('COUNTER');terminal=launch('TERMINAL');until(lambda:'PTY: open' in log(),'PTY not opened')
   font=mem(int(re.search(r'PTY: open .*font=([0-9a-f]+)',log())[1],16),4096)
   glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
   def screen(pid=terminal):
    address,w,h=window(pid);data=array('I');data.frombytes(mem(address,w*h*4));lines=[]
    for row in range(h//16):
     line=''
     for col in range(w//8):
      bg=data[row*16*w+col*8];bits=bytes(data[(row*16+y)*w+col*8+x]!=bg for y in range(16) for x in range(8));line+=glyphs.get(bits,'?')
     lines.append(line.rstrip())
    return '\n'.join(lines)
   def cmd(s,expected):
    g.type(s+'\n');until(lambda:expected in screen(),'Output '+expected+'\n'+screen())
   until(lambda:'xinyi:/' in screen(),'Prompt\n'+screen())
   cmd('version','Xinyi kernel version');cmd('uptime','Uptime:');cmd('help','install');cmd('clear','xinyi:/>');cmd('mkdir docs','Created');cmd('cd docs','xinyi:/DOCS>');cmd('touch note.txt','Created');cmd('write note.txt shared shell','Wrote');cmd('cat note.txt','shared shell')
   cmd('run hello','User program exited.');assert 'Desktop: text foreground' not in log()
   cmd('run snake','S N A K E');address,w,h=window(counter);before=mem(address,w*h*4);time.sleep(.35);assert mem(address,w*h*4)!=before,'Snake starved Counter';g.key('esc');until(lambda:'User program exited.' in screen(),'Snake return')
   cmd('run tetris','T E T R I S');g.key('esc');until(lambda:'User program exited.' in screen(),'Tetris return')
   cmd('edit note.txt','Xinyi Editor');g.key('end');g.type('!');g.key('ctrl-s');until(lambda:inspect_tree(image,2048)[0].get('DOCS/NOTE.TXT')==b'shared shell!','Editor save');g.key('ctrl-q');until(lambda:'User program exited.' in screen(),'Editor return')
   cmd('run notepad child.txt','Graphical application started.');until(lambda:log().count('Multiwindow: open')==3,'GUI detach');g.type('inherited');g.key('ctrl-s');until(lambda:inspect_tree(image,2048)[0].get('DOCS/CHILD.TXT')==b'inherited','GUI cwd');g.key('ctrl-q');time.sleep(.2)
   click(258,11);cmd('run snake','S N A K E');g.key('ctrl-c');until(lambda:'Interrupted.' in screen(),'Ctrl-C interrupt')
   g.call('screendump',{'filename':str(out/'pty.png'),'format':'png'});cmd('run spin','run spin');until(lambda:'name=spin' in log(),'Spin start');m=re.search(r'Multiwindow: open pid='+str(terminal)+r' .*pos=(\d+),(\d+)',log());tx,ty=int(m[1]),int(m[2]);click(tx+638,ty+15);time.sleep(.2);g.key('esc');time.sleep(.2);assert f'Scheduler: exit pid={terminal}' not in log();click(tx+638,ty+15);click(tx+638,ty+15);until(lambda:f'Scheduler: exit pid={terminal} code=0' in log(),'Terminal close');click(110,11);g.key('q');until(lambda:f'Scheduler: exit pid={counter} code=0' in log(),'Counter close');click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'PTY frame leak';g.command('cat /docs/note.txt','shared shell!');print('PASS PTY: normal shell, windowed Snake/editor, GUI detach/cwd, Ctrl-C and frame balance')
  finally:
   g.call('screendump',{'filename':str(out/'pty-last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'pty-debug.log')

if __name__=='__main__':main()
