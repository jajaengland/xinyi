"""Build a clean, BIOS-bootable QEMU image and portable launcher ZIP."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile

from folders_qemu import inspect_tree
from install_qemu import verify_payload
from smoke import Guest, ROOT


WINDOWS = r'''@echo off
setlocal
cd /d "%~dp0"
set "QEMU_EXE=qemu-system-i386.exe"
where /q qemu-system-i386.exe
if not errorlevel 1 goto launch
set "QEMU_EXE=%ProgramFiles%\qemu\qemu-system-i386.exe"
if exist "%QEMU_EXE%" goto launch
echo QEMU was not found. Install QEMU, then run this launcher again.
echo Download: https://www.qemu.org/download/
pause
exit /b 1
:launch
"%QEMU_EXE%" -m 128M -drive "file=xinyi.img,format=raw,if=ide,index=0" -boot c -vga std -display sdl,grab-mod=rctrl -name "Xinyi - shared preview" -audiodev dsound,id=audio,in.voices=0,out.frequency=48000 -device AC97,audiodev=audio -machine pcspk-audiodev=audio -nic none %*
if errorlevel 1 pause
'''
UNIX = '''#!/bin/sh
set -eu
cd -- "$(dirname -- "$0")"
if ! command -v qemu-system-i386 >/dev/null 2>&1; then
    echo "Install QEMU with SDL support: https://www.qemu.org/download/" >&2
    exit 1
fi
displays=$(qemu-system-i386 -display help 2>&1)
if printf '%s\n' "$displays" | grep -qx 'gtk'; then
    display=gtk
elif printf '%s\n' "$displays" | grep -qx 'sdl'; then
    display=sdl,grab-mod=rctrl
else
    echo "This QEMU installation has no GTK or SDL graphical display." >&2
    echo "Install your distribution's QEMU GUI package (for example qemu-system-gui on Debian/Ubuntu)." >&2
    exit 1
fi
exec qemu-system-i386 -m 128M -drive file=xinyi.img,format=raw,if=ide,index=0 \\
    -boot c -vga std -display "$display" -name "Xinyi - shared preview" \\
    -device AC97 \\
    -nic none "$@"
'''
README = '''Xinyi - QEMU preview

Share this entire ZIP. Extract all files before launching.
QEMU is required but is not included. Download/install it from:
https://www.qemu.org/download/

Windows: double-click Launch-Xinyi.cmd. The launcher finds QEMU on PATH or
in the standard Program Files\\qemu installation folder.
Linux/macOS: install QEMU with GTK or SDL support, then run sh ./Launch-Xinyi.sh.
The Windows launcher is tested; the Unix launcher is supplied for convenience.
Rust, Python, NASM and the Xinyi source code are not needed to run this image.

The desktop starts automatically at 1024x768, 100% UI scale, with all standard
apps installed. Click inside the QEMU window to capture the mouse.
Right Ctrl + G releases/captures the mouse. Right Ctrl + F toggles fullscreen.
Use Apps to launch programs; Files browses the system drive. More contains
file actions and app install/remove/update. Shell opens the text console
after all applications are closed; type desktop to return.

This is an experimental OS preview. Networking is disabled. The raw image
contains only a fresh system and bundled assets, without personal test files.
Your edits and settings are saved inside xinyi.img. Keep the original ZIP
if you want to reset the system. Close QEMU before copying the image.

For another QEMU frontend, attach xinyi.img as a raw IDE hard disk, boot
with legacy BIOS, standard VGA, 128 MiB RAM and AC97 audio. The image is
not an ISO and does not use UEFI. BUILD.json records the exact source state
and verification performed; SHA256SUMS.txt lists the packaged file hashes.
Source: https://github.com/jajaengland/xinyi
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, help='New package directory; existing paths refused')
    args = parser.parse_args()
    env = os.environ.copy()
    for path in (r'C:\msys64\usr\bin', r'C:\Program Files\qemu'):
        if Path(path).is_dir(): env['PATH'] = path + os.pathsep + env['PATH']
    qemu = shutil.which('qemu-system-i386', path=env['PATH'])
    if not qemu: parser.error('QEMU is required to verify the package')
    parent = ROOT / 'target/releases'
    parent.mkdir(parents=True, exist_ok=True)
    output = args.output_dir.resolve() if args.output_dir else parent / time.strftime('Xinyi-QEMU-%Y%m%d-%H%M%S')
    archive = output.with_suffix('.zip')
    if output.exists() or archive.exists(): parser.error('Output already exists')
    output.mkdir(parents=True)
    build_target = ROOT / 'target/ci-kernel'
    env['CARGO_TARGET_DIR'] = str(build_target)
    def run(command): subprocess.run(command, cwd=ROOT, env=env, check=True)
    run(['cargo', '+nightly', 'build', '--release', '-Z', 'build-std=core'])
    kernel = build_target / 'i686-unknown-none/release/rust-os'
    image = output / 'xinyi.img'
    with tempfile.TemporaryDirectory(prefix='qemu-package-', dir=ROOT/'target') as temporary:
        work = Path(temporary)
        apps = work / 'package'
        run([sys.executable, str(ROOT/'tools/build_apps.py'), '--output-dir', str(apps)])
        run([sys.executable, str(ROOT/'tools/system_qemu.py'), '--kernel', str(kernel),
             '--package', str(apps), '--qemu', qemu, '--output', str(image)])
        verify_payload(image, kernel)
        guest = Guest(qemu, None, work, disk=image)
        try:
            guest.command('display 1024x768', 'Saved 1024x768')
            guest.command('uiscale 100', 'Saved UI scale 100%')
            guest.command('fsck', 'Read-only scan; no changes made.')
            assert 'fsck: clean' in guest.screen(), guest.screen()
        finally:
            guest.close()
        files, _ = inspect_tree(image, 2048)
        entries = json.loads((apps/'applications.json').read_text())
        for entry in entries:
            # Compare each executable and sidecar with the fresh app build.
            name = entry['name']
            for asset in (apps/'apps'/name).rglob('*'):
                if asset.is_file():
                    disk_path = 'APPS/' + name.upper() + '/' + asset.relative_to(apps/'apps'/name).as_posix().upper()
                    assert files[disk_path] == asset.read_bytes(), disk_path
        assert {name for name in files if '/' not in name}=={'DISPLAY.CFG','UISCALE.CFG'}, 'Unexpected test files in package'
        assert files['DISPLAY.CFG'] == b'XDP1\x00\x04\x00\x03'
        assert files['UISCALE.CFG'] == b'XUS1\x64'
        run([sys.executable, str(ROOT/'tools/desktop_boot_qemu.py'), '--disk', str(image), '--qemu', qemu])
    (output/'Launch-Xinyi.cmd').write_bytes(WINDOWS.replace('\n', '\r\n').encode('ascii'))
    (output/'Launch-Xinyi.sh').write_text(UNIX, encoding='ascii', newline='\n')
    (output/'README.txt').write_text(README, encoding='utf-8')
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT))
    report = {'source_commit': commit, 'includes_uncommitted_changes': dirty,
              'resolution': '1024x768', 'ui_scale': 100, 'applications': len(entries),
              'qemu': subprocess.check_output([qemu, '--version'], text=True).splitlines()[0],
              'verification': ['system install/app/save/cold-boot integration', 'kernel payload',
                               'all bundled app bytes', 'clean filesystem', 'desktop boot and text fallback']}
    (output/'BUILD.json').write_text(json.dumps(report, indent=2)+'\n')
    (output/'SHA256SUMS.txt').write_text(''.join(
        hashlib.sha256(path.read_bytes()).hexdigest()+'  '+path.name+'\n'
        for path in sorted(output.iterdir()) if path.is_file()))
    with zipfile.ZipFile(archive, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zipped:
        for path in sorted(output.iterdir()): zipped.write(path, output.name+'/'+path.name)
    (ROOT/'target/latest-qemu-package.json').write_text(json.dumps({'directory': str(output), 'zip': str(archive)}, indent=2))
    print(f'QEMU package: {archive} ({archive.stat().st_size:,} bytes)', flush=True)


if __name__ == '__main__': main()
