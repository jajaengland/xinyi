"""Exercise ELF loading, private globals/heaps, faults and legacy compatibility."""
import argparse,json,re,shutil,tempfile,time
from seed_apps import seed_apps
from pathlib import Path
import subprocess,sys
from smoke import Guest,ROOT
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args()
 out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
 with tempfile.TemporaryDirectory(prefix='elf-',dir=out) as tmp:
  d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image)
  package=d/'extra';(package/'apps').mkdir(parents=True)
  subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/memory_check/main.rs'),'--format','elf','--output',str(package/'apps/check.elf')],check=True)
  import struct
  code=bytes.fromhex('b801000000bb25000000cd800f0b');tiny=bytearray(256+len(code));tiny[:7]=bytes([127,69,76,70,1,1,1]);struct.pack_into('<HHIIIIIHHHHHH',tiny,16,2,3,1,0x40000100,52,0,0,52,32,1,0,0,0);struct.pack_into('<8I',tiny,52,1,256,0x40000100,0,len(code),len(code),5,4096);tiny[256:]=code;(package/'apps/offset.elf').write_bytes(tiny)
  bad=bytearray(tiny);struct.pack_into('<I',bad,52+20,0xffffffff);(package/'apps/bad.elf').write_bytes(bad)
  (package/'applications.json').write_text(json.dumps([{'name':name,'file':name.upper()+'.ELF'} for name in ['check','offset','bad']]));seed_apps(image,package)
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
   def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
   baseline=frames()
   for command,expected in [('run memdemo','ELF data/BSS and heap checks passed!'),('run check.elf api','ELF memory API checks passed!'),('run check.elf cap','Heap cap checks passed!'),('run offset.elf','User program exited with code 37.'),('run bad.elf','Invalid or unsupported ELF executable.')]:
    g.command(command,expected)
    if 'bad' not in command:g.wait_for('User program exited with code '+('37.' if 'offset' in command else '0.'))
    assert frames()==baseline,'ELF exit/reject frame leak: '+command
   for mode,address,error in [('unmapped','0x40100000',4),('code','0x40000000',7),('guard','0x40200000',4),('freed','0x40101000',4)]:
    text=g.command('run check.elf '+mode,'User program faulted:');assert 'error=0x'+str(error) in text and 'address='+address in text,text;assert frames()==baseline,'ELF fault leaked frames'
   print('PASS ELF foreground execution, buffers, heap cap, rejection/faults and frame balance',flush=True)
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
    until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/'+name+r'\.(?:BIN|ELF)',log()[start:]),'Launch '+name);pid=int(re.search(r'Desktop: launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log()[start:],'Window '+name);return pid
   def tree():return inspect_tree(image,2048)[0]
   first=launch('CHECK');second=launch('CHECK')
   until(lambda:f'Scheduler: exit pid={first} code=0' in log() and f'Scheduler: exit pid={second} code=0' in log(),'Concurrent ELF globals/heaps corrupted or stalled')
   assert 'Scheduler: fault' not in log(),'Unexpected ELF fault'
   names=sorted(k.split('/')[-1][:-4] for k in tree() if k.startswith('APPS/') and k.count('/')==1 and k.endswith(('.BIN','.ELF')));start=len(log());click(40,11);click(40,46+names.index('MEMDEMO')*24);until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/MEMDEMO.ELF',log()[start:]),'Launch MEMDEMO');demo=int(re.search(r'Desktop: launch pid=(\d+)',log()[start:])[1]);until(lambda:'Desktop: text terminal' in log()[start:],'ELF console handoff');until(lambda:f'Scheduler: exit pid={demo} code=0' in log(),'ELF text app lost globals/heap during terminal handoff')
   terminal=int(re.findall(r'Multiwindow: open pid=(\d+)',log())[-1]);g.key('ctrl-q');until(lambda:f'Scheduler: exit pid={terminal} code=0' in log(),'ELF terminal close')
   click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'Concurrent ELF/handoff frame leak'
   print('PASS ELF: data/BSS, Vec/Box/alignment/reuse, global/heap syscall buffers, heap cap, non-base entry, malformed rejection, real page faults, concurrent isolation, text handoff and frame balance',flush=True)
  finally:
   g.close();shutil.copyfile(d/'debug.log',out/'elf-debug.log')
if __name__=='__main__':main()
