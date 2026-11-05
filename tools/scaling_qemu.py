"""Check scaled fonts/client pixels, input, persistence and safe scale fallback."""
import argparse, re, shutil, struct, tempfile, time
from pathlib import Path
from smoke import Guest, ROOT
from folders_qemu import inspect_tree


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disk', type=Path, required=True)
    parser.add_argument('--vga', choices=['std', 'cirrus'], default='std')
    parser.add_argument('--scale', type=int, choices=[125, 150], default=125)
    args = parser.parse_args()
    pw, ph = 1280, 1024
    lw, lh = pw * 100 // args.scale, ph * 100 // args.scale
    work = (lh - 56) // 16 * 16
    cw, ch = (lw - 8) // 8 * 8, (work - 32) // 16 * 16
    client_x = (lw-cw)//2
    out = ROOT / 'target/scaling'
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-', dir=out) as temporary:
        folder = Path(temporary)
        image = folder / 'system.img'
        shutil.copyfile(args.disk, image)
        qemu = shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe'
        def guest(desktop=False):
            return Guest(qemu, None, folder,
                         disk=image, start_desktop=desktop, extra_args=['-vga', args.vga])
        g = guest()
        try:
            g.command('uiscale 125', 'too little workspace')
            assert 'UISCALE.CFG' not in inspect_tree(image, 2048)[0]
            g.command('display 1280x1024', 'Saved 1280x1024')
            g.command('uiscale ' + str(args.scale), 'Saved UI scale')
            record = b'XUS1' + bytes([args.scale])
            assert inspect_tree(image, 2048)[0]['UISCALE.CFG'] == record
            g.command('uiscale 999', 'Unsupported UI scale')
            assert inspect_tree(image, 2048)[0]['UISCALE.CFG'] == record
        finally:
            g.close()
        g = guest(True)
        try:
            def log(): return (folder / 'debug.log').read_text(errors='replace')
            def until(check, message):
                deadline = time.monotonic() + 30
                while not check():
                    assert time.monotonic() < deadline, message + '\n' + log()[-1500:]
                    time.sleep(.05)
            assert f'scale {args.scale}%, logical {lw}x{lh}' in log(), log()
            def picture():
                path = folder / 'screen.ppm'
                g.call('screendump', {'filename': str(path)})
                with path.open('rb') as stream:
                    assert stream.readline().strip() == b'P6'
                    line = stream.readline()
                    while line.startswith(b'#'): line = stream.readline()
                    assert tuple(map(int, line.split())) == (pw, ph)
                    assert stream.readline().strip() == b'255'
                    return stream.read()
            def memory(address, length):
                path = folder / 'surface.bin'
                g.call('human-monitor-command', {'command-line': f'pmemsave {address:#x} {length} "{path.as_posix()}"'})
                return path.read_bytes()
            g.key('esc'); g.wait_for('xinyi:/')
            def frames(): return int(re.search(r'Free now: +(\d+)', g.command('frames', 'Free now:'))[1])
            baseline = frames()
            start = len(log()); g.type('desktop\n')
            until(lambda: 'Desktop scheduler: ready' in log()[start:], 'Desktop restart')
            g.key('ctrl-esc'); g.type('files\n')
            until(lambda: re.search(r'Multiwindow: open pid=(\d+) .*pos=(\d+),(\d+)', log()[start:]), 'Files open')
            match = re.search(r'Multiwindow: open pid=(\d+) .*pos=(\d+),(\d+)', log()[start:])
            pid, wx, wy = map(int, match.groups())
            def relative(dx, dy):
                g.call('input-send-event', {'events': [
                    {'type': 'rel', 'data': {'axis': 'x', 'value': dx}},
                    {'type': 'rel', 'data': {'axis': 'y', 'value': dy}}]})
                time.sleep(.02)
            # Saturate at the top-left; movement then uses physical pixels.
            for _ in range(15): relative(-200, -200)
            position = [0, 0]
            def move(x, y):
                while position != [x, y]:
                    dx = max(-60, min(60, x-position[0])); dy = max(-60, min(60, y-position[1]))
                    relative(dx, dy); position[0] += dx; position[1] += dy
            def click(x, y):
                move(x * pw // lw, y * ph // lh)
                for down in [True, False]:
                    g.call('input-send-event', {'events': [{'type': 'btn', 'data': {'button': 'left', 'down': down}}]})
                    time.sleep(.08)
            click(wx+100, wy+15); click(wx+100, wy+15)
            pattern = rf'Multiwindow: resized pid={pid} surface=([0-9a-f]+) client={cw}x{ch} pos=0,0 maximized=true'
            until(lambda: re.search(pattern, log()), 'Scaled maximise hit test/geometry')
            address = int(re.search(pattern, log())[1], 16)
            move(0, 0)
            # Compare real RGB output against the owned app surface across text
            # and controls. This covers font rasterisation, fractional cell
            # boundaries, full-client stride and RGB565 conversion together.
            def client_matches():
                source = memory(address, cw * 100 * 4)
                pixels = picture()
                for py in range((28*ph+lh-1)//lh, (118*ph+lh-1)//lh):
                    sy = py * lh // ph - 28
                    for px in range((client_x*pw+lw-1)//lw, ((client_x+cw)*pw+lw-1)//lw):
                        sx = px * lw // pw - client_x
                        at = (sy*cw+sx)*4
                        expected = (source[at+2], source[at+1], source[at])
                        actual = pixels[(py*pw+px)*3:(py*pw+px)*3+3]
                        if any(abs(a-b)>7 for a,b in zip(actual, expected)):
                            (out / f'mismatch-{args.vga}-{args.scale}.txt').write_text(str((px,py,sx,sy,tuple(actual),expected)))
                            return False
                assert sum(source[i:i+3] == b'\0\0\0' for i in range(0, len(source), 4)) > 100, 'Missing text in reference surface'
                return True
            until(client_matches, 'Scaled font/control pixels differ from logical surface')
            # The pointer occupies a scaled footprint; moving it must restore
            # the source pixels, without stale cursor fragments.
            move(200, 70); move(0, 0)
            until(client_matches, 'Scaled pointer left stale client pixels')
            path = out / f'fonts-ui-{args.vga}-{args.scale}.png'
            g.call('screendump', {'filename': str(path), 'format': 'png'})
            click(lw-16, 12)
            until(lambda: f'Scheduler: exit pid={pid} code=0' in log(), 'Scaled far-right close button')
            click(lw-44, work+32)
            g.wait_for('Desktop finished.')
            assert frames() == baseline, 'Scaled composition leaked frames'
            g.command('display 800x600', 'Saved 800x600')
            shutil.copyfile(folder / 'debug.log', out / f'geometry-{args.vga}-{args.scale}.log')
        finally:
            g.close()
        g = guest(True)
        try:
            log = (folder / 'debug.log').read_text()
            assert 'using 100%' in log and 'scale 100%, logical 800x600' in log, log
            path = folder / 'fallback.png'
            g.call('screendump', {'filename': str(path), 'format': 'png'})
            assert struct.unpack('>II', path.read_bytes()[16:24]) == (800, 600)
            assert inspect_tree(image, 2048)[0]['UISCALE.CFG'] == record
            assert 'Xinyi panic:' not in log
        finally:
            g.close()
    print(f'PASS scaling ({args.vga}, {args.scale}%): persistence, font/control pixels, physical input, pointer repair, maximise, frame balance and 100% fallback')


if __name__ == '__main__': main()

