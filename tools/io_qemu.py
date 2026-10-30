"""Exercise real app progress and input while queued file operations are active."""
from desktop_controls import APPS_BUTTON,SHELL_BUTTON,app_card,task_button
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

from folders_qemu import inspect_tree
from seed_apps import seed_apps
from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disk', type=Path, required=True)
    parser.add_argument('--vga', default='std')
    parser.add_argument('--usb', action='store_true')
    parser.add_argument('--native', action='store_true', help='Use native IDE instead of firmware storage')
    args = parser.parse_args()
    output = ROOT / 'target/smoke'
    output.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='io-', dir=output) as temporary:
        directory = Path(temporary)
        image = directory / 'system.img'
        shutil.copyfile(args.disk, image)
        package = directory / 'extra'
        (package / 'apps').mkdir(parents=True)
        subprocess.run([sys.executable, str(ROOT / 'tools/build_program.py'),
                        str(ROOT / 'programs/io_check/main.rs'), '--format', 'elf',
                        '--output', str(package / 'apps/iocheck.elf')], check=True)
        (package / 'applications.json').write_text(json.dumps([{'name': 'iocheck', 'file': 'IOCHECK.ELF'}]))
        seed_apps(image, package)
        extra = ['-vga', args.vga]
        if args.usb:
            extra += ['-drive', f'if=none,id=stick,file={image.as_posix()},format=raw',
                      '-device', 'piix3-usb-uhci,id=uhci',
                      '-device', 'usb-storage,bus=uhci.0,drive=stick,bootindex=1']
        qemu = shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
        guest = Guest(qemu, None, directory, disk=image, attach_disk=not args.usb, extra_args=extra)
        try:
            def log(): return (directory / 'debug.log').read_text()

            def until(predicate, message, timeout=None):
                if timeout is None: timeout = 300 if args.usb else 90
                deadline = time.monotonic() + timeout
                while True:
                    try:
                        if predicate(): return
                    except AssertionError:
                        pass
                    assert time.monotonic() < deadline, message + '\n' + log()[-2500:]
                    time.sleep(.04)

            def tree(): return inspect_tree(image, 2048)[0]

            def memory(address, length):
                path = directory / 'memory.bin'
                guest.call('human-monitor-command', {'command-line': f'pmemsave {address:#x} {length} "{path.as_posix()}"'})
                return path.read_bytes()

            def frames(): return int(re.search(r'Free now: +(\d+)', guest.command('frames', 'Free now:'))[1])

            if args.native:
                assert not args.usb, 'Native IDE mode needs an IDE disk'
                guest.command('use 1', 'Drive 1 ready')
            baseline = frames()
            guest.type('desktop\n')
            until(lambda: 'Desktop scheduler: ready' in log(), 'Desktop startup')
            names = sorted(path.split('/')[-1][:-4] for path in tree()
                           if path.startswith('APPS/') and path.count('/') in (1,2)
                           and path.endswith(('.BIN', '.ELF')))

            def launch(name):
                start = len(log())
                guest.key('ctrl-esc'); guest.key('home')
                for _ in range(names.index(name)): guest.key('down')
                guest.key('ret')
                pattern = r'Desktop: launch pid=(\d+) path=/APPS/' + name + r'\.(BIN|ELF)'
                until(lambda: re.search(pattern, log()[start:]), 'Launch ' + name)
                pid = int(re.search(pattern, log()[start:])[1])
                until(lambda: f'Multiwindow: open pid={pid} ' in log(), 'Open ' + name)
                return pid

            def window(pid):
                match = re.search(r'Multiwindow: open pid=' + str(pid) +
                                  r' handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+) pos=(\d+),(\d+)', log())
                return int(match[1], 16), *map(int, match.groups()[1:])

            def task_pids():
                matches = re.findall(r'Taskbar: pids=(\d+),(\d+),(\d+),(\d+)', log())
                return [int(pid) for pid in matches[-1]] if matches else [0,0,0,0]

            position = [400, 300]

            def move(x, y):
                while position != [x, y]:
                    dx = max(-80, min(80, x - position[0])); dy = max(-80, min(80, y - position[1]))
                    guest.call('input-send-event', {'events': [
                        {'type': 'rel', 'data': {'axis': 'x', 'value': dx}},
                        {'type': 'rel', 'data': {'axis': 'y', 'value': dy}}]})
                    position[0] += dx; position[1] += dy
                    time.sleep(.025)

            def button(down):
                guest.call('input-send-event', {'events': [{'type': 'btn', 'data': {'button': 'left', 'down': down}}]})
                time.sleep(.07)

            def click(x, y): move(x, y); button(True); button(False)

            def task(pid):
                until(lambda: pid in task_pids(), 'Missing taskbar process '+str(pid))
                click(*task_button(task_pids().index(pid)))

            counter = launch('COUNTER'); pad = launch('NOTEPAD')
            terminal = launch('TERMINAL'); probe = launch('IOCHECK')
            address, width, height, _, _ = window(probe)

            def status(): return int.from_bytes(memory(address + (48 * width + 8) * 4, 4), 'little')

            font = memory(int(re.search(r'PTY: open .*font=([0-9a-f]+)', log())[1], 16), 4096)
            glyphs = {bytes(bool(font[c*16+y] & (0x80 >> x)) for y in range(16) for x in range(8)): chr(c)
                      for c in range(32, 127)}

            def screen():
                pointer, w, h, _, _ = window(terminal)
                pixels = array('I'); pixels.frombytes(memory(pointer, w*h*4))
                return '\n'.join(''.join(glyphs.get(bytes(pixels[(row*16+y)*w+col*8+x] != 0
                    for y in range(16) for x in range(8)), '?') for col in range(80)).rstrip()
                    for row in range(25))

            subprocess.run(['rustc','--edition=2021',str(ROOT/'tools/font_dump.rs'),'-o',str(directory/'font.exe')],check=True)
            ui_font=subprocess.check_output([str(directory/'font.exe')])
            ui_glyphs={bytes(bool(ui_font[c*16+y]&(0x80>>x)) for y in range(16) for x in range(8)):chr(c) for c in range(32,127)}
            def count():
                pointer,w,_,_,_=window(counter)
                pixels=array('I');pixels.frombytes(memory(pointer+40*w*4,w*16*4))
                text=''.join(ui_glyphs.get(bytes(pixels[y*w+8+col*8+x]==0 for y in range(16) for x in range(8)),'?') for col in range(35))
                match=re.search(r'Count: (\d+)',text)
                return int(match[1]) if match else -1
            until(lambda: count()>=0, 'Counter text did not render')
            idle_start=time.monotonic();idle_count=count();time.sleep(2)
            idle_rate=(count()-idle_count)/(time.monotonic()-idle_start)
            assert 8<=idle_rate<=12, ('Idle timer drift',idle_rate)

            task(pad); guest.type('before'); guest.key('ctrl-s'); guest.type('note.txt\n')
            until(lambda: tree().get('NOTE.TXT') == b'before', 'Initial editor save')
            task(probe)
            start = len(log()); guest.key('v' if args.usb else 'w')
            until(lambda: f'Disk I/O: start pid={probe} nr=89' in log()[start:], 'Queued large save did not start')
            pointer, cw, ch, _, _ = window(counter)
            before = memory(pointer, cw*ch*4)
            guest.key('alt-shift-tab'); guest.key('alt-shift-tab')
            guest.type(' during'); guest.key('ctrl-s')
            guest.key('alt-tab'); guest.type('echo live')
            until(lambda: 'echo live' in screen(), 'Terminal did not render typed input during I/O')
            assert status() == 0xaa0000, 'Save workload ended before the concurrency assertion'
            assert memory(pointer, cw*ch*4) != before, 'Counter stopped during disk operations'
            guest.key('ret')
            until(lambda: '\nlive\n' in screen(), 'Non-disk terminal command stalled during I/O')
            assert status() == 0xaa0000, 'Workload ended before the live command check'
            until(lambda: status() == 0x00aa00, 'Large saves did not finish')
            until(lambda: tree().get('NOTE.TXT') == b'before during', 'Queued second-app save lost data')
            until(lambda: '\nlive\n' in screen(), 'Deferred terminal command did not finish')
            expected = bytes(index % 95 + 32 for index in range(65536))
            assert tree()['SOURCE/NEST/A.BIN'] == expected
            task(probe); guest.key('r')
            until(lambda: status() == 0x7755aa, 'Queued read/error/buffer checks failed')
            copy_start=time.monotonic();copy_count=count()
            start = len(log()); guest.key('c')
            until(lambda: f'Disk I/O: start pid={probe} nr=81' in log()[start:], 'Recursive copy did not start')
            task(pad)
            _, _, _, x, y = window(pad)
            move(x+100, y+15); button(True); move(x+70, y-5); button(False)
            guest.type(' copy'); guest.key('ctrl-s')
            assert status() == 0x0000aa, 'Copy workload ended before the drag/input check'
            guest.key('alt-tab'); guest.type('touch waited.txt\n')
            until(lambda: 'Waiting for disk...' in screen(), 'Filesystem shell command did not wait for the worker')
            guest.key('alt-shift-tab')
            until(lambda: f'Multiwindow: restored pid={pad} pos={x-30},{y-20}' in log(), 'Dragging failed during the copy')
            until(lambda: status() == 0x00aaaa, 'Recursive copy did not finish')
            copy_rate=(count()-copy_count)/(time.monotonic()-copy_start)
            assert 8<=copy_rate<=12 and .8<=copy_rate/idle_rate<=1.25, ('Copying changed timer speed',idle_rate,copy_rate)
            print(f'PASS timer rate: idle={idle_rate:.2f}/s copying={copy_rate:.2f}/s',flush=True)
            until(lambda: tree().get('NOTE.TXT') == b'before during copy', 'Editor input during copy was lost')
            until(lambda: 'WAITED.TXT' in tree(), 'Deferred shell operation did not run')
            contents = tree()
            for name in (['A', 'B'] if args.usb else ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H']):
                assert contents['DEST/NEST/' + name + '.BIN'] == expected
            # Leave a legacy write queued behind an active copy. A deferred
            # unmount changes the epoch before that write may start.
            task(pad); guest.key('alt-f4')
            until(lambda: f'Scheduler: exit pid={pad} code=0' in log(), 'Editor close')
            until(lambda: task_pids()==[counter,terminal,probe,0], 'Taskbar did not compact after closing the middle app')
            secondary = launch('IOCHECK')
            until(lambda: task_pids()==[counter,terminal,probe,secondary], 'New app was not appended to the taskbar')
            secondary_pointer, sw, _, _, _ = window(secondary)
            task(terminal); guest.type('unmount')
            task(probe); start = len(log()); guest.key('k')
            until(lambda: f'Disk I/O: start pid={probe} nr=81' in log()[start:], 'Final copying write did not start')
            guest.key('ctrl-alt-k')
            until(lambda: f'Disk I/O: termination deferred pid={probe}' in log()[start:], 'Termination was not deferred')
            task(secondary); guest.key('l')
            task(terminal); guest.key('ret')
            until(lambda: 'Waiting for disk...' in screen(), 'Unmount did not wait for active writes')
            until(lambda: f'Scheduler: forced exit pid={probe}' in log()[start:], 'Deferred termination did not finish')
            until(lambda: int.from_bytes(memory(secondary_pointer+(48*sw+8)*4,4),'little')==0xaa00aa,
                  'Queued legacy write did not reject the changed volume')
            assert 'STALE.TXT' not in tree()
            guest.type('mount\n')
            until(lambda: 'Mounted FAT16' in screen(), 'Remount after stale request')
            for name in (['A', 'B'] if args.usb else ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H']):
                assert tree()['FINISH/NEST/' + name + '.BIN'] == expected
            section = log()[start:]
            assert section.index(f'Disk I/O: done pid={probe}') < section.index(f'Scheduler: forced exit pid={probe}')
            active = None
            for kind, pid in re.findall(r'Disk I/O: (start|done) pid=(\d+)', log()):
                if kind == 'start':
                    assert active is None, 'Filesystem operations overlapped'
                    active = pid
                else:
                    assert active == pid, 'Wrong I/O owner completed'
                    active = None
            assert active is None
            until(lambda: task_pids()==[counter,terminal,secondary,0], 'Taskbar did not compact after forced exit')
            task(secondary); guest.key('q')
            until(lambda: f'Scheduler: exit pid={secondary} code=0' in log(), 'Secondary probe close')
            until(lambda: task_pids()==[counter,terminal,0,0], 'Taskbar did not compact after secondary close')
            task(counter); guest.key('q')
            until(lambda: f'Scheduler: exit pid={counter} code=0' in log(), 'Counter close')
            until(lambda: task_pids()==[terminal,0,0,0], 'Last app was not moved to the leftmost taskbar position')
            task(terminal); guest.key('alt-f4')
            until(lambda: f'Scheduler: exit pid={terminal} code=0' in log(), 'Terminal close')
            click(*SHELL_BUTTON); guest.wait_for('Desktop finished.')
            assert frames() == baseline, 'Queued I/O leaked frames'
            print(('PASS queued I/O (' + ('native IDE' if args.native else 'BIOS USB' if args.usb else 'BIOS IDE') + '):') + ' steady timer rate, compact taskbar/click targets, live counter/editor/terminal, dragging and focus during writes/copies, '
                  'serialized queued saves, read/error validation, deferred termination, queued volume invalidation, exact file contents and frame balance', flush=True)
        finally:
            guest.call('screendump', {'filename': str(output / 'io-last.png'), 'format': 'png'})
            guest.close()
            shutil.copyfile(directory / 'debug.log', output / 'io-debug.log')


if __name__ == '__main__': main()
