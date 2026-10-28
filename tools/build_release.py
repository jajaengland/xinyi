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
    subprocess.run([sys.executable,str(ROOT/"tools/firmware_qemu.py"),"--package",str(directory),"--kernel",str(kernel)],
                   cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/mouse_qemu.py"),"--kernel",str(kernel),"--disk",str(image),"--vga","cirrus"],
                   cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/desktop_qemu.py"),"--kernel",str(kernel),"--disk",str(image),"--vga","cirrus"],
                   cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/window_qemu.py"),"--kernel",str(kernel),"--package",str(directory),"--disk",str(image),"--vga","cirrus","--usb"],
                   cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/window_drag_qemu.py"),"--disk",str(image),"--output",str(directory/"drag-check.json"),"--check"],
                   cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/launcher_qemu.py"),"--kernel",str(kernel),"--package",str(directory),"--disk",str(image),"--vga","cirrus","--usb"],
                   cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/multitask_qemu.py"),"--kernel",str(kernel),"--package",str(directory),"--disk",str(image),"--vga","cirrus","--usb"],
                   cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/desktop_apps_qemu.py"),"--disk",str(image),"--vga","cirrus","--usb"],
                   cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/window_drag_qemu.py"),"--disk",str(image),"--output",str(directory/"desktop-drag-check.json"),"--desktop","--check"],
                   cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/terminal_clipboard_qemu.py"),"--disk",str(image),"--vga","cirrus","--usb"],
                   cwd=ROOT,env=environment,check=True)
    for suite in ("pty_qemu.py", "files_qemu.py", "files_settings_qemu.py", "widgets_qemu.py", "elf_qemu.py", "large_files_qemu.py", "resize_qemu.py", "maximize_qemu.py", "window_lifecycle_qemu.py", "keyboard_qemu.py", "io_qemu.py"):
        subprocess.run([sys.executable,str(ROOT/"tools"/suite),"--disk",str(image),"--vga","cirrus","--usb"],
                       cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/fsck_qemu.py"),"--disk",str(image)],cwd=ROOT,env=environment,check=True)
    subprocess.run([sys.executable,str(ROOT/"tools/power_qemu.py"),"--disk",str(image),"--kernel",str(kernel)],cwd=ROOT,env=environment,check=True)
    shutil.copyfile(kernel,directory/"rust-os")
    shutil.copyfile(ROOT/"programs/apps.txt",directory/"applications.txt")
    (directory/"README.txt").write_text(
        "Xinyi BIOS-bootable release\n\nPrograms are ordinary files in /APPS, not embedded in the kernel.\n"
        "ELF apps support writable globals, zeroed BSS and a private 1 MiB heap; run memdemo to try Vec/Box.\n"
        "The installed system drive mounts automatically. Type apps, run snake, run tetris, edit note.txt or notepad note.txt.\n"
        "Notepad Open/Save As browses folders; Terminal has a draggable scrollbar beside its full 80-column text area.\n"
        "Editor: Ctrl+S save, Ctrl+Shift+S Save As, Ctrl+O open, Ctrl+N new, Ctrl+Z/Y undo/redo, Ctrl+Q quit.\n"
        "Notepad: drag or Shift+arrows to select, Ctrl+A select all, Ctrl+C/X/V shared clipboard.\n"
        "Type terminal (or use Apps) for a graphical command window: help, pwd/cd, ls, cat, file operations and run APP.\n"
        "Terminal uses the normal shell; text programs run inside its window. Ctrl+C interrupts; Ctrl+Shift+C copies; Ctrl+V pastes.\n"
        "Each desktop app has its own working directory; terminal-launched children inherit that terminal's directory.\n"
        "Notepad supports 64 KiB ASCII documents, a scrollbar and four undo steps. EDIT/shell text commands and clipboard retain a 1024-byte limit. Save errors retain the document.\n"
        "Use disks to list numbered drives, use NUMBER to switch, and install NUMBER to preview installation.\n"
        "Installation copies /APPS from the currently mounted source to the destination after confirmation.\n"
        "BIOS disk/USB boots mount the actual boot filesystem as drive 0; IDE destinations use install NUMBER.\n"
        "QEMU speaker sound is enabled by Launch-Xinyi.cmd. Games: P pauses, R restarts, Escape quits.\n"
        "Graphics: QEMU std VGA, or BIOS VESA 800x600 RGB565/24/32-bit on supported cards. Type bios for diagnostics.\n"
        "Type mouse for the blue-screen PS/2 pointer/buttons/menu test; Escape exits.\n"
        "Type desktop for concurrent graphical applications from /APPS and running-app buttons in the top taskbar.\n"
        "Apps opens text programs in Terminal automatically. The _ button minimizes; taskbar buttons restore.\n"
        "Windows can move partly off-screen; the top taskbar remains accessible.\n"
        "Type multitask for two concurrent protected counter windows; Q quits the focused app, C crashes it, Escape exits the demo.\n"
        "Ctrl+Alt+K stops the focused app; multitask hog verifies timer preemption of a CPU-bound app.\n"
        "Up to four apps run together. Close apps before Exit; Escape cancels menus or goes to the focused app.\n"
        "Choose FILES in Apps to browse folders, open files, create folders and confirm deletions.\n"
        "Drag lower-right grips to resize Files/Notepad; Terminal grows vertically and keeps 80 columns. Maximize/restore with the title-bar button or double-click; normal placement is remembered.\n"
        "shutdown flushes the disk and powers off or safely halts; in Desktop, save your work then shutdown confirm.\n"
        "Files: Ctrl/Shift-click selects multiple items; right-click opens actions. Copy/Move/Paste works across Files windows and transfers folders.\n"
        "Settings: choose desktop/title colours; Apply previews, Save writes /GUI.CFG for future boots.\n"
        "The mouse/windowtest tests and application windows use a simple built-in ASCII bitmap font.\n"
        "Notepad is /APPS/NOTEPAD.BIN: mouse toolbar, keyboard editing, Ctrl+S save, Ctrl+Q quit.\n"
        "USB access uses BIOS EDD for the boot drive only; no hotplug/native USB or UEFI support.\n"
        "fsck audits the selected FAT16 volume from the physical console without writes.\n"
        "Unmount, then fsck repair-orphans previews cleanup; fsck confirm applies it. Live-chain damage requires manual recovery.\n"
        "Firmware writes are read back, but provide no portable cache-flush durability guarantee.\n"
        "Verified in QEMU: BIOS boot, actual app files, editor saves across cold boots and copying apps onto another drive.\n")
    # %~dp0 keeps the Windows launcher usable after moving the release folder.
    (directory/"Launch-Xinyi.cmd").write_text('@echo off\n"'+qemu+'" -m 128M -drive "file=%~dp0xinyi.img,format=raw,if=ide,index=0" -boot c -display sdl -name "Xinyi release" -audiodev dsound,id=speaker -machine pcspk-audiodev=speaker -nic none\n')
    files=sorted(path for path in directory.rglob("*") if path.is_file())
    (directory/"SHA256SUMS.txt").write_text("".join(hashlib.sha256(path.read_bytes()).hexdigest()+"  "+path.relative_to(directory).as_posix()+"\n" for path in files))
    report={"directory":str(directory),"image":str(image),"kernel":str(directory/"rust-os"),"qemu":qemu}
    (ROOT/"target/latest-release.json").write_text(json.dumps(report,indent=2))
    print("Verified release: "+str(directory))


if __name__=="__main__":main()
