"""Verify HDA codec discovery and actual PCM output in a disposable QEMU guest."""
import argparse
from array import array
import math
from pathlib import Path
import re
import shutil
import struct
import tempfile
import wave

from smoke import Guest, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disk", type=Path, required=True)
    parser.add_argument("--usb", action="store_true", help="Boot through BIOS USB mass storage")
    args = parser.parse_args()
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    qemu = shutil.which("qemu-system-i386") or r"C:\Program Files\qemu\qemu-system-i386.exe"
    with tempfile.TemporaryDirectory(prefix="hda-", dir=output) as work:
        directory = Path(work)
        disk = directory / "xinyi.img"
        capture = directory / "tone.wav"
        shutil.copyfile(args.disk, disk)
        devices = [
            "-audiodev", f"wav,id=hda,path={capture.as_posix()},out.frequency=48000,out.channels=2,out.format=s16",
            "-device", "intel-hda", "-device", "hda-output,audiodev=hda",
        ]
        if args.usb:
            devices += [
                "-drive", f"if=none,id=stick,file={disk.as_posix()},format=raw",
                "-device", "piix3-usb-uhci,id=uhci",
                "-device", "usb-storage,bus=uhci.0,drive=stick,bootindex=1",
            ]
        guest = Guest(qemu, None, directory, disk=disk, attach_disk=not args.usb, extra_args=devices)
        try:
            guest.command("audio", "PCM ready")
            guest.wait_for("xinyi:/", timeout=5)
            probe = guest.command("audio probe", "HD Audio codec 0:")
            print(probe.strip(), flush=True)
            guest.wait_for("pin", timeout=5)
            guest.wait_for("xinyi:/", timeout=5)
            baseline = guest.command("frames", "Free now:")
            for _ in range(2):
                guest.command("audio test", "HD Audio test tone completed.")
                guest.wait_for("xinyi:/", timeout=5)
            after = guest.command("frames", "Free now:")
            assert re.findall(r"Free now: +(\d+)", baseline)[-1] == re.findall(r"Free now: +(\d+)", after)[-1]
        finally:
            try:
                guest.call("quit")
                guest.process.wait(timeout=10)
            finally:
                guest.close()
                shutil.copyfile(directory / "debug.log", output / "hda-debug.log")

        data = bytearray(capture.read_bytes())
        assert len(data) > 44 and data[:4] == b"RIFF" and data[36:40] == b"data"
        if data[4:8] == bytes(4) and data[40:44] == bytes(4):
            # Some Windows QEMU builds leave the RIFF lengths unset.
            struct.pack_into("<I", data, 4, len(data) - 8)
            struct.pack_into("<I", data, 40, len(data) - 44)
            capture.write_bytes(data)
        with wave.open(str(capture), "rb") as wav:
            assert (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (48000, 2, 2)
            pcm = array("h", wav.readframes(wav.getnframes()))
        samples = pcm[::2]
        assert sum(abs(sample) > 100 for sample in samples) > 12000, "No sustained PCM tone"
        count = 4800
        cosine = [math.cos(2 * math.pi * i / 80) for i in range(count)]
        sine = [math.sin(2 * math.pi * i / 80) for i in range(count)]
        amplitude = max(
            math.hypot(
                sum(samples[start+i]*cosine[i] for i in range(count)),
                sum(samples[start+i]*sine[i] for i in range(count)),
            ) * 2 / count
            for start in range(0, len(samples) - count, count // 2)
        )
        assert amplitude > 300, f"Expected an audible 600 Hz signal, measured {amplitude:.1f}"
        shutil.copyfile(capture, output / "hda-tone.wav")
    print("PASS HD Audio codec, analog route, recorded 600 Hz tone and repeatable DMA cleanup")


if __name__ == "__main__":
    main()
