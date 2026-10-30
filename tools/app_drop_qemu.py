"""Verify explicit file drops into Notepad and the restricted media Player."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree
from desktop_controls import SHELL_BUTTON,task_button
def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);a=p.parse_args();out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='app-drop-',dir=out) as tmp:
  d=Path(tmp);image=d/'test.img';shutil.copyfile(a.disk,image);pkg=d/'helper';(pkg/'apps').mkdir(parents=True)
  subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/app_check/main.rs'),'--format','elf','--output',str(pkg/'apps/metahelp.elf')],check=True)
  (pkg/'applications.json').write_text(json.dumps([{'file':'METAHELP.ELF'}]));seed_apps(image,pkg)
  g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,extra_args=['-audiodev','none,id=pcm','-device','AC97,audiodev=pcm'])
  try:
   g.command('run metahelp docs','Manifest checks passed!');g.wait_for('User program exited with code 0.');baseline=int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1]);g.command('cd /docs','xinyi:/DOCS>');g.type('desktop\n')
   def log():return (d/'debug.log').read_text(errors='replace')
   def tree():return inspect_tree(image,2048)[0]
   def until(fn,msg):
    end=time.monotonic()+35
    while True:
     try:
      if fn():return
     except AssertionError:pass
     assert time.monotonic()<end,msg+'\n'+log()[-1200:];time.sleep(.04)
   until(lambda:'Desktop scheduler: ready' in log(),'desktop')
   def launch(query):
    start=len(log());g.key('ctrl-esc');g.type(query+'\n');until(lambda:'Multiwindow: open pid=' in log()[start:],'launch '+query);return int(re.search(r'Multiwindow: open pid=(\d+)',log()[start:])[1])
   files=launch('files');note=launch('notepad');pointer=[400,300]
   def move(x,y):
    while pointer!=[x,y]:
     dx=max(-60,min(60,x-pointer[0]));dy=max(-60,min(60,y-pointer[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pointer[0]+=dx;pointer[1]+=dy;time.sleep(.035)
   def button(down):g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.1)
   def drop(row):
    count=log().count('Drag: begin');move(80,220+row*16);button(True);move(102,220+row*16);until(lambda:log().count('Drag: begin')>count,'begin');move(758,350);button(False);time.sleep(.35)
   drop(0);g.key('ctrl-end');g.type(' edited');g.key('ctrl-s');until(lambda:tree()['DOCS/README.TXT']==b'manifest document edited','document drop load/save')
   g.type(' unsaved');drop(3);g.key('c');g.key('ctrl-s');until(lambda:tree()['DOCS/README.TXT']==b'manifest document edited unsaved','cancel retained unsaved document')
   g.type(' discardme');drop(0);g.key('d');g.key('ctrl-end');g.type(' reopened');g.key('ctrl-s');until(lambda:tree()['DOCS/README.TXT']==b'manifest document edited unsaved reopened','discard/open flow')
   player=launch('player');start=len(log());drop(1);until(lambda:f'Audio: open pid={player}' in log()[start:],'WAV drop did not play');assert tree()['DOCS/TONE.WAV'].startswith(b'RIFF')
   assert 'App manifest: path=/APPS/PLAYER/PLAYER.ELF access=media' in log()
   g.call('screendump',{'filename':str(out/'app-drops.png'),'format':'png'});g.key('alt-f4');until(lambda:f'Scheduler: exit pid={player} code=0' in log(),'Player close');move(*task_button(1));button(True);button(False);g.key('alt-f4');until(lambda:f'Scheduler: exit pid={note} code=0' in log(),'Notepad close');g.key('alt-f4');until(lambda:f'Scheduler: exit pid={files} code=0' in log(),'Files close')
   move(*SHELL_BUTTON);button(True);button(False);g.wait_for('Desktop finished.');assert int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])==baseline,'app drop frames leaked';g.command('fsck','Read-only scan; no changes made.');assert 'fsck: clean' in g.screen()
  finally:g.call('screendump',{'filename':str(out/'app-drops-last.png'),'format':'png'});g.close();shutil.copyfile(d/'debug.log',out/'app-drops-debug.log')
 print('PASS app drops: Notepad load/save, unsaved cancel/discard, Player Media WAV playback, source preservation, cleanup, frame balance and fsck')
if __name__=='__main__':main()
