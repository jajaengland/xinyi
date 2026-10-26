"""Build and verify a BIOS-bootable release with its standard applications."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT=Path(__file__).resolve().parents[1]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir",type=Path,help="New directory; existing directories are refused")
    args=parser.parse_args()
    if args.output_dir and args.output_dir.exists():parser.error("Output directory already exists")
    environment=os.environ.copy()
    for directory in (r"C:\msys64\usr\bin",r"C:\Program Files\qemu"):
        if Path(directory).is_dir():environment["PATH"]=directory+os.pathsep+environment["PATH"]
    qemu=shutil.which("qemu-system-i386",path=environment["PATH"])
    if not qemu:parser.error("QEMU is required to verify the installed release")
    subprocess.run(["cargo","+nightly","build","--release","-Z","build-std=core"],cwd=ROOT,env=environment,check=True)
    if args.output_dir:
        directory=args.output_dir.resolve();directory.mkdir(parents=True)
    else:
        parent=ROOT/"target/releases";parent.mkdir(parents=True,exist_ok=True)
        directory=Path(tempfile.mkdtemp(prefix="xinyi-",dir=parent))
    kernel=ROOT/"target/i686-unknown-none/release/rust-os"
    image=directory/"xinyi.img"
    subprocess.run([sys.executable,str(ROOT/"tools/build_apps.py"),"--output-dir",str(directory)],cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/system_qemu.py"),"--package",str(directory),"--kernel",str(kernel),
                    "--qemu",qemu,"--output",str(image)],cwd=ROOT,env=environment,check=True)
    shutil.copyfile(kernel,directory/"rust-os")
    shutil.copyfile(ROOT/"programs/apps.txt",directory/"applications.txt")
    (directory/"README.txt").write_text(
        "Xinyi BIOS-bootable release\n\nPrograms are ordinary files in /APPS, not embedded in the kernel.\n"
        "The installed system drive mounts automatically. Type apps, run snake, run tetris or edit note.txt.\n"
        "Editor: Ctrl+S save, Ctrl+Shift+S Save As, Ctrl+O open, Ctrl+N new, Ctrl+Z/Y undo/redo, Ctrl+Q quit.\n"
        "Text files currently support ASCII and up to 1024 bytes. Save errors retain the document.\n"
        "Use disks to list numbered drives, use NUMBER to switch, and install NUMBER to preview installation.\n"
        "Installation copies /APPS from the currently mounted source to the destination after confirmation.\n"
        "A prebuilt QEMU image remembers its QEMU IDE controller; on other hardware select the drive with disks/use.\n"
        "QEMU speaker sound is enabled by Launch-Xinyi.cmd. Games: P pauses, R restarts, Escape quits.\n"
        "Verified: BIOS boot, actual app files, editor saves across cold boots and copying apps onto another drive.\n")
    # %~dp0 keeps the Windows launcher usable after moving the release folder.
    (directory/"Launch-Xinyi.cmd").write_text('@echo off\n"'+qemu+'" -m 128M -drive "file=%~dp0xinyi.img,format=raw,if=ide,index=0" -boot c -display sdl -name "Xinyi release" -audiodev dsound,id=speaker -machine pcspk-audiodev=speaker -nic none\n')
    files=sorted(path for path in directory.rglob("*") if path.is_file())
    (directory/"SHA256SUMS.txt").write_text("".join(hashlib.sha256(path.read_bytes()).hexdigest()+"  "+path.relative_to(directory).as_posix()+"\n" for path in files))
    report={"directory":str(directory),"image":str(image),"kernel":str(directory/"rust-os"),"qemu":qemu}
    (ROOT/"target/latest-release.json").write_text(json.dumps(report,indent=2))
    print("Verified release: "+str(directory))


if __name__=="__main__":main()
