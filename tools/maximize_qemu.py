"""Verify maximize/restore placement, app resize limits, input and frame ownership."""
from desktop_controls import APPS_BUTTON,SHELL_BUTTON,app_card,task_button
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
 with tempfile.TemporaryDirectory(prefix='maximize-',dir=out) as tmp:
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
   def pixel(x,y):
    if a.vga=='std':
     device=next(dev for dev in g.call('query-pci')[0]['devices'] if dev.get('id',{}).get('vendor')==0x1234)
     base=next(region['address'] for region in device['regions'] if region['bar']==0);pitch=3200;size=4
    else:
     mode=re.findall(r'BIOS VBE mode [0-9a-f]+ base=([0-9a-f]+) pitch=(\d+) bpp=(\d+)',log())[-1]
     base=int(mode[0],16);pitch=int(mode[1]);size=int(mode[2])//8
    return int.from_bytes(mem(base+y*pitch+x*size,size),'little')
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
    names=sorted(k.split('/')[-1][:-4] for k in inspect_tree(image,2048)[0] if k.startswith('APPS/') and k.count('/') in (1,2) and k.endswith(('.BIN','.ELF')));start=len(log());click(*APPS_BUTTON);click(*app_card(names.index(name)))
    until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name+r'\.BIN',log()[start:]),'Launch '+name);pid=int(re.search(r'Desktop: launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log()[start:],'Window '+name);return pid
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
   def placement(pid):
    matches=re.findall(r'Multiwindow: resized pid='+str(pid)+r' .*pos=(-?\d+),(-?\d+) maximized=(true|false)',log())
    if matches:
     rx,ry,m=matches[-1];_,w,h=current(pid);return int(rx),int(ry),800 if m=='true' else w+16,544 if m=='true' else h+38,m=='true'
    rx,ry=rect(pid);_,w,h=window(pid);return rx,ry,w+16,h+38,False
   def changes(pid):return log().count(f'Multiwindow: resized pid={pid} ')
   def toggle(pid,double=False):
    rx,ry,ow,oh,m=placement(pid);before=changes(pid)
    if double:
     click(rx+110,ry+15);click(rx+110,ry+15)
    else:click(rx+ow-42,ry+15)
    until(lambda:changes(pid)>before,'Maximize/restore acknowledgement')
    return placement(pid)
   note=launch('NOTEPAD');original=placement(note)
   rx,ry,ow,oh,m=original
   until(lambda:pixel(rx+ow-46,ry+13) in (0xffffff,0xffff),'Maximize icon missing before pointer hover')
   g.type('a'*100)
   # Chrome button capture cancels if released elsewhere.
   rx,ry,ow,oh,m=original;before=changes(note);move(rx+ow-42,ry+15);button(True);move(rx+250,ry+50);button(False);assert changes(note)==before,'Cancelled maximize changed window'
   assert toggle(note)==(0,0,800,544,True);assert current(note)[1:]==(792,512)
   # Input uses the centered client, not the full-frame origin.
   click(4+24,28+40);g.type('g');g.key('ctrl-s');g.type('max.txt\n');until(lambda:tree().get('MAX.TXT')==b'aaa'+b'g'+b'a'*97,'Maximized Notepad coordinates/save')
   click(734,15);until(lambda:f'Multiwindow: minimized pid={note}' in log(),'Maximized minimize');click(*task_button(0));until(lambda:f'Multiwindow: restored pid={note} pos=0,0' in log(),'Maximized taskbar restore')
   assert toggle(note)==original,'Normal placement was not restored exactly'
   assert toggle(note,True)==(0,0,800,544,True);assert toggle(note,True)==original,'Double-click restore'
   # At maximum client size, toggling placement reuses the existing surface.
   rx,ry,ow,oh,m=placement(note);drag(rx+100,ry+15,100,15);drag(650,432,799,536);until(lambda:current(note)[1:]==(792,512),'Manual maximum size');surface=current(note)[0]
   assert toggle(note)==(0,0,800,544,True);assert current(note)[0]==surface,'Same-size maximize reallocated surface'
   assert toggle(note)==(0,0,808,550,False);assert current(note)[0]==surface,'Same-size restore reallocated surface'
   g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={note} code=0' in log(),'Notepad close')
   files=launch('FILES');old=placement(files);assert toggle(files)==(0,0,800,544,True);assert current(files)[1:]==(792,512)
   # Row twenty is newly visible in the expanded client.
   click(4+40,28+80+20*16+8);addr,w,h=current(files);until(lambda:int.from_bytes(mem(addr+((80+20*16)*w+8)*4,4),'little')==0x153e8e,'Maximized Files row input')
   addr,w,h=current(files)
   until(lambda:int.from_bytes(mem(addr+(16*w+w-8)*4,4),'little')==0xc4c4c4,'Files toolbar does not fill expanded width')
   g.call('screendump',{'filename':str(out/'maximize-files.png'),'format':'png'})
   assert toggle(files)==old
   assert toggle(files)==(0,0,800,544,True)
   click(4+w-20,28+16);until(lambda:f'Scheduler: exit pid={files} code=0' in log(),'Expanded Files Quit button hit target')
   term=launch('TERMINAL');old=placement(term);g.type('echo normal\n');assert toggle(term)==(0,0,800,544,True);assert current(term)[1:]==(792,512),'Terminal client did not expand with the frame'
   until(lambda:pixel(5,535)==0 and pixel(770,535)==0,'Maximized client must reach within four pixels of the screen edges')
   addr,w,h=current(term)
   until(lambda:int.from_bytes(mem(addr+(300*w+w-8)*4,4),'little')!=0,'Terminal scrollbar did not move to the right edge')
   # The new bottom row must support selection, not just paint background.
   move(4,532);button(True);move(44,532);button(False)
   until(lambda:int.from_bytes(mem(addr+31*16*w*4,4),'little')==0x0000aa,'Terminal row 32 selection did not render')
   g.type('echo maximized\n');time.sleep(.2);g.call('screendump',{'filename':str(out/'maximize-terminal.png'),'format':'png'});assert toggle(term)==old;g.type('echo restored\n');time.sleep(.2);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={term} code=0' in log(),'Terminal close')
   # Fixed-size legacy apps keep their existing title controls.
   counter=launch('COUNTER');rx,ry,ow,oh,m=placement(counter);before=changes(counter);click(rx+80,ry+15);click(rx+80,ry+15);assert changes(counter)==before;g.key('q');until(lambda:f'Scheduler: exit pid={counter} code=0' in log(),'Counter close')
   click(*SHELL_BUTTON);g.wait_for('Desktop finished.');assert frames()==baseline,'Maximize/restore leaked frames'
   print('PASS maximize/restore: button capture, double-click, exact previous placement, bottom taskbar, centered client input/save, minimize/taskbar restore, initial chrome redraw, responsive Files toolbar/rows, expanded Terminal background/scrollbar, legacy apps and frame balance',flush=True)
  finally:
   g.call('screendump',{'filename':str(out/'maximize-last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'maximize-debug.log')
if __name__=='__main__':main()
