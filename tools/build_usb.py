"""Package an existing verified release as a raw USB image and test USB boot."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from smoke import ROOT


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release",type=Path,help="Verified release directory; defaults to latest-release.json")
    args=parser.parse_args()
    release=args.release.resolve() if args.release else Path(json.loads((ROOT/"target/latest-release.json").read_text())["directory"])
    parent=ROOT/"target/usb";parent.mkdir(parents=True,exist_ok=True)
    directory=Path(tempfile.mkdtemp(prefix="xinyi-bios-",dir=parent));image=directory/"xinyi-usb.img"
    shutil.copyfile(release/"xinyi.img",image)
    subprocess.run([sys.executable,str(ROOT/"tools/usb_qemu.py"),"--image",str(image),"--kernel",str(release/"rust-os")],check=True,cwd=ROOT)
    notes=("Xinyi BIOS USB image\n\n"
           "Flash xinyi-usb.img as a raw disk image (for example Rufus DD mode or balenaEtcher).\n"
           "Choose the correct USB: flashing replaces its partition table and existing contents.\n"
           "Boot in legacy BIOS mode. This image contains the kernel and FAT16 /APPS files.\n"
           "MEMDEMO in Apps demonstrates ELF globals/BSS and heap-backed Rust collections.\n"
           "The boot USB mounts automatically as drive 0. Type apps, edit note.txt, run snake, or run tetris.\n"
           "Graphical editor: notepad note.txt. Drag its title bar; close requests check unsaved changes.\n"
           "Notepad: drag/Shift+arrows to select; Ctrl+A/C/X/V selects all/copies/cuts/pastes using a shared clipboard.\n"
           "Terminal: type terminal or choose TERMINAL in Apps; it runs the same shell as the physical console.\n"
           "FILES in Apps browses folders and opens files. Terminal history: Up/Down; scrollback: Shift+PgUp/PgDn; Ctrl+Q closes. Each terminal has its own directory.\n"
           "Files/Notepad resize from the lower-right grip; Terminal can show up to 33 rows. Use the title-bar maximize/restore button or double-click to fill the desktop below the taskbar.\n"
           "shutdown powers off or safely halts after flushing; use shutdown confirm from the desktop terminal.\n"
           "Notepad Open/Save As browses folders; Terminal has a right-hand scrollbar for history.\n"
           "Settings in Apps changes desktop/title colours; Save keeps them on the boot drive.\n"
           "Files supports Ctrl/Shift selection, right-click actions and folder copy/move across windows.\n"
           "Editor: Ctrl+S save, Ctrl+Q quit. Notepad supports 64 KiB ASCII documents, a scrollbar and four undo steps; EDIT/shell text commands and clipboard retain a 1024-byte limit.\n"
           "Type graphics for the 800x600 framebuffer test, arrows to move, Escape to return.\n"
           "Type mouse for the PS/2 arrow pointer, two clickable buttons and right-click menu.\n"
           "Type desktop for concurrent graphical apps and a top taskbar. Click running apps to switch focus.\n"
           "Apps opens text programs in Terminal automatically. The _ button minimizes; taskbar buttons restore.\n"
           "Windows can move partly off-screen; the top taskbar remains accessible.\n"
           "Type multitask for independent counter windows: Q quits, C crashes the focused app, Escape exits.\n"
           "Ctrl+Alt+K force-stops the focused app; multitask hog tests an app that never yields.\n"
           "Run text programs in Terminal to keep other windows running. Ctrl+C interrupts; Ctrl+Shift+C copies.\n"
           "BIOS VESA 2/3 RGB565/24/32-bit modes are supported; bios prints diagnostics.\n"
           "To install: disks, then install NUMBER for the desired IDE destination, inspect the preview, then install confirm.\n"
           "Installation copies the apps from the mounted source. Drive 0 is not a format/install destination.\n"
           "Use unmount before removing the USB. Firmware writes are read back but have no portable cache-flush guarantee.\n"
           "Support is for the BIOS boot drive, without native USB hotplug, non-boot USB, AHCI, or UEFI support.\n"
           "Verified with QEMU USB-only boot and Cirrus VESA.\n")
    (directory/"README.txt").write_text(notes)
    digest=hashlib.sha256(image.read_bytes()).hexdigest()
    (directory/"SHA256SUMS.txt").write_text(digest+"  "+image.name+"\n")
    report={"directory":str(directory),"image":str(image),"sha256":digest,"release":str(release)}
    (ROOT/"target/latest-usb.json").write_text(json.dumps(report,indent=2))
    print("Verified USB image: "+str(image))
    print("SHA256: "+digest)


if __name__=="__main__":main()
