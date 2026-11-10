"""Verify wallpaper Settings, placement, persistence, failure recovery and frame cleanup."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from PIL import Image
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);a=p.parse_args();out=ROOT/'target/wallpaper';out.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='check-',dir=out) as temporary:
  d=Path(temporary);image=d/'system.img';shutil.copyfile(a.disk,image)
  fixture=d/'fixture';(fixture/'apps').mkdir(parents=True)
  picture=Image.new('RGB',(4,2));colors=[(255,0,0),(0,255,0),(0,0,255),(255,255,0)]
  for y in range(2):
   for x,color in enumerate(colors):picture.putpixel((x,y),color)
  picture.save(fixture/'apps/wall.bmp');(fixture/'apps/bad.bmp').write_bytes(bytes(54))
  (fixture/'applications.json').write_text(json.dumps([{'file':'WALL.BMP'},{'file':'BAD.BMP'}]));seed_apps(image,fixture,root_name='PICS')
  helper=d/'helper';(helper/'apps').mkdir(parents=True);subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/wallpaper_check/main.rs'),'--output',str(helper/'apps/wpcheck.bin')],check=True)
  (helper/'applications.json').write_text(json.dumps([{'file':'WPCHECK.BIN'}]));seed_apps(image,helper,root_name='TOOLS')
  subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(d/'font.exe')],check=True);font=subprocess.check_output([str(d/'font.exe')])
  qemu=r'C:\Program Files\qemu\qemu-system-i386.exe';g=Guest(qemu,None,d,disk=image)
  try:
   def log():return (g.directory/'debug.log').read_text(errors='replace')
   def until(fn,message,timeout=45):
    end=time.monotonic()+timeout
    while True:
     try:
      if fn():return
     except AssertionError:pass
     assert time.monotonic()<end,message+'\n'+log()[-1800:];time.sleep(.05)
   def tree():return inspect_tree(image,2048)[0]
   def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
   def desktop():
    start=len(log());g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log()[start:],'Desktop')
   pointer=[400,300]
   def click(x,y):
    while pointer!=[x,y]:
     dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
    for down in [True,False]:g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.07)
   def settings():
    start=len(log());g.key('ctrl-esc');g.type('settings\n');until(lambda:re.search(r'Multiwindow: open pid=(\d+).*pos=(\d+),(\d+)',log()[start:]),'Settings launch')
    m=re.search(r'Multiwindow: open pid=(\d+).*pos=(\d+),(\d+)',log()[start:]);return tuple(map(int,m.groups()))
   def text(pid,x,y,label):
    match=re.search(r'Multiwindow: open pid='+str(pid)+r'.*surface=([0-9a-f]+)',log());surface=int(match[1],16);f=d/'text.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {surface+y*640*4:#x} {640*16*4} "{f.as_posix()}"'});data=f.read_bytes()
    for i,c in enumerate(label):
     for row in range(16):
      for col in range(8):
       if (int.from_bytes(data[(row*640+x+i*8+col)*4:(row*640+x+i*8+col+1)*4],'little')==0)!=bool(font[c*16+row]&(128>>col)):return False
    return True
   def choose(w,path):
    pid,x,y=w;click(x+8+556,y+34+86);until(lambda:text(pid,64,294,b'Name:'),'Wallpaper file picker');g.key('ctrl-a')
    for i,c in enumerate(path):g.type(c);until(lambda:text(pid,126,294,path[:i+1].encode()),'Wallpaper filename input')
    g.key('ret');until(lambda:not text(pid,64,294,b'Name:'),'Wallpaper picker close')
   def apply(w,save=False,error=False):
    pid,x,y=w;start=len(log());click(x+8+(176 if save else 64),y+34+356)
    until(lambda:f'Dialog: pid={pid} kind={2 if error else 0}' in log()[start:],'Settings result');g.dismiss_dialogs(pid)
   def close(w):g.key('esc');until(lambda:f'Scheduler: exit pid={w[0]}' in log(),'Settings close')
   def screenshot(name):
    path=out/name;g.call('screendump',{'filename':str(path.resolve()),'format':'png'});return Image.open(path).convert('RGB')
   g.command('run /tools/wpcheck.bin','Wallpaper boundary checks passed!');g.wait_for('User program exited with code 0.')
   g.command('run --read-only /tools/wpcheck.bin denied','User program exited with code 0.')
   baseline=frames();desktop()
   w=settings();choose(w,'/PICS/WALL.BMP');apply(w);close(w)
   assert 'WALL.CFG' not in tree(),'Preview persisted a preference'
   fit=screenshot('fit.png');assert fit.getpixel((300,20))==(32,54,79);assert fit.getpixel((300,100))==(0,255,0);assert fit.getpixel((600,400))==(255,255,0)
   w=settings();click(w[1]+8+556,w[2]+34+126);apply(w);close(w)
   fill=screenshot('fill.png');assert fill.getpixel((300,20))==(0,255,0);assert fill.getpixel((600,20))==(0,0,255)
   w=settings();click(w[1]+8+556,w[2]+34+126);apply(w,True);close(w)
   center=screenshot('center.png');assert center.getpixel((398,271))==(255,0,0);assert center.getpixel((401,272))==(255,255,0)
   saved=tree()['WALL.CFG'];assert saved==b'XINYI-WALL 1\n2\n/PICS/WALL.BMP\n'
   w=settings();choose(w,'/PICS/BAD.BMP');apply(w,error=True);close(w);assert tree()['WALL.CFG']==saved
   assert screenshot('bad-preserved.png').getpixel((398,271))==(255,0,0)
   g.key('esc');g.wait_for('Desktop finished.');assert frames()==baseline,'Wallpaper replacement/failure leaked frames'
   g.close();cold=d/'cold';cold.mkdir();g=Guest(qemu,None,cold,disk=image,start_desktop=True);pointer[:]=[400,300]
   assert screenshot('cold.png').getpixel((398,271))==(255,0,0),'Saved wallpaper not restored'
   g.key('esc');g.wait_for('Desktop finished.');g.command('delete /PICS/WALL.BMP','Deleted:');desktop();pointer[:]=[400,300]
   assert screenshot('missing-fallback.png').getpixel((398,271))!=(255,0,0)
   w=settings();until(lambda:f'Dialog: pid={w[0]} kind=1' in log(),'Missing wallpaper warning');g.dismiss_dialogs(w[0]);click(w[1]+8+556,w[2]+34+166);apply(w,True);close(w)
   assert tree()['WALL.CFG']==b'XINYI-WALL 1\n2\n\n','Saved colour fallback failed'
   w=settings();choose(w,'/APPS/SETTINGS/WALL.BMP');apply(w,True);close(w);screenshot('bundled-sample.png')
   g.key('esc');g.wait_for('Desktop finished.');g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
   print('PASS wallpaper: Settings picker, Fit/Fill/Center, preview vs save, malformed/source-missing recovery, bundled sample, cold boot, permissions/buffers, frame balance and fsck')
  finally:
   if g.process.poll() is None:
    try:g.call('screendump',{'filename':str((out/'last.png').resolve()),'format':'png'})
    except (OSError,RuntimeError):pass
   shutil.copyfile(g.directory/'debug.log',out/'latest-debug.log');g.close();shutil.copyfile(d/'debug.log',out/'debug.log')
if __name__=='__main__':main()
