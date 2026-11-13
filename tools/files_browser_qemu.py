"""Check whole-folder sorting, history, properties and column resizing in Files."""
import argparse
from array import array
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
from seed_apps import seed_apps
from folders_qemu import inspect_tree
from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disk', type=Path, required=True)
    args = parser.parse_args()
    output = ROOT / 'target/files-browser'
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-', dir=output) as temporary:
        d = Path(temporary); image = d / 'system.img'; shutil.copyfile(args.disk, image)
        package = d / 'package'; (package / 'apps/files').mkdir(parents=True)
        subprocess.run([sys.executable, str(ROOT / 'tools/build_program.py'), str(ROOT / 'programs/files/main.rs'),
                        '--format', 'elf', '--output', str(package / 'apps/files/files.bin')], check=True)
        (package / 'applications.json').write_text(json.dumps([{'directory': 'FILES', 'file': 'FILES.BIN'}]))
        qemu = shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
        guest = Guest(qemu, None, d, disk=image)
        try: guest.command('delete /apps/files/files.bin', 'Deleted:')
        finally: guest.close()
        seed_apps(image, package)
        fixture = d / 'fixture'; (fixture / 'apps/sub').mkdir(parents=True)
        (fixture / 'apps/sub/item.txt').write_bytes(b'inside')
        big = bytes(i % 251 for i in range(65536))
        (fixture / 'apps/sub/big.bin').write_bytes(big)
        entries = [{'directory': 'SUB', 'file': 'ITEM.TXT'}, {'directory': 'SUB', 'file': 'BIG.BIN'}]
        for i in reversed(range(36)):
            name = f'F{i:02}.TXT'
            (fixture / 'apps' / name.lower()).write_bytes(b'x' * (i + 1))
            entries.append({'file': name})
        (fixture / 'applications.json').write_text(json.dumps(entries))
        seed_apps(image, fixture, root_name='BROWSE')
        subprocess.run(['rustc', '--edition=2021', str(ROOT / 'tools/font_dump.rs'), '-o', str(d / 'font.exe')], check=True)
        font = subprocess.check_output([str(d / 'font.exe')])
        glyphs = {bytes(bool(font[c*16+y] & (0x80 >> x)) for y in range(16) for x in range(8)): chr(c) for c in range(32,127)}
        guest = Guest(qemu, None, d, disk=image)
        try:
            def log(): return (d / 'debug.log').read_text()
            def until(test, message):
                deadline = time.monotonic() + 45
                while not test():
                    assert time.monotonic() < deadline, message + '\n' + log()[-1500:]
                    time.sleep(.06)
            def frames(): return int(re.search(r'Free now: +(\d+)', guest.command('frames','Free now:'))[1])
            baseline = frames()
            guest.command('cd /browse', 'xinyi:/BROWSE>')
            start = len(log()); guest.type('desktop\n'); guest.key('ctrl-esc'); guest.type('files\n')
            until(lambda: 'Multiwindow: open pid=' in log()[start:], 'Files did not open')
            m = re.search(r'Multiwindow: open pid=(\d+) handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)', log()[start:])
            pid, address, width, height, wx, wy = int(m[1]), int(m[2],16), *map(int,m.groups()[2:])
            pointer = [400,300]
            def move(x,y):
                while pointer != [x,y]:
                    dx=max(-60,min(60,x-pointer[0])); dy=max(-60,min(60,y-pointer[1]))
                    guest.call('input-send-event',{'events':[{'type':'rel','data':{'axis':'x','value':dx}}, {'type':'rel','data':{'axis':'y','value':dy}}]})
                    pointer[0]+=dx;pointer[1]+=dy;time.sleep(.02)
            def button(down):
                guest.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
            def click(x,y): move(wx+8+x,wy+34+y);button(True);button(False)
            def line(y,x=8,count=12,fg=0):
                path=d/'pixels.bin'; guest.call('human-monitor-command', {'command-line':f'pmemsave {address+y*width*4:#x} {width*16*4} "{path.as_posix()}"'})
                pixels=array('I');pixels.frombytes(path.read_bytes())
                return ''.join(glyphs.get(bytes(pixels[dy*width+x+col*8+dx]==fg for dy in range(16) for dx in range(8)), '?') for col in range(count)).rstrip()
            until(lambda: line(80,fg=0xffffff)=='SUB', 'Initial physical-order listing')
            guest.key('shift-tab');guest.key('ret')  # Reverse wrap from the list to Next.
            until(lambda: line(80).startswith('F'), 'Keyboard Next did not advance the listing')
            guest.key('home')  # List navigation returns focus from the toolbar.
            until(lambda: line(80,fg=0xffffff)=='SUB', 'Home did not return to the first item')
            click(40,70)
            until(lambda: line(96)=='F00.TXT', 'Name sort did not include entries beyond first page')
            guest.key('home');guest.key('down')
            for _ in range(7): guest.key('tab')  # File list to More.
            guest.key('spc');guest.key('up');guest.key('ret')  # Skip disabled Update app to Properties.
            until(lambda: line(120,40,10)=='Properties', 'Properties did not open')
            assert line(152,40)=='F00.TXT' and line(184,208,10)=='1', 'Wrong properties after sorting'
            guest.key('esc');guest.key('home');guest.key('down');guest.key('shift-down');guest.key('ctrl-c')
            guest.key('home');guest.key('ret')
            until(lambda: '/BROWSE/SUB' in line(40,224,min(60,(width-232)//8),fg=0x20384d), 'Copy destination did not open')
            guest.key('ctrl-v')
            def copied():
                try:
                    files=inspect_tree(image,2048)[0]
                    return files.get('BROWSE/SUB/F00.TXT')==b'x' and files.get('BROWSE/SUB/F01.TXT')==b'xx'
                except AssertionError: return False
            until(copied, 'Sorted Shift-selection copied the wrong files')
            guest.dismiss_dialogs(pid, wait=True)
            guest.key('alt-left')
            until(lambda: line(40,224,min(60,(width-232)//8),fg=0x20384d)=='/BROWSE', 'Back after sorted copy failed')
            guest.key('home');guest.key('ret')
            until(lambda: '/BROWSE/SUB' in line(40,224,min(60,(width-232)//8),fg=0x20384d), 'Folder did not open')
            guest.key('alt-left')
            until(lambda: line(40,224,min(60,(width-232)//8),fg=0x20384d)=='/BROWSE', 'Back navigation failed')
            guest.key('alt-right')
            until(lambda: '/BROWSE/SUB' in line(40,224,min(60,(width-232)//8),fg=0x20384d), 'Forward navigation failed')
            guest.key('alt-left')
            until(lambda: line(40,224,min(60,(width-232)//8),fg=0x20384d)=='/BROWSE', 'Second Back failed')
            click(40,70)
            until(lambda: line(96)=='F35.TXT', 'Descending global sort failed')
            click(570,70)
            until(lambda: line(96)=='F00.TXT', 'Ascending size sort failed')
            # A column divider moves without changing sort order or selecting rows.
            move(wx+8+320,wy+34+70);button(True);move(wx+8+240,wy+34+70);button(False)
            until(lambda: line(64,248,4)=='Type', 'Column divider did not move')
            assert line(96)=='F00.TXT', 'Column drag changed row order'
            guest.key('end');guest.key('alt-ret')
            until(lambda: line(120,40,10)=='Properties', 'Last-page properties failed')
            assert line(152,40)=='F35.TXT' and line(184,208,10)=='36', 'Wrong last-page selection/properties'
            guest.key('esc')
            guest.key('ctrl-3')
            until(lambda: line(64,8,5)=='Icons', 'Icon view did not appear')
            guest.key('home');guest.key('right');guest.key('alt-ret')
            until(lambda: line(120,40,10)=='Properties', 'Icon selection failed')
            assert line(152,40)=='F00.TXT', 'Icon right-arrow selected wrong file'
            guest.key('esc')
            click(180,110);guest.key('alt-ret')
            until(lambda: line(120,40,10)=='Properties', 'Icon mouse selection failed')
            assert line(152,40)=='F00.TXT', 'Icon click selected wrong file'
            guest.key('esc')
            move(wx+8+420,wy+34+110);button(True);move(wx+8+440,wy+34+110);move(wx+8+60,wy+34+110);button(False)
            def icon_drop():
                try: return inspect_tree(image,2048)[0].get('BROWSE/SUB/F02.TXT')==b'xxx'
                except AssertionError: return False
            until(icon_drop, 'Icon folder drop chose the wrong destination')
            guest.dismiss_dialogs(pid, wait=True)
            guest.key('home');guest.key('ret')
            until(lambda: '/BROWSE/SUB' in line(40,224,min(60,(width-232)//8),fg=0x20384d), 'Icon folder open failed')
            guest.key('ctrl-d')
            until(lambda: b'/BROWSE/SUB' in inspect_tree(image,2048)[0].get('FAVS.CFG',b''), 'Favourite did not save')
            guest.key('end');guest.key('ctrl-c');guest.key('alt-left')
            until(lambda: line(40,224,min(60,(width-232)//8),fg=0x20384d)=='/BROWSE', 'Icon Back failed')
            guest.key('ctrl-v')
            until(lambda: line(128,40,16)=='Copying items', 'Transfer item progress did not appear')
            assert line(160,136,1)=='0' and line(160,216,1)=='1', 'Transfer displayed incorrect completed/total counts'
            def big_copied():
                try: return inspect_tree(image,2048)[0].get('BROWSE/BIG.BIN')==big
                except AssertionError: return False
            until(big_copied, 'Icon copy produced incorrect contents')
            guest.dismiss_dialogs(pid, wait=True)
            guest.key('ctrl-2')
            until(lambda: line(64,8,7)=='Compact', 'Compact view did not appear')
            guest.key('home');guest.key('right');guest.key('alt-ret')
            until(lambda: line(120,40,10)=='Properties', 'Compact selection failed')
            assert line(152,40)=='F00.TXT', 'Compact right-arrow selected wrong file'
            guest.key('esc');click(250,92);guest.key('alt-ret')
            until(lambda: line(120,40,10)=='Properties', 'Compact mouse selection failed')
            assert line(152,40)=='F00.TXT', 'Compact click selected wrong file'
            guest.key('esc');guest.key('ctrl-b')
            until(lambda: line(147,40,40,fg=0xffffff)=='/BROWSE/SUB', 'Places did not list saved favourite')
            guest.key('ret')
            until(lambda: '/BROWSE/SUB' in line(40,224,min(60,(width-232)//8),fg=0x20384d), 'Favourite did not navigate')
            guest.key('alt-left');guest.key('ctrl-1')
            until(lambda: line(64,8,4)=='Name', 'Details view did not return')
            start = len(log())
            move(wx+100,wy+15);button(True);button(False);button(True);button(False)
            until(lambda: re.search(r'Multiwindow: resized pid='+str(pid)+r' surface=([0-9a-f]+) client=792x512',log()[start:]), 'Files did not maximise')
            resized = re.search(r'Multiwindow: resized pid='+str(pid)+r' surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log()[start:])
            address,width,height,wx,wy=int(resized[1],16),*map(int,resized.groups()[1:])
            until(lambda: line(40,224,min(60,(width-232)//8),fg=0x20384d)=='/BROWSE', 'Maximised path/footer failed to reflow')
            assert line(64,248,4)=='Type', 'Maximising lost custom columns'
            guest.key('ctrl-q')
            until(lambda: f'Scheduler: exit pid={pid} code=0' in log(), 'Files did not exit cleanly')
            guest.key('esc');guest.wait_for('Desktop finished.')
            assert frames()==baseline, 'Files sort/history leaked frames'
            shutil.copyfile(d/'debug.log',output/'session-debug.log')
            guest.close()
            guest=Guest(qemu,None,d,disk=image)
            cold_baseline=frames();start=len(log());guest.type('desktop\n');guest.key('ctrl-esc');guest.type('files\n')
            until(lambda: 'Multiwindow: open pid=' in log()[start:], 'Files did not reopen after cold boot')
            m=re.search(r'Multiwindow: open pid=(\d+) handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)',log()[start:])
            pid,address,width,height,wx,wy=int(m[1]),int(m[2],16),*map(int,m.groups()[2:])
            guest.key('ctrl-b')
            until(lambda: line(147,40,40,fg=0xffffff)=='/BROWSE/SUB', 'Favourite was lost after cold boot')
            guest.key('ret');until(lambda: '/BROWSE/SUB' in line(40,224,min(60,(width-232)//8),fg=0x20384d), 'Cold-boot favourite did not open')
            guest.key('ctrl-d')
            until(lambda: inspect_tree(image,2048)[0].get('FAVS.CFG')==b'XFV1\x00', 'Removing favourite did not persist')
            guest.key('ctrl-q');until(lambda: f'Scheduler: exit pid={pid} code=0' in log()[start:], 'Cold-boot Files did not close')
            guest.key('esc');guest.wait_for('Desktop finished.');assert frames()==cold_baseline, 'Favourite load/remove leaked frames'
            print('PASS Files browser: global sorting, paged selection/properties, history, resizable columns/maximise, compact/icon mouse and keyboard input, icon folder drop, item transfer progress, exact data, cold-boot favourites and frame balance',flush=True)
        finally:
            guest.call('screendump',{'filename':str(output/'last.png'),'format':'png'})
            guest.close();shutil.copyfile(d/'debug.log',output/'debug.log')


if __name__ == '__main__': main()
