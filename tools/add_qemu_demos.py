"""Add demos to a closed manual QEMU session, preserving its existing files."""
import argparse
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import time

from folders_qemu import inspect_tree
from smoke import Guest, ROOT


def command(guest, text, expected):
    screen = guest.command(text, expected)
    deadline = time.monotonic() + 5
    while not guest.prompt_ready(screen):
        assert time.monotonic() < deadline, screen
        time.sleep(0.02)
        screen = guest.screen()
    return screen


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    image = Path(config["image"]).resolve()
    image.relative_to((ROOT / "target/manual-qemu").resolve())
    original = image.read_bytes()
    start = struct.unpack_from("<I", original, 454)[0]
    before, folders = inspect_tree(image, start)
    assert "APPS" in folders
    with tempfile.TemporaryDirectory(prefix="demos-", dir=image.parent) as temporary:
        directory = Path(temporary)
        files = {}
        for source, name in [("hello", "HELLO.BIN"), ("count", "COUNT.BIN"),
                             ("check_io", "CHECK.BIN"), ("pid", "PID.BIN")]:
            binary = directory / name
            subprocess.run([sys.executable, str(ROOT / "tools/build_program.py"),
                            str(ROOT / f"programs/{source}/main.rs"), "--output", str(binary)], check=True)
            files[name] = binary.read_bytes()
        files["FAULT.BIN"] = b"\x0f\x0b"  # Deliberate UD2, recovered by the kernel.
        files["DEMOS.TXT"] = (
            b"Run /apps/hello.bin: greeting\nRun /apps/count.bin: counts to ten\n"
            b"Run /apps/check.bin: console and buffer checks\n"
            b"Run /apps/pid.bin: exits with its process ID\n"
            b"Run /apps/fault.bin: deliberate fault, returns to shell\n")
        assert all(0 < len(payload) <= 1024 for payload in files.values())
        for name, payload in files.items():
            existing = before.get("APPS/" + name)
            if existing is not None and existing != payload:
                raise RuntimeError(f"Refusing to overwrite existing APPS/{name}")
        trial = directory / "system.img"
        trial.write_bytes(original)
        guest = Guest(config["qemu"], None, directory, disk=trial)
        try:
            command(guest, "mount", "Mounted FAT16")
            for name in files:
                if "APPS/" + name not in before:
                    command(guest, "touch apps/" + name.lower(), "Created:")
            command(guest, "unmount", "Unmounted.")
        finally:
            guest.close()

        data = bytearray(trial.read_bytes())
        boot = start * 512
        spc, fats = data[boot + 13], data[boot + 16]
        reserved, entries, spf = (struct.unpack_from("<H", data, boot + offset)[0]
                                  for offset in (14, 17, 22))
        total = struct.unpack_from("<H", data, boot + 19)[0] or struct.unpack_from("<I", data, boot + 32)[0]
        fat = start + reserved
        root = fat + fats * spf
        first = root + (entries * 32 + 511) // 512
        count = (total - (first - start)) // spc
        def find(offset, slots, name):
            for slot in range(slots):
                entry = offset + slot * 32
                if data[entry:entry + 11] == name:
                    return entry
            raise RuntimeError(f"Missing entry {name!r}")
        apps = find(root * 512, entries, b"APPS       ")
        cluster = struct.unpack_from("<H", data, apps + 26)[0]
        for name, payload in files.items():
            if "APPS/" + name in before:
                continue
            base, extension = name.split(".")
            entry = find((first + (cluster - 2) * spc) * 512, spc * 16,
                         (base.ljust(8) + extension.ljust(3)).encode())
            assert struct.unpack_from("<I", data, entry + 28)[0] == 0
            needed = (len(payload) + spc * 512 - 1) // (spc * 512)
            free = [c for c in range(2, count + 2)
                    if struct.unpack_from("<H", data, fat * 512 + c * 2)[0] == 0][:needed]
            assert len(free) == needed
            for index, current in enumerate(free):
                value = free[index + 1] if index + 1 < needed else 0xffff
                for copy in range(fats):
                    struct.pack_into("<H", data, (fat + copy * spf) * 512 + current * 2, value)
                offset = (first + (current - 2) * spc) * 512
                chunk = payload[index * spc * 512:(index + 1) * spc * 512]
                data[offset:offset + spc * 512] = chunk.ljust(spc * 512, b"\0")
            struct.pack_into("<H", data, entry + 26, free[0])
            struct.pack_into("<I", data, entry + 28, len(payload))
        trial.write_bytes(data)
        after, _ = inspect_tree(trial, start)
        assert all(after[name] == payload for name, payload in before.items())
        assert all(after["APPS/" + name] == payload for name, payload in files.items())
        transcript = []
        guest = Guest(config["qemu"], None, directory, disk=trial)
        try:
            command(guest, "mount", "Mounted FAT16")
            for name, expected in [("hello", "User program exited with code 0."),
                                   ("count", "User program exited with code 0."),
                                   ("check", "Console checks passed!"),
                                   ("pid", "User program exited with code 4."),
                                   ("fault", "User program faulted: vector=6")]:
                transcript.append(command(guest, f"run /apps/{name}.bin", expected))
            transcript.append(command(guest, "echo recovered", "\nrecovered\n"))
        finally:
            guest.close()
        # Publish only if the original has remained unchanged while preparing.
        assert image.read_bytes() == original, "Original disk changed; close QEMU before retrying"
        backup = image.with_name("before-demos-" + str(time.time_ns()) + ".img")
        shutil.copyfile(image, backup)
        with image.open("r+b") as stream:
            stream.write(trial.read_bytes())
        (image.parent / "demos-transcript.txt").write_text("\n\n".join(transcript))
        print(f"Added and tested demos in {image}; backup: {backup}")


if __name__ == "__main__":
    main()
