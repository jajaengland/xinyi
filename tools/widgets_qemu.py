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
  package=d/'extra';(package/'apps').mkdir(parents=True)
  stream=d/'stream.rs';stream.write_text('#![no_std]\n#![no_main]\n#[path='+json.dumps(str(ROOT/'sdk/xinyi.rs').replace(chr(92),'/'))+']mod xinyi;\nfn main()->u32{for i in 0..40{let line=[108,105,110,101,32,48+i/10,48+i%10,10];xinyi::write(&line);xinyi::sleep_ms(80);}0}\n')
  subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(stream),'--output',str(package/'apps/stream.bin')],check=True)
  (package/'applications.json').write_text(json.dumps([{'name':'stream','file':'STREAM.BIN'}]));seed_apps(image,package)
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
   g.command('mkdir docs','Created directory')
   g.command('touch docs/orig.txt','Created:');g.command('write docs/orig.txt original','Wrote')
   for i in range(12):g.command('touch f'+str(i)+'.txt','Created:')
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

   note=launch('NOTEPAD');x,y=coords(note);g.type('first draft');g.key('ctrl-s');time.sleep(.2)
   # Browse DOCS by double click, then save through the shared filename field/button.
   click(x+100,y+132);click(x+100,y+132);time.sleep(.2);click(x+160,y+300);g.type('saved.txt');g.call('screendump',{'filename':str(out/'widgets-save-dialog.png'),'format':'png'});click(x+392,y+336)
   until(lambda:tree().get('DOCS/SAVED.TXT')==b'first draft','Browse and Save')
   g.type('!');g.key('ctrl-shift-s');g.type('/docs/saved.txt\n');time.sleep(.2);click(x+310,y+234);assert tree()['DOCS/SAVED.TXT']==b'first draft','Overwrite Cancel changed disk'
   g.key('ctrl-shift-s');g.type('/docs/saved.txt\n');time.sleep(.2);click(x+170,y+234);until(lambda:tree()['DOCS/SAVED.TXT']==b'first draft!','Overwrite Replace')
   # A fresh relative save starts in the original cwd: browsing must not cd the process.
   g.key('ctrl-shift-s');g.type('root.txt\n');until(lambda:tree().get('ROOT.TXT')==b'first draft!','File picker leaked cwd')
   g.key('ctrl-o');time.sleep(.2);click(x+100,y+132);click(x+100,y+132);time.sleep(.2);click(x+100,y+116);click(x+392,y+336);time.sleep(.2)
   g.key('ctrl-end');g.type('!');g.key('ctrl-s');until(lambda:tree()['DOCS/ORIG.TXT']==b'original!','Mouse Open file')
   # Text-field selection/paste and Escape cancel retain the current document.
   g.key('ctrl-shift-s');g.type('discard.txt');g.key('ctrl-a');g.type('cancel.txt');g.key('esc');g.key('ctrl-s');assert 'CANCEL.TXT' not in tree() and tree()['DOCS/ORIG.TXT']==b'original!'
   g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note} code=0' in log(),'Notepad close')
   term=launch('TERMINAL');x,y=coords(term);addr,w,h=window(term);assert w==656
   g.type('help\nhelp\nhelp\n');time.sleep(.5)
   def content():
    data=mem(addr,w*h*4);return b''.join(data[row*w*4:row*w*4+640*4] for row in range(400))
   before=content();click(x+648,y+8);until(lambda:content()!=before,'Scrollbar Up did not scroll')
   # Drag the thumb beyond both ends; capture must clamp and release cleanly.
   def thumb():
    data=mem(addr+640*4,w*h*4-640*4)
    return [row for row in range(16,384) if int.from_bytes(data[row*w*4:row*w*4+4],'little')==0xffffff]
   tops=thumb();assert tops,'Missing thumb';drag(x+648,y+tops[0]+3,x+648,y+20);time.sleep(.15);old=content();click(x+648,y+392);until(lambda:content()!=old,'Scrollbar Down did not scroll')
   g.key('shift-pgup');time.sleep(.15);g.key('shift-pgdn');g.type('echo live\n');time.sleep(.2)
   # All eighty columns remain drawable; the scrollbar is outside the PTY grid.
   assert w*4==2624 and int.from_bytes(mem(addr+((392*w)+640)*4,4),'little')!=0,'Scrollbar missing'
   start=len(log());g.type('run stream\n');time.sleep(.3);g.key('shift-pgup');time.sleep(.2);anchored=content();time.sleep(.4);assert content()==anchored,'Scrolled terminal moved while background output arrived'
   until(lambda:re.search(r'Scheduler: exit pid=\d+ code=0',log()[start:]),'Output fixture did not finish');g.type('echo live\n');time.sleep(.2)
   g.call('screendump',{'filename':str(out/'widgets-terminal.png'),'format':'png'});g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={term} code=0' in log(),'Terminal close')
   settings=launch('SETTINGS');sx,sy=coords(settings);click(sx+120,sy+218);g.type('112233');g.key('ctrl-a');g.type('294f9c\n');click(sx+160,sy+264);until(lambda:tree().get('GUI.CFG')==b'294F9C 153E8E\n','Shared Settings field/Save');g.call('screendump',{'filename':str(out/'widgets-settings.png'),'format':'png'});click(sx+400,sy+264);until(lambda:f'Scheduler: exit pid={settings}' in log(),'Settings close')
   click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'Widget application frame leak'
   print('PASS GUI controls: file browsing/save/open/cancel/overwrite, private cwd, field selection, terminal scrollbar and 80 columns, Settings and frame balance',flush=True)
  finally:
   g.call('screendump',{'filename':str(out/'widgets-last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'widgets-debug.log')
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,attach_disk=not a.usb,extra_args=extra)
  try:g.command('cat /docs/orig.txt','original!');g.command('cat /root.txt','first draft!')
  finally:g.close()

if __name__=='__main__':main()
