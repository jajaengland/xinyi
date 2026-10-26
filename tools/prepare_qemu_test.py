"""Prepare a new persistent BIOS-bootable test disk without touching old images."""
import json
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time

from folders_qemu import inspect_tree
from install_qemu import verify_payload
from smoke import Guest, ROOT


def main():
    qemu = shutil.which("qemu-system-i386")
    kernel = ROOT / "target/i686-unknown-none/debug/rust-os"
    if not qemu or not kernel.is_file():
        raise RuntimeError("Build Xinyi and put QEMU on PATH first")
    parent = ROOT / "target/manual-qemu"
    parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="test-", dir=parent))
    image = directory / "system.img"
    program = directory / "hello.bin"
    subprocess.run([sys.executable, str(ROOT / "tools/build_program.py"),
                    str(ROOT / "programs/hello/main.rs"), "--output", str(program)], check=True)
    with image.open("xb") as stream:
        stream.truncate(16 * 1024 * 1024)
    guest = Guest(qemu, kernel, directory, disk=image)
    try:
        def command(text, expected):
            screen = guest.command(text, expected)
            deadline = time.monotonic() + 5
            while not guest.prompt_ready(screen):
                assert time.monotonic() < deadline, screen
                time.sleep(0.01)
                screen = guest.screen()
            return screen
        command("install", "To install, enter: install confirm")
        guest.type("install confirm\n")
        guest.wait_for("Installation complete:", timeout=30)
        command("mount", "Mounted FAT16 at sector 2048")
        for name in ("apps", "docs"):
            command("mkdir " + name, "Created directory:")
        command("touch apps/hello.bin", "Created:")
        command("touch docs/readme.txt", "Created:")
        text = "Try mkdir test and cd test. Run /APPS/HELLO.BIN to test Rust programs."
        command("write docs/readme.txt " + text, "Wrote")
        command("unmount", "Unmounted.")
    finally:
        guest.close()
    verify_payload(image, kernel)

    # Populate the empty HELLO.BIN through its standard FAT16 metadata.
    data = bytearray(image.read_bytes())
    start = 2048
    boot = data[start * 512:(start + 1) * 512]
    spc, fats = boot[13], boot[16]
    reserved, entries, spf = (struct.unpack_from("<H", boot, offset)[0] for offset in (14, 17, 22))
    total = struct.unpack_from("<H", boot, 19)[0] or struct.unpack_from("<I", boot, 32)[0]
    fat = start + reserved
    root = fat + fats * spf
    first = root + (entries * 32 + 511) // 512
    def find(offset, count, name):
        for slot in range(count):
            entry = offset + slot * 32
            if data[entry:entry + 11] == name:
                return entry
        raise RuntimeError("Expected directory entry is missing")
    apps = find(root * 512, entries, b"APPS       ")
    cluster = struct.unpack_from("<H", data, apps + 26)[0]
    hello = find((first + (cluster - 2) * spc) * 512, spc * 16, b"HELLO   BIN")
    assert struct.unpack_from("<I", data, hello + 28)[0] == 0
    binary = program.read_bytes()
    assert 0 < len(binary) <= spc * 512
    clusters = (total - (first - start)) // spc
    free = next(cluster for cluster in range(2, clusters + 2)
                if struct.unpack_from("<H", data, fat * 512 + cluster * 2)[0] == 0)
    for copy in range(fats):
        struct.pack_into("<H", data, (fat + copy * spf) * 512 + free * 2, 0xffff)
    offset = (first + (free - 2) * spc) * 512
    data[offset:offset + spc * 512] = bytes(spc * 512)
    data[offset:offset + len(binary)] = binary
    struct.pack_into("<H", data, hello + 26, free)
    struct.pack_into("<I", data, hello + 28, len(binary))
    with image.open("r+b") as stream:
        stream.write(data)
    verify_payload(image, kernel)
    files, folders = inspect_tree(image, start)
    assert files["APPS/HELLO.BIN"] == binary and folders == {"APPS", "DOCS"}

    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    config = {"directory": str(directory), "image": str(image), "qemu": qemu, "port": port}
    (directory / "config.json").write_text(json.dumps(config, indent=2), encoding="utf-8")
    launcher = f'@echo off\n"{qemu}" -m 128M -drive "file={image.as_posix()},format=raw,if=ide,index=0" -boot c -display sdl -name "Xinyi test" -nic none\n'
    (directory / "Launch-Xinyi.cmd").write_text(launcher, encoding="utf-8")
    (directory / "TESTING.txt").write_text(
        "This is a persistent virtual disk, booting through its installed BIOS loader.\n"
        "On later boots type mount first.\n"
        "Try: ls, cd docs, pwd, cat readme.txt, cd /, run /apps/hello.bin\n"
        "Create folders/files with mkdir, touch and write. Use delete for files and rmdir for empty folders.\n"
        "Changes on the mounted disk persist. RAM files do not.\n"
        "Use Ctrl+Alt to release the mouse if QEMU captures it.\n", encoding="utf-8")
    print(json.dumps(config))


if __name__ == "__main__":
    main()
