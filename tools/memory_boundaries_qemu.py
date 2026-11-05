"""Adversarial syscall buffers in foreground and scheduled ELF processes."""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

from seed_apps import seed_apps
from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disk', type=Path, required=True)
    parser.add_argument('--qemu', default=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe')
    args = parser.parse_args()
    output = ROOT / 'target/smoke'
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='memory-boundaries-', dir=output) as temporary:
        directory = Path(temporary)
        image = directory / 'system.img'
        shutil.copyfile(args.disk, image)
        package = directory / 'helper'
        (package / 'apps').mkdir(parents=True)
        subprocess.run([sys.executable, str(ROOT / 'tools/build_program.py'),
                        str(ROOT / 'programs/memory_check/main.rs'), '--format', 'elf',
                        '--output', str(package / 'apps/bounds.elf')], check=True)
        (package / 'applications.json').write_text(json.dumps([{'name': 'bounds', 'file': 'BOUNDS.ELF'}]))
        seed_apps(image, package, root_name='TOOLS')
        guest = Guest(args.qemu, None, directory, disk=image)
        try:
            def log():
                return (directory / 'debug.log').read_text()

            def until(test, message):
                deadline = time.monotonic() + 30
                while not test():
                    assert time.monotonic() < deadline, message + '\n' + log()[-2000:]
                    time.sleep(.04)

            def frames():
                return int(re.search(r'Free now: +(\d+)', guest.command('frames', 'Free now:'))[1])

            baseline = frames()
            for _ in range(2):
                guest.command('run /tools/bounds.elf bounds', 'Syscall boundary checks passed!')
                guest.wait_for('User program exited with code 0.')
                assert frames() == baseline, 'Foreground boundary test leaked frames'
            start = len(log())
            guest.type('desktop\n')
            until(lambda: 'Desktop scheduler: ready' in log()[start:], 'Desktop did not start')
            guest.key('ctrl-esc')
            guest.type('terminal\n')
            until(lambda: re.search(r'Multiwindow: open pid=(\d+)', log()[start:]), 'Terminal did not open')
            terminal = int(re.search(r'Multiwindow: open pid=(\d+)', log()[start:])[1])
            for _ in range(2):
                start = len(log())
                guest.type('run /tools/bounds.elf bounds\n')
                until(lambda: re.search(r'Desktop: terminal launch pid=(\d+) name=/tools/bounds.elf', log()[start:]), 'Scheduled probe did not launch')
                pid = int(re.search(r'Desktop: terminal launch pid=(\d+)', log()[start:])[1])
                until(lambda: f'Scheduler: exit pid={pid} code=0' in log()[start:], 'Scheduled boundary checks failed')
            guest.key('ctrl-q')
            until(lambda: f'Scheduler: exit pid={terminal} code=0' in log(), 'Terminal did not close')
            guest.key('esc')
            guest.wait_for('Desktop finished.')
            assert frames() == baseline, 'Scheduled boundary tests leaked frames'
            assert 'Scheduler: fault' not in log() and 'Xinyi panic:' not in log(), 'Unexpected kernel/process fault'
            print('PASS syscall boundaries: wraparound, guards, read-only output, byte-granular heap limits, freed pages, atomic clipboard rejection, foreground/scheduled execution and frame balance', flush=True)
        finally:
            guest.close()
            shutil.copyfile(directory / 'debug.log', output / 'memory-boundaries-debug.log')
            shutil.copyfile(directory / 'qemu.stderr', output / 'memory-boundaries-qemu.stderr')


if __name__ == '__main__':
    main()
