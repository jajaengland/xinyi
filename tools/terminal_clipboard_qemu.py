"""Test graphical Terminal, cross-app clipboard/selection and private directories."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from array import array
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--vga',default='std');p.add_argument('--usb',action='store_true');a=p.parse_args();output=ROOT/'target/smoke';output.mkdir(exist_ok=True)
 subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(ROOT/'target/font-dump.exe')],check=True);font=subprocess.check_output([str(ROOT/'target/font-dump.exe')]);glyphs={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
 with tempfile.TemporaryDirectory(prefix='terminal-clipboard-',dir=output) as temp:
  directory=Path(temp);image=directory/'system.img';shutil.copyfile(a.disk,image);package=directory/'extra';(package/'apps').mkdir(parents=True)
  subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/clipboard_check/main.rs'),'--output',str(package/'apps/ccheck.bin')],check=True);(package/'applications.json').write_text(json.dumps([{'name':'ccheck','file':'CCHECK.BIN'}]));seed_apps(image,package)
  extra=['-vga',a.vga]
  if a.usb:extra+=['-drive',f'if=none,id=stick,file={image.as_posix()},format=raw','-device','piix3-usb-uhci,id=uhci','-device','usb-storage,bus=uhci.0,drive=stick,bootindex=1']
  qemu=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe';g=Guest(qemu,None,directory,disk=image,attach_disk=not a.usb,extra_args=extra)
  try:
   def log():return (directory/'debug.log').read_text()
   def until(test,message):
    end=time.monotonic()+30
    while not test():assert time.monotonic()<end,message+'\n'+log()[-1800:];time.sleep(.04)
   def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
   g.command('run ccheck','Clipboard/terminal API checks passed!');g.wait_for('User program exited with code 0.');g.command('run ccheck fill','Clipboard filled to capacity');g.wait_for('User program exited with code 0.');baseline=frames();g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'No desktop')
   pos=[400,300]
   def move(x,y):
    while pos!=[x,y]:
     dx=max(-80,min(80,x-pos[0]));dy=max(-80,min(80,y-pos[1]));g.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}},{'type':'rel','data':{'axis':'y','value':dy}}]});pos[0]+=dx;pos[1]+=dy;time.sleep(.025)
   def button(down):g.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.06)
   def click(x,y):move(x,y);button(True);button(False)
   def task(slot):click(110+slot*148,11)
   def launch(name):
    names=sorted(k.split('/')[-1][:-4] for k in inspect_tree(image,2048)[0] if k.startswith('APPS/') and k.count('/')==1 and k.endswith(('.BIN','.ELF')));offset=len(log());click(40,11);click(40,46+names.index(name)*24)
    until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/'+name+r'\.BIN',log()[offset:]),'Launch '+name);pid=int(re.search(r'Desktop: launch pid=(\d+)',log()[offset:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log()[offset:],'Window '+name);return pid
   def window(pid):
    m=re.search(r'Multiwindow: open pid='+str(pid)+r' handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log());return int(m[1],16),int(m[2]),int(m[3]),int(m[4])+8,int(m[5])+34
   def memory(address,length):
    path=directory/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {address:#x} {length} "{path.as_posix()}"'});return path.read_bytes()
   def screen(pid,color=0xd0e8d0):
    address,w,h,x,y=window(pid);data=array('I');data.frombytes(memory(address,w*h*4));return '\n'.join(''.join(glyphs.get(bytes(data[(row*16+dy)*w+col*8+dx]==color for dy in range(16) for dx in range(8)),'?') for col in range(w//8)) for row in range(min(24,h//16)))
   def file(name):
    try:return inspect_tree(image,2048)[0].get(name)
    except AssertionError:return None
   def exited(pid):return f'Scheduler: exit pid={pid} code=0' in log()
   def command(pid,text,expected=None):
    g.type(text+'\n');time.sleep(.15)
    if expected:until(lambda:expected in terminal_screen(pid),'Missing terminal output '+expected)
   def terminal_screen(pid):
    font=memory(int(re.search(r'PTY: open .*font=([0-9a-f]+)',log())[1],16),4096);letters={bytes(bool(font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
    address,w,h,x,y=window(pid);data=array('I');data.frombytes(memory(address,w*h*4));return '\n'.join(''.join(letters.get(bytes(data[(row*16+dy)*w+col*8+dx]!=data[row*16*w+col*8] for dy in range(16) for dx in range(8)),'?') for col in range(80)) for row in range(25))
   first=launch('NOTEPAD');g.key('ctrl-v');g.key('ctrl-v');g.key('ctrl-s');g.type('full.txt\n');until(lambda:file('FULL.TXT')==b'a'*2048,'Full clipboard paste or capacity refusal failed');g.key('ctrl-n');g.type('alpha beta\ngamma');until(lambda:'gamma' in screen(first,0),'Initial editor typing did not finish');surface,w,h,x,y=window(first)
   move(x,y+40);button(True)
   # QEMU PS/2 may aggregate a rapid press+motion before the guest samples it.
   # Confirm the press placed the caret before injecting the drag trajectory.
   until(lambda:int.from_bytes(memory(surface+32*w*4,4),'little')==0,'Selection press did not place the caret')
   move(x+40,y+40);until(lambda:int.from_bytes(memory(surface+32*w*4,4),'little')==0x153e8e,'Mouse selection was not highlighted');button(False);g.key('ctrl-c')
   second=launch('NOTEPAD');g.key('ctrl-v');g.type('!');g.key('ctrl-s');g.type('shared.txt\n');until(lambda:file('SHARED.TXT')==b'alpha!','Mouse-selected clipboard paste failed')
   g.key('ctrl-a');g.key('ctrl-x');g.key('ctrl-z');g.key('ctrl-s');until(lambda:file('SHARED.TXT')==b'alpha!','Cut/undo failed');g.key('ctrl-q');until(lambda:exited(second),'Close clipboard source')
   terminal=launch('TERMINAL');command(terminal,'clear','xinyi:/>');g.type('echo ');g.key('ctrl-v');g.key('ret');until(lambda:any(row.strip()=='alpha!' for row in terminal_screen(terminal).splitlines()),'Terminal paste failed')
   # Copy the command output back into the first editor.
   surface,w,h,x,y=window(terminal);row=next(i for i,row in enumerate(terminal_screen(terminal).splitlines()) if row.strip()=='alpha!');move(x,y+row*16+8);button(True);time.sleep(.15);move(x+6*8,y+row*16+8);time.sleep(.15);button(False);g.key('ctrl-shift-c');task(0);g.key('ctrl-end');g.type(' ');g.key('ctrl-v');g.key('ctrl-s');g.type('source.txt\n');until(lambda:file('SOURCE.TXT')==b'alpha beta\ngamma alpha!','Terminal-to-Notepad clipboard failed')
   g.key('ctrl-home')
   for _ in range(5):g.key('shift-right')
   g.key('ctrl-x');g.key('ctrl-s');until(lambda:file('SOURCE.TXT')==b' beta\ngamma alpha!','Shift selection/cut failed');g.key('ctrl-z');g.key('ctrl-s');until(lambda:file('SOURCE.TXT')==b'alpha beta\ngamma alpha!','Selection undo failed')
   task(1);command(terminal,'mkdir docs');command(terminal,'cd docs','/DOCS');command(terminal,'touch term.txt');command(terminal,'write term.txt terminal data');until(lambda:file('DOCS/TERM.TXT')==b'terminal data','Terminal file write failed');command(terminal,'cat term.txt','terminal data')
   # Another terminal starts in the shell directory, independently of the first.
   other=launch('TERMINAL');command(other,'pwd','xinyi:/> pwd');command(other,'touch root.txt');command(other,'write root.txt root data');until(lambda:file('ROOT.TXT')==b'root data','Terminal directory leaked')
   task(1);command(terminal,'pwd','/DOCS');command(terminal,'run notepad child.txt');until(lambda:'Desktop: terminal launch' in log(),'Terminal app launch failed');until(lambda:log().count('Multiwindow: open')>=5,'Child Notepad did not open');g.type('inherited directory');g.key('ctrl-s');until(lambda:file('DOCS/CHILD.TXT')==b'inherited directory','Child did not inherit terminal directory');g.key('ctrl-q');time.sleep(.2)
   # History recalls and re-executes the last command in the other terminal.
   task(2);g.key('up');g.key('ret');time.sleep(.2);assert file('ROOT.TXT')==b'root data';command(other,'clear');command(other,'echo final terminal','final terminal')
   g.call('screendump',{'filename':str(output/f'terminal-clipboard-{a.vga}.png'),'format':'png'})
   g.key('ctrl-q');until(lambda:exited(other),'Close other terminal');task(1);g.key('ctrl-q');until(lambda:exited(terminal),'Close terminal');task(0);g.key('ctrl-q');until(lambda:exited(first),'Close first editor');click(768,11);g.wait_for('Desktop finished.');assert frames()==baseline,'Clipboard/terminal jobs leaked frames';g.command('pwd','xinyi:/>');g.command('cat source.txt','gamma alpha!')
   print(f'PASS Terminal/clipboard ({a.vga}, usb={a.usb}): validated ABI, mouse/Shift selection, cross-app copy/cut/paste/undo, source-close retention, shell commands/history, private cwd/inheritance, app launch, saved files and frame balance',flush=True)
  finally:
   g.call('screendump',{'filename':str(output/f'terminal-clipboard-last-{a.vga}.png'),'format':'png'});g.close();shutil.copyfile(directory/'debug.log',output/f'terminal-clipboard-{a.vga}-debug.log')
  g=Guest(qemu,None,directory,disk=image,attach_disk=not a.usb,extra_args=extra)
  try:g.command('cat /docs/term.txt','terminal data');g.command('cat shared.txt','alpha!');g.command('run ccheck empty','Clipboard empty after boot')
  finally:g.close()

if __name__=='__main__':main()
