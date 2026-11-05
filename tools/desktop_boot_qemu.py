"""Check desktop-first startup, console access and graphics fallback on private VMs."""
import argparse
from pathlib import Path
import shutil
import tempfile
import time

from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel', type=Path, default=ROOT / 'target/i686-unknown-none/release/rust-os')
    parser.add_argument('--disk', type=Path, help='Test BIOS disk boot using a private copy')
    parser.add_argument('--qemu', default=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe')
    args = parser.parse_args()
    output = ROOT / 'target/desktop-boot'
    output.mkdir(parents=True, exist_ok=True)
    for mode in ('desktop', 'no-graphics'):
        with tempfile.TemporaryDirectory(prefix=mode + '-', dir=output) as temporary:
            directory = Path(temporary)
            disk = None
            if args.disk:
                disk = directory / 'system.img'
                shutil.copyfile(args.disk, disk)
            guest = None
            try:
                guest = Guest(args.qemu, None if disk else args.kernel.resolve(), directory,
                              disk=disk, attach_disk=disk is not None, start_desktop=mode == 'desktop',
                              extra_args=['-vga', 'std' if mode == 'desktop' else 'none'])
                if mode == 'desktop':
                    # Readiness is emitted only after the desktop is composed and flushed.
                    assert 'Boot: console ready' not in (directory / 'debug.log').read_text()
                    guest.key('esc')
                    guest.wait_for('xinyi:/')
                    guest.command('version', 'Xinyi kernel version')
                    guest.type('desktop\n')
                    deadline = time.monotonic() + 15
                    while (directory / 'debug.log').read_text().count('Desktop: ready') < 2:
                        assert time.monotonic() < deadline, 'Desktop did not reopen'
                        time.sleep(.05)
                    guest.key('esc')
                    guest.wait_for('xinyi:/')
                else:
                    guest.command('version', 'Xinyi kernel version')
                print('PASS ' + mode, flush=True)
            finally:
                if guest:
                    guest.close()
                for name in ('debug.log', 'qemu.stderr'):
                    if (directory / name).exists():
                        shutil.copyfile(directory / name, output / (mode + '-' + name))


if __name__ == '__main__':
    main()
