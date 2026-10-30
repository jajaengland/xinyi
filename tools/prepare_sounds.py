"""Prepare compact voice assets from sounds/*.wav; originals are never modified.

Requires ffmpeg only when regenerating assets, not when building Xinyi.
"""
import argparse
from array import array
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import wave

ROOT = Path(__file__).resolve().parents[1]
NAMES = ('startup', 'shutdown', 'error', 'warning', 'info', 'complete',
         'connect', 'disconnect', 'empty')


def prepare(source, destination):
    encoded = source.read_bytes()
    with wave.open(str(source), 'rb') as f:
        channels, width, rate, count, compression, _ = f.getparams()
        if width != 2 or channels not in (1, 2) or compression != 'NONE':
            raise ValueError(f'{source}: expected mono/stereo 16-bit PCM WAV')
        if not 8000 <= rate <= 96000 or not 0 < count <= rate * 3:
            raise ValueError(f'{source}: expected a voice clip of at most three seconds')
        samples = array('h', f.readframes(count))
    mono = [sum(samples[i:i+channels]) / channels for i in range(0, len(samples), channels)]
    block = max(1, rate // 100)
    levels = [math.sqrt(sum(v*v for v in mono[i:i+block]) / len(mono[i:i+block]))
              for i in range(0, len(mono), block)]
    threshold = max(100, max(levels) * .025)
    active = [i for i, v in enumerate(levels) if v >= threshold]
    if not active:
        raise ValueError(f'{source}: no speech detected')
    start = max(0, active[0] * block - round(rate * .03))
    end = min(len(mono), (active[-1]+1) * block + round(rate * .06))
    mono = mono[start:end]
    gain = min(4., 23170 / max(abs(v) for v in mono))  # -3 dBFS peak, no clipping
    fade = max(1, round(rate * .005))
    pcm = array('h', (round(v * gain * min(1., i/fade, (len(mono)-1-i)/fade))
                      for i, v in enumerate(mono)))
    # ffmpeg's band-limited resampler prevents aliasing when reducing sample rate.
    result = subprocess.run(['ffmpeg', '-v', 'error', '-f', 's16le', '-ar', str(rate),
                             '-ac', '1', '-i', 'pipe:0', '-f', 's16le', '-ar', '16000',
                             '-ac', '1', 'pipe:1'], input=pcm.tobytes(), capture_output=True, check=True)
    with wave.open(str(destination), 'wb') as f:
        f.setparams((1, 2, 16000, 0, 'NONE', 'not compressed'))
        f.writeframes(result.stdout)
    return dict(source_sha256=hashlib.sha256(encoded).hexdigest(), source_bytes=len(encoded),
                trim_start=round(start/rate, 4), trim_end=round(end/rate, 4),
                gain=round(gain, 4), frames=len(result.stdout)//2,
                bytes=destination.stat().st_size)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output-dir', type=Path, default=ROOT/'assets/sounds')
    a = p.parse_args()
    if not shutil.which('ffmpeg'):
        p.error('ffmpeg is required to regenerate sound assets')
    hashes = [hashlib.sha256((ROOT/'sounds'/f'{n}.wav').read_bytes()).digest() for n in NAMES]
    if len(set(hashes)) != len(hashes):
        p.error('Duplicate source recordings: export separate clips before preparing assets')
    a.output_dir.mkdir(parents=True, exist_ok=True)
    manifest = {n: prepare(ROOT/'sounds'/f'{n}.wav', a.output_dir/f'{n}.wav') for n in NAMES}
    (a.output_dir/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print(f'Prepared {len(NAMES)} clips: {sum(v["source_bytes"] for v in manifest.values())} -> '
          f'{sum(v["bytes"] for v in manifest.values())} bytes')


if __name__ == '__main__':
    main()
