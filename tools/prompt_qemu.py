"""Check directory prompts, backend changes and editing after a wrapped prompt."""
import shutil
import tempfile
import time
from pathlib import Path

from mkfat16 import create_image
from smoke import Guest, ROOT


def main():
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    transcript = []
    with tempfile.TemporaryDirectory(prefix="prompt-", dir=output) as temporary:
        directory = Path(temporary)
        image = directory / "data.img"
        create_image(image)
        guest = Guest(shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe",
                      ROOT / "target/i686-unknown-none/debug/rust-os", directory, disk=image)
        try:
            def check(text, prompt, expected=None):
                screen = guest.command(text, expected or "xinyi:/")
                deadline = time.monotonic() + 5
                while not guest.prompt_ready(screen):
                    assert time.monotonic() < deadline, screen
                    time.sleep(0.02)
                    screen = guest.screen()
                assert prompt in screen.replace("\n", ""), screen
                transcript.append(screen)
                return screen

            check("pwd", "xinyi:/>", "\n/\n")
            check("mkdir docs", "xinyi:/>", "Created directory:")
            check("cd docs", "xinyi:/DOCS>")
            check("pwd", "xinyi:/DOCS>", "\n/DOCS\n")
            check("cd missing", "xinyi:/DOCS>", "Storage:")
            check("mount", "xinyi:/>", "Mounted FAT16")
            check("mkdir diskdir", "xinyi:/>", "Created directory:")
            check("cd diskdir", "xinyi:/DISKDIR>")
            check("unmount", "xinyi:/>", "Unmounted.")
            check("cd docs", "xinyi:/DOCS>")
            path = "/DOCS"
            for _ in range(7):
                check("mkdir abcdefgh.ijk", f"xinyi:{path}>", "Created directory:")
                path += "/ABCDEFGH.IJK"
                check("cd abcdefgh.ijk", f"xinyi:{path}>")
            assert len(path) > 80
            check("echo wrapped", f"xinyi:{path}>", "\nwrapped\n")
            # Edit across an input wrap, then recall it from history.
            text = "x" * 65
            guest.type("clear\necho " + text + "z")
            guest.key("backspace")
            guest.type("\n")
            guest.wait_for(text)
            guest.key("up")
            guest.type("\n")
            screen = guest.wait_for(text)
            assert "z" not in screen, screen
            check("pwd", f"xinyi:{path}>", path[:60])
            check("cd /", "xinyi:/>")
        finally:
            guest.close()
    (output / "prompt-transcript.txt").write_text("\n\n".join(transcript))
    print("PASS directory prompts, pwd, failed cd, mount/unmount, wrapped paths and line editing")


if __name__ == "__main__":
    main()
