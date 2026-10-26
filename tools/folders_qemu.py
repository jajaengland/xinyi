"""Verify nested FAT16/RAM folders, host interoperability and cold boots."""
import argparse
from pathlib import Path
import shutil
import struct
import tempfile
import time

from mkfat16 import create_image
from smoke import Guest, ROOT

APP = bytes.fromhex("b801000000bb2a000000cd80")


def seed_host_folder(image, start):
    with image.open("r+b") as stream:
        def write(sector, data):
            stream.seek(sector * 512)
            stream.write(data)
        for copy in range(2):
            stream.seek((start + 1 + copy * 128) * 512 + 4)
            stream.write(b"\xff\xff\xff\xff")
        root = bytearray(512)
        root[:11], root[11] = b"HOST       ", 0x10
        struct.pack_into("<H", root, 26, 2)
        write(start + 257, root)
        directory = bytearray(512)
        for offset, name, cluster, attr, size in [(0, b".          ", 2, 0x10, 0),
                (32, b"..         ", 0, 0x10, 0), (64, b"APP     BIN", 3, 0x20, len(APP))]:
            directory[offset:offset + 11] = name
            directory[offset + 11] = attr
            struct.pack_into("<H", directory, offset + 26, cluster)
            struct.pack_into("<I", directory, offset + 28, size)
        write(start + 289, directory)
        write(start + 290, APP)


