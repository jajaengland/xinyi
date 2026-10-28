"""Verify desktop keyboard navigation, focus routing and retained window state."""
import argparse
import re
import shutil
import tempfile
import time
from pathlib import Path

from folders_qemu import inspect_tree
from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disk', type=Path, required=True)
    parser.add_argument('--vga', default='std')
    parser.add_argument('--usb', action='store_true')
    args = parser.parse_args()
    output = ROOT / 'target/smoke'
    output.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='keyboard-', dir=output) as temporary:
        directory = Path(temporary)
        image = directory / 'system.img'
        shutil.copyfile(args.disk, image)
        extra = ['-vga', args.vga]
        if args.usb:
            extra += ['-drive', f'if=none,id=stick,file={image.as_posix()},format=raw',
                      '-device', 'piix3-usb-uhci,id=uhci',
                      '-device', 'usb-storage,bus=uhci.0,drive=stick,bootindex=1']
        qemu = shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
        guest = Guest(qemu, None, directory, disk=image, attach_disk=not args.usb, extra_args=extra)
        try:
            def log():
                return (directory / 'debug.log').read_text()

            def until(predicate, message):
                deadline = time.monotonic() + 30
                while True:
                    try:
                        if predicate():
                            return
                    except AssertionError:
                        pass  # A live FAT snapshot may catch a mirror update.
                    assert time.monotonic() < deadline, message + '\n' + log()[-2000:]
                    time.sleep(.04)

            def tree():
                return inspect_tree(image, 2048)[0]

            def frames():
                return int(re.search(r'Free now: +(\d+)', guest.command('frames', 'Free now:'))[1])

            baseline = frames()
            guest.type('desktop\n')
            until(lambda: 'Desktop scheduler: ready' in log(), 'Desktop did not start')
            guest.key('alt-tab')
            guest.key('alt-shift-tab')
            names = sorted(path.split('/')[-1][:-4] for path in tree()
                           if path.startswith('APPS/') and path.count('/') == 1
                           and path.endswith(('.BIN', '.ELF')))

            def launch(name):
                start = len(log())
                guest.key('ctrl-esc')
                guest.key('end')
                guest.key('home')
                for _ in range(names.index(name)):
                    guest.key('down')
                guest.key('ret')
                pattern = r'Desktop: launch pid=(\d+) path=/APPS/' + name + r'\.(BIN|ELF)'
                until(lambda: re.search(pattern, log()[start:]), 'Keyboard launch ' + name)
                pid = int(re.search(pattern, log()[start:])[1])
                until(lambda: f'Multiwindow: open pid={pid} ' in log(), 'Missing window ' + name)
                return pid

            counter = launch('COUNTER')
            pad = launch('NOTEPAD')
            terminal = launch('TERMINAL')

            def switch(pid, reverse=False):
                start = len(log())
                guest.key('alt-shift-tab' if reverse else 'alt-tab')
                until(lambda: f'Multiwindow: keyboard focus pid={pid}\n' in log()[start:],
                      'Keyboard focus did not reach ' + str(pid))

            switch(counter)
            switch(pad)
            guest.type('first')
            switch(terminal)
            guest.type('touch kept.txt\n')
            until(lambda: 'KEPT.TXT' in tree(), 'Typing did not reach Terminal')
            switch(pad, reverse=True)
            guest.type(' second')
            # Alt+F4 must request a normal close, preserving unsaved editor data.
            start = len(log())
            guest.key('alt-f4')
            until(lambda: f'Multiwindow: close request pid={pad}' in log()[start:], 'No close request')
            time.sleep(.2)
            assert f'Scheduler: exit pid={pad}' not in log(), 'Unsaved editor was force-closed'
            guest.key('esc')
            guest.key('ctrl-s')
            guest.type('route.txt\n')
            until(lambda: tree().get('ROUTE.TXT') == b'first second',
                  'Window-switching keys leaked into the document or typing was misrouted')

            position = [400, 300]

            def click(x, y):
                while position != [x, y]:
                    dx = max(-80, min(80, x - position[0]))
                    dy = max(-80, min(80, y - position[1]))
                    guest.call('input-send-event', {'events': [
                        {'type': 'rel', 'data': {'axis': 'x', 'value': dx}},
                        {'type': 'rel', 'data': {'axis': 'y', 'value': dy}}]})
                    position[0] += dx
                    position[1] += dy
                    time.sleep(.03)
                for down in [True, False]:
                    guest.call('input-send-event', {'events': [
                        {'type': 'btn', 'data': {'button': 'left', 'down': down}}]})
                    time.sleep(.08)

            match = re.search(r'Multiwindow: open pid=' + str(pad) + r' .*pos=(\d+),(\d+)', log())
            click(int(match[1]) + 614, int(match[2]) + 15)
            until(lambda: re.search(r'Multiwindow: resized pid=' + str(pad) +
                                   r' .*client=792x544 .*maximized=true', log()), 'Maximize editor')
            changes = log().count(f'Multiwindow: resized pid={pad} ')
            click(734, 39)
            until(lambda: f'Multiwindow: minimized pid={pad}' in log(), 'Minimize editor')
            switch(pad, reverse=True)
            assert f'Multiwindow: restored pid={pad} pos=0,24' in log(), 'Maximized placement lost'
            assert log().count(f'Multiwindow: resized pid={pad} ') == changes, 'Restore resized the editor'
            # Opening/canceling Apps consumes its navigation keys, preserving app data.
            guest.key('ctrl-esc')
            guest.key('down')
            guest.key('esc')
            guest.key('alt-f4')
            until(lambda: f'Scheduler: exit pid={pad} code=0' in log(), 'Saved editor did not close')
            switch(counter)  # Removed middle slot must be skipped.
            guest.key('q')
            until(lambda: f'Scheduler: exit pid={counter} code=0' in log(), 'Counter did not receive q')
            switch(terminal)  # A single remaining window stays usable.
            guest.type('touch survived.txt\n')
            until(lambda: 'SURVIVED.TXT' in tree(), 'Remaining terminal lost keyboard input')
            extra_counter = launch('COUNTER')
            guest.key('ctrl-esc')
            guest.key('ctrl-alt-k')
            until(lambda: f'Scheduler: forced exit pid={extra_counter}' in log(),
                  'Emergency stop was swallowed by the Apps menu')
            guest.key('alt-f4')
            until(lambda: f'Scheduler: exit pid={terminal} code=0' in log(), 'Terminal close')
            until(lambda: 'Taskbar: pids=0,0,0,0' in log(), 'Taskbar did not empty')
            time.sleep(.2)  # Let the final synchronous catalog refresh finish.
            guest.key('alt-tab')
            time.sleep(.15)
            click(768, 11)
            guest.wait_for('Desktop finished.')
            assert frames() == baseline, 'Keyboard switching leaked frames'
            assert tree()['ROUTE.TXT'] == b'first second'
            print('PASS desktop keyboard: forward/reverse cycling, menu launch/cancel, '
                  'typing isolation, normal close prompts, maximized/minimized restore, '
                  'removed/empty/single-window cases and frame balance', flush=True)
        finally:
            guest.call('screendump', {'filename': str(output / 'keyboard-last.png'), 'format': 'png'})
            guest.close()
            shutil.copyfile(directory / 'debug.log', output / 'keyboard-debug.log')


if __name__ == '__main__':
    main()
