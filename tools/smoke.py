"""Headless QEMU baseline checks. Build the kernel first; uses only stdlib."""
import argparse
import json
from pathlib import Path
import re
import shutil
import socket
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]


class Guest:
    def __init__(self, qemu, kernel, directory, disk=None, attach_disk=True,
                 boot_message="Welcome to Xinyi.", memory_mib=128,
                 disk_index=0, extra_args=None, disk_format="raw"):
        self.directory = directory
        # The fixture is private to this run. Never attach a developer disk.
        if disk is None:
            disk = directory / "fixture.img"
            with disk.open("wb") as stream:
                stream.truncate(10 * 1024 * 1024)
                stream.seek(510)
                stream.write(b"\x55\xaa")
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        self.stderr = (directory / "qemu.stderr").open("w")
        self.process = subprocess.Popen([
            qemu, *(["-kernel", str(kernel)] if kernel is not None else ["-boot", "c"]),
            "-m", f"{memory_mib}M",
            *(extra_args or []),
            "-display", "none", "-monitor", "none", "-serial", "none",
            "-no-reboot", "-nic", "none",
            *(["-drive", f"file={disk.as_posix()},format={disk_format},if=ide,index={disk_index}"] if attach_disk else []),
            "-qmp", f"tcp:127.0.0.1:{port},server=on,wait=off",
            "-debugcon", f"file:{(directory / 'debug.log').as_posix()}",
            "-global", "isa-debugcon.iobase=0xe9",
        ], stdout=subprocess.DEVNULL, stderr=self.stderr,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.connection = None
        try:
            deadline = time.monotonic() + 10
            while True:
                try:
                    self.connection = socket.create_connection(("127.0.0.1", port), timeout=2)
                    break
                except OSError:
                    if self.process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError((directory / "qemu.stderr").read_text())
                    time.sleep(0.05)
            self.reader = self.connection.makefile("rb")
            self.reader.readline()  # QMP greeting
            self.call("qmp_capabilities")
            self.wait_for(boot_message, timeout=45)  # USB EDD loads one sector per BIOS call.
            if boot_message=="Welcome to Xinyi.":
                self.wait_for("xinyi:/",timeout=30)
        except BaseException:
            self.close()
            raise

    def call(self, command, arguments=None):
        self.connection.sendall((json.dumps({"execute": command, "arguments": arguments or {}}) + "\n").encode())
        while True:
            line = self.reader.readline()
            if not line:
                raise RuntimeError("QEMU disconnected")
            reply = json.loads(line)
            if "error" in reply:
                raise RuntimeError(reply["error"])
            if "return" in reply:
                return reply["return"]

    def screen(self):
        path = self.directory / "vga.bin"
        self.call("human-monitor-command", {"command-line": f'pmemsave 0xb8000 4000 "{path.as_posix()}"'})
        data = path.read_bytes()[::2]
        return "\n".join(data[i:i + 80].decode("cp437").rstrip() for i in range(0, 2000, 80))

    def wait_for(self, expected, timeout=5):
        deadline = time.monotonic() + timeout
        while True:
            screen = self.screen()
            if expected in screen:
                return screen
            if time.monotonic() > deadline:
                raise AssertionError(f"Missing {expected!r}:\n{screen}")
            time.sleep(0.05)

    def key(self, key):
        result = self.call("human-monitor-command", {"command-line": f"sendkey {key} 20"})
        if result:
            raise RuntimeError(f"Could not send key {key}: {result}")
        time.sleep(0.045)  # Let the key release complete before sending another.

    def type(self, text):
        for character in text:
            if character == "!":
                self.key("shift-1")
                continue
            key = {" ": "spc", "\n": "ret", "\t": "tab", ".": "dot", ":": "shift-semicolon", "-": "minus", "/": "slash", "\\": "backslash"}.get(character, character)
            if character.isascii() and character.isupper():
                key = "shift-" + character.lower()
            self.key(key)

    def command(self, text, expected):
        self.type("clear\n")
        self.wait_for("xinyi:/")
        self.type(text + "\n")
        return self.wait_for(expected, timeout=30)  # BIOS metadata writes can take several seconds.

    @staticmethod
    def prompt_ready(screen):
        # A maximal path wraps once on the 80-column console.
        return re.search(r"(?m)^xinyi:/[^>\n]*(?:\n[^>\n]*)?>\s*$", screen) is not None

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)
        if hasattr(self, "reader"):
            self.reader.close()
        if self.connection:
            self.connection.close()
        self.stderr.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default=shutil.which("qemu-system-i386"))
    parser.add_argument("--kernel", type=Path, default=ROOT / "target/i686-unknown-none/debug/rust-os")
    args = parser.parse_args()
    if not args.qemu or not args.kernel.is_file():
        parser.error("QEMU and a built kernel are required")
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    try:
        with tempfile.TemporaryDirectory(prefix="run-", dir=output) as temporary:
            directory = Path(temporary)
            for mode in ("shell", "breakpoint", "panic", "usermode"):
                guest = Guest(args.qemu, args.kernel.resolve(), directory)
                try:
                    if mode == "shell":
                        for command, expected in [
                            ("version", "Xinyi kernel version 0.1.0"),
                            ("memory", "0x00100000-0x07fe0000 available"),
                            ("paging", "Paging: enabled"),
                            ("frames", "Freed:"),
                            ("keyboard", "Dropped scancodes:  0"),
                            ("echo baseline", "\nbaseline\n"),
                            ("ver", "Did you mean 'version'?"),
                            ("touch sample", "Created: sample"),
                            ("cat sample", "xinyi:/> cat sample\n\n\nxinyi:/>"),
                            ("cat missing", "Storage: file not found"),
                            ("ls", "SAMPLE       0 bytes"),
                            ("touch sample", "Storage: file already exists"),
                            ("write sample hello", "Wrote 5 bytes"),
                            ("cat sample", "\nhello\n"),
                            ("mount", "Mount failed: no supported FAT16 volume"),
                            ("ps", "Total tasks: 0"),
                            ("spawn", "Created task record 1"),
                            ("run demo", "User program exited with code 0."),
                            ("ps", "Total tasks: 1"),
                            ("disk", "Valid boot signature"),
                        ]:
                            screen = guest.command(command, expected)
                            if command == "frames":
                                before = re.search(r"Free: +(\d+)", screen)[1]
                                after = re.search(r"Free now: +(\d+)", screen)[1]
                                assert before == after, screen
                            transcript.append(f"[{mode}: {command}]\n{screen}\n")
                            print(f"PASS {command}", flush=True)
                        first = guest.command("uptime", "Uptime:")
                        time.sleep(1.1)
                        second = guest.command("uptime", "Uptime:")
                        assert re.search(r"Uptime: (\S+)", first)[1] != re.search(r"Uptime: (\S+)", second)[1]
                        guest.type("clear\nver\t\n")
                        guest.wait_for("Xinyi kernel version")
                        guest.key("up")
                        guest.key("ret")
                        deadline = time.monotonic() + 5
                        while guest.screen().count("Xinyi kernel version") < 2:
                            if time.monotonic() > deadline:
                                raise AssertionError("History recall did not rerun version")
                            time.sleep(0.05)
                        guest.type("echo ax")
                        guest.key("left")
                        guest.key("backspace")
                        guest.key("b")
                        guest.key("ret")
                        guest.wait_for("\nbx\n")
                        transcript.append(f"[shell: uptime, completion, history, editing]\n{guest.screen()}\n")
                        print("PASS uptime, completion, history, editing", flush=True)
                    else:
                        expected = {"breakpoint": "Exception: Breakpoint", "panic": "Xinyi: kernel panic!", "usermode": "User program faulted:"}[mode]
                        screen = guest.command(mode, expected)
                        time.sleep(0.1)
                        debug = (directory / "debug.log").read_text()
                        marker = {"breakpoint": "vector=3", "panic": "Xinyi panic: panic command requested", "usermode": "vector=13"}[mode]
                        assert marker in debug, debug
                        transcript.append(f"[{mode}]\n{screen}\nDEBUG: {debug}\n")
                        if mode == "usermode":
                            assert "Syscall frame: nr=4 args=00000001/00000000/00000000 cs=001b" in debug, debug
                            for number in (20, 4, 0x7fffffff):
                                expected_frame = f"Syscall frame: nr={number} args=11223344/55667788/99aabbcc cs=001b"
                                assert debug.count(expected_frame) == 3, debug
                            assert "cs=0000001b" in debug.lower(), debug
                            print("OBSERVED saved-frame getpid and validated zero-byte write", flush=True)
                        print(f"PASS {mode} diagnostics", flush=True)
                except BaseException:
                    transcript.append(f"[FAILED: {mode}]\n{guest.screen()}\n")
                    raise
                finally:
                    guest.close()
    finally:
        (output / "transcript.txt").write_text("\n".join(transcript), encoding="utf-8")
    print(f"Transcript: {output / 'transcript.txt'}")


if __name__ == "__main__":
    main()