def inspect_tree(image, start):
    data = image.read_bytes()
    boot = data[start * 512:(start + 1) * 512]
    spc = boot[13]
    reserved, entries, spf = (struct.unpack_from("<H", boot, offset)[0] for offset in (14, 17, 22))
    total = struct.unpack_from("<H", boot, 19)[0] or struct.unpack_from("<I", boot, 32)[0]
    fat = data[(start + reserved) * 512:(start + reserved + spf) * 512]
    assert fat == data[(start + reserved + spf) * 512:(start + reserved + 2 * spf) * 512]
    root = start + reserved + 2 * spf
    first = root + (entries * 32 + 511) // 512
    count = (total - (first - start)) // spc
    allocated, files, folders = set(), {}, set()

    def chain(cluster):
        content = bytearray()
        while cluster:
            assert 2 <= cluster <= count + 1 and cluster not in allocated, "Cross-linked/cyclic chain"
            allocated.add(cluster)
            offset = (first + (cluster - 2) * spc) * 512
            content.extend(data[offset:offset + spc * 512])
            cluster = struct.unpack_from("<H", fat, cluster * 2)[0]
            if cluster >= 0xfff8:
                break
            assert cluster >= 2
        return content

    def directory(content, path, own, parent):
        if own:
            assert content[:11] == b".          " and content[32:43] == b"..         "
            assert struct.unpack_from("<H", content, 26)[0] == own
            assert struct.unpack_from("<H", content, 58)[0] == parent
        for offset in range(0, len(content), 32):
            entry = content[offset:offset + 32]
            if entry[0] == 0:
                break
            if entry[0] in (0xe5, ord('.')) or entry[11] & 8:
                continue
            name = entry[:8].decode().rstrip()
            extension = entry[8:11].decode().rstrip()
            if extension:
                name += "." + extension
            full = path + name
            cluster = struct.unpack_from("<H", entry, 26)[0]
            size = struct.unpack_from("<I", entry, 28)[0]
            payload = chain(cluster)
            if entry[11] & 0x10:
                assert size == 0
                folders.add(full)
                directory(payload, full + "/", cluster, own)
            else:
                assert len(payload) >= size
                files[full] = bytes(payload[:size])
    directory(data[root * 512:root * 512 + entries * 32], "", 0, 0)
    used = {cluster for cluster in range(2, count + 2) if struct.unpack_from("<H", fat, cluster * 2)[0] != 0}
    assert allocated == used, "Leaked/unreferenced clusters"
    return files, folders


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel", type=Path, default=ROOT / "target/i686-unknown-none/debug/rust-os")
    parser.add_argument("--corrupt-only", action="store_true", help="Run only the corrupt-folder fallback check")
    args = parser.parse_args()
    if not args.qemu or not args.kernel.is_file():
        parser.error("Build the kernel and put QEMU on PATH")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="folders-", dir=output) as temporary:
            directory = Path(temporary)
            for partitioned in (() if args.corrupt_only else (False, True)):
                image = directory / ("partition.img" if partitioned else "volume.img")
                start = 63 if partitioned else 0
                create_image(image, partitioned)
                seed_host_folder(image, start)
                original_boot = image.read_bytes()[:512]
                for boot in (1, 2):
                    guest = Guest(args.qemu, args.kernel.resolve(), directory, disk=image)
                    try:
                        def command(text, expected=None):
                            screen = guest.command(text, expected or f"> {text}")
                            deadline = time.monotonic() + 5
                            while not guest.prompt_ready(screen):
                                assert time.monotonic() < deadline, screen
                                time.sleep(0.01)
                                screen = guest.screen()
                            transcript.append(f"[{partitioned=} {boot=}: {text}]\n{screen}")
                            return screen

                        if boot == 1:
                            command("unmount", "Unmounted.")
                            command("mkdir ram", "Created directory: ram")
                            command("cd ram")
                            command("touch ramonly", "Created: ramonly")
                        command("mount", f"Mounted FAT16 at sector {start}")
                        command("pwd", "\n/\n")
                        command("cd host")
                        command("pwd", "\n/HOST\n")
                        command("run app.bin", "User program exited with code 42.")
                        command("cd /")
                        if boot == 1:
                            command("mkdir docs", "Created directory: docs")
                            command("mkdir docs/sub", "Created directory: docs/sub")
                            command("touch same.txt", "Created: same.txt")
                            command("write same.txt root", "Wrote 4 bytes")
                            command("touch docs/same.txt", "Created: docs/same.txt")
                            command("write docs/same.txt docs", "Wrote 4 bytes")
                            command("cd docs/sub")
                            command("pwd", "\n/DOCS/SUB\n")
                            command("touch note.txt", "Created: note.txt")
                            command("write note.txt nested data", "Wrote 11 bytes")
                            command("rmdir .", "Storage: cannot remove the root, current directory or an ancestor")
                            command("rmdir /docs", "Storage: cannot remove the root, current directory or an ancestor")
                            command("run ../../host/app.bin", "User program exited with code 42.")
                            command("cd ../..")
                            command("rmdir docs", "Storage: directory is not empty")
                            command("delete docs", "Storage: path is a directory")
                            command("cd same.txt", "Storage: path is not a directory")
                            command("pwd", "\n/\n")
                            command("ls docs", "SUB/  <DIR>")
                            command("cd \\docs\\sub")
                            command("cat note.txt", "\nnested data\n")
                            command("unmount", "File commands now use RAM")
                            command("pwd", "\n/\n")
                            command("ls /ram", "RAMONLY")
                            command("cd ram")
                            command("rmdir .", "Storage: cannot remove the root, current directory or an ancestor")
                            command("cd")
                            command("delete ram/ramonly", "Deleted: ram/ramonly")
                            command("rmdir ram", "Removed directory: ram")
                        else:
                            command("cat docs/sub/note.txt", "\nnested data\n")
                            command("cat docs/same.txt", "\ndocs\n")
                            command("cat same.txt", "\nroot\n")
                            command("delete docs/sub/note.txt", "Deleted: docs/sub/note.txt")
                            command("rmdir docs/sub", "Removed directory: docs/sub")
                            command("delete docs/same.txt", "Deleted: docs/same.txt")
                            command("rmdir docs", "Removed directory: docs")
                            command("rmdir host", "Storage: directory is not empty")
                    finally:
                        guest.close()
                    assert image.read_bytes()[:512] == original_boot
                    files, folders = inspect_tree(image, start)
                    expected = {"HOST/APP.BIN": APP, "SAME.TXT": b"root"}
                    if boot == 1:
                        expected.update({"DOCS/SAME.TXT": b"docs", "DOCS/SUB/NOTE.TXT": b"nested data"})
                    assert files == expected, files
                    assert folders == ({"HOST", "DOCS", "DOCS/SUB"} if boot == 1 else {"HOST"}), folders
                    print(f"PASS {'MBR' if partitioned else 'whole-volume'} cold boot {boot}: folders, file paths, program execution, mirrors and no leaked clusters", flush=True)
            # A malformed host directory must detach disk storage and reset
            # the working directory, never leave a stale disk location in RAM.
            damaged = directory / "damaged.img"
            create_image(damaged)
            seed_host_folder(damaged, 0)
            with damaged.open("r+b") as stream:
                stream.seek(289 * 512 + 58)
                stream.write(b"\x01")
            original = damaged.read_bytes()
            guest = Guest(args.qemu, args.kernel.resolve(), directory, disk=damaged)
            try:
                transcript.append(guest.command("mount", "Mounted FAT16 at sector 0"))
                transcript.append(guest.command("cd host", "Storage: invalid FAT chain or inconsistent metadata"))
                transcript.append(guest.command("pwd", "\n/\n"))
                screen = guest.command("ls", "0 file(s)")
                assert "RAM, not persistent" in screen, screen
                transcript.append(screen)
                transcript.append(guest.command("mkdir safe", "Created directory: safe"))
            finally:
                guest.close()
            assert damaged.read_bytes() == original
            print("PASS corrupt folder: disk detached, cwd reset, usable RAM fallback, no writes", flush=True)
            oem = directory / "oem.img"
            create_image(oem)
            seed_host_folder(oem, 0)
            with oem.open("r+b") as stream:
                stream.seek(257 * 512)
                stream.write(b"\x82")
            guest = Guest(args.qemu, args.kernel.resolve(), directory, disk=oem)
            try:
                transcript.append(guest.command("mount", "Mounted FAT16 at sector 0"))
                transcript.append(guest.command("ls", "(non-ASCII)/  <DIR>"))
                transcript.append(guest.command("echo usable", "\nusable\n"))
            finally:
                guest.close()
            print("PASS OEM folder name: placeholder displayed and shell remains usable", flush=True)
    finally:
        transcript_path = output / ("folders-errors-transcript.txt" if args.corrupt_only else "folders-transcript.txt")
        transcript_path.write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {transcript_path}")


if __name__ == "__main__":
    main()
