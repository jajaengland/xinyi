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
   g.command('mkdir src','Created directory');g.command('mkdir dst','Created directory');g.command('mkdir src/folder','Created directory');g.command('mkdir src/folder/sub','Created directory')
   for name,value in [('a.txt','alpha'),('b.txt','beta'),('c.txt','gamma'),('folder/sub/n.txt','nested')]:g.command('touch src/'+name,'Created:');g.command('write src/'+name+' '+value,'Wrote')
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
   first=launch('FILES');x,y=coords(first);click(x+32,y+104);g.key('ret');time.sleep(.2)
   click(x+32,y+104);modifier('ctrl',True);click(x+32,y+120);modifier('ctrl',False);until(lambda:chosen(first,1) and chosen(first,2),'Ctrl selection')
   click(x+32,y+104);modifier('shift',True);click(x+32,y+136);modifier('shift',False);until(lambda:all(chosen(first,i) for i in range(1,4)),'Shift range')
   modifier('ctrl',True);click(x+32,y+120);modifier('ctrl',False);until(lambda:chosen(first,1) and not chosen(first,2) and chosen(first,3),'Ctrl toggle')
   right(x+32,y+104);click(x+60,y+104+4+2*22+10);until(lambda:'Copy selected.' in status(first),'Stage copy: '+status(first));g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={first}' in log(),'Source close')
   other=launch('FILES');x,y=coords(other);click(x+32,y+120);g.key('ret');time.sleep(.2);right(x+300,y+300);click(x+330,y+238+4+4*22+10)
   until(lambda:tree().get('DST/A.TXT')==b'alpha' and tree().get('DST/C.TXT')==b'gamma','Cross-window copy');assert 'DST/B.TXT' not in tree()
   g.key('backspace');time.sleep(.2);click(x+32,y+104);g.key('ret');time.sleep(.2);click(x+32,y+88);g.key('ctrl-x');g.key('backspace');time.sleep(.2);click(x+32,y+120);g.key('ret');time.sleep(.2);g.key('ctrl-v')
   until(lambda:tree().get('DST/FOLDER/SUB/N.TXT')==b'nested' and 'SRC/FOLDER/SUB/N.TXT' not in tree(),'Recursive folder move')
   click(x+32,y+88);modifier('ctrl',True);click(x+32,y+104);modifier('ctrl',False);g.key('delete');g.key('n');assert tree().get('DST/A.TXT')==b'alpha';g.key('delete');g.key('y');until(lambda:'DST/A.TXT' not in tree() and 'DST/C.TXT' not in tree(),'Batch delete');assert tree()['DST/FOLDER/SUB/N.TXT']==b'nested'
   settings=launch('SETTINGS');sx,sy=coords(settings)
   # Exact custom RGB entry for desktop, then title bar; Save also applies.
   click(sx+120,sy+218);g.type('294f9c\n');click(sx+50,sy+80);click(sx+120,sy+218);g.type('526a83\n');click(sx+160,sy+264)
   until(lambda:tree().get('GUI.CFG')==b'294F9C 526A83\n','Settings save')
   if a.vga=='std':
    dev=next(v for v in g.call('query-pci')[0]['devices'] if v.get('id',{}).get('vendor')==0x1234);base=next(v['address'] for v in dev['regions'] if v['bar']==0);pitch=3200;size=4
   else:
    m=re.findall(r'BIOS VBE mode [0-9a-f]+ base=([0-9a-f]+) pitch=(\d+) bpp=(\d+)',log())[-1];base,pitch,size=int(m[0],16),int(m[1]),int(m[2])//8
   def pixel(px,py):return int.from_bytes(mem(base+py*pitch+px*size,size),'little')
   def native(rgb):return ((rgb>>19&31)<<11|(rgb>>10&63)<<5|(rgb>>3&31)) if size==2 else rgb
   until(lambda:pixel(780,580)==native(0x294f9c),'Desktop colour not applied');tx,ty=rect(settings);until(lambda:pixel(tx+400,ty+15)==native(0x526a83),'Title colour not applied')
   g.call('screendump',{'filename':str(out/'files-settings.png'),'format':'png'});click(sx+400,sy+264);until(lambda:f'Scheduler: exit pid={settings}' in log(),'Settings close');click(110,11);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={other}' in log(),'Files close');click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'Files/settings leaked frames'
   print('PASS multi-selection, context menus, shared file clipboard after source close, recursive move, batch delete/cancel, theme save/apply and frame balance',flush=True)
  finally:
   g.call('screendump',{'filename':str(out/'files-settings-last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'files-settings-debug.log')
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,attach_disk=not a.usb,extra_args=extra)
  try:
   g.command('cat /dst/folder/sub/n.txt','nested');g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'Cold desktop');g.call('screendump',{'filename':str(out/'files-settings-cold.png'),'format':'png'});assert pixel(780,580)==native(0x294f9c),'Saved theme lost after cold boot';g.key('esc');g.wait_for('Desktop finished.')
  finally:g.close()

if __name__=='__main__':main()
