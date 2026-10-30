# Prepared system voices

These are generated playback copies of the original recordings in `/sounds`.
Keep the originals when changing the sounds. Regenerate with:

```
python tools/prepare_sounds.py
```

Regeneration needs ffmpeg. Normal kernel builds do not: they embed these files.
The manifest records each source SHA-256, trim interval, gain and output size.
Each output is a canonical 44-byte WAV header followed by 16 kHz mono signed
16-bit little-endian PCM. Do not replace the files directly with other WAV formats;
use the preparation script, then rebuild the kernel.

Preparation retains 30 ms before and 60 ms after detected speech where available,
balances peaks to -3 dBFS (gain capped at 4), adds 5 ms edge fades and resamples
with an anti-aliasing filter. There is no lossy codec or dynamic compression.

The filename selects the event: startup, shutdown, error, warning, info, complete,
connect, disconnect or empty. Notifications reuse info. Connect/disconnect currently
refer to explicit filesystem mount/unmount actions, not general USB hotplug support.
