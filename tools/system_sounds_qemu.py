"""Record voice playback and verify mute, debounce, volume persistence and shutdown.

Uses NumPy to compare actual QEMU PCM output with each prepared voice waveform.
"""
import argparse
import json
from pathlib import Path
import shutil
import struct
import tempfile
import time
import wave
import numpy as np
from smoke import Guest, ROOT


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--disk',type=Path,required=True)
    a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='voices-',dir=out) as tmp:
        d=Path(tmp);disk=d/'system.img';shutil.copyfile(a.disk,disk)
        capture=d/'voices.wav'
        args=['-audiodev',f'wav,id=voice,path={capture.as_posix()},out.frequency=48000,out.channels=2,out.format=s16',
              '-device','AC97,audiodev=voice']
        g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=disk,extra_args=args)
        def log():return (d/'debug.log').read_text(errors='replace')
        def until(fn,description):
            end=time.monotonic()+10
            while not fn():
                assert time.monotonic()<end,description+'\n'+log()[-1800:]
                time.sleep(.02)
        try:
            until(lambda:'Sound: play startup' in log() and 'Sound: finished' in log(),'startup voice')
            for name in ('error','warning','info','complete','connect','disconnect','empty'):
                time.sleep(.8);start=len(log());g.type('sounds test '+name+'\n')
                until(lambda:f'Sound: play {name}' in log()[start:] and 'Sound: finished' in log()[start:],name+' voice')
            time.sleep(.8);start=len(log());g.type('sounds test notification\n')
            until(lambda:'Sound: play info' in log()[start:] and 'Sound: finished' in log()[start:],'notification alias')
            time.sleep(.8);start=len(log());g.type('a\n');g.key('up');g.key('ret')
            until(lambda:'Sound: finished' in log()[start:],'error debounce')
            assert log()[start:].count('Sound: play error')==1,'Repeated errors stacked voices'
            g.command('sounds mute','xinyi:/>');start=len(log());g.type('sounds test error\n');time.sleep(.8)
            assert 'Sound: play' not in log()[start:],'Muted sound played'
            g.command('sounds volume 40','xinyi:/>')
            g.command('sounds save','System sound volume saved.')
            assert 'System sounds volume: 40%' in g.command('sounds','System sounds volume:')
            start=len(log());g.type('shutdown\n');g.process.wait(timeout=15)
            assert 'Sound: play shutdown' in log()[start:] and 'Sound: finished' in log()[start:],'Shutdown voice cut off'
            assert 'Power: disks synchronized' in log()[start:]
        finally:
            if g.process.poll() is None:
                try:g.call('quit');g.process.wait(timeout=10)
                except (OSError,RuntimeError):pass
            g.close();shutil.copyfile(d/'debug.log',out/'system-sounds-debug.log')
        # Windows QEMU leaves these two header sizes zero; repair metadata only.
        data=bytearray(capture.read_bytes())
        assert data[:4]==b'RIFF' and data[36:40]==b'data' and (len(data)-44)%4==0
        if data[4:8]==bytes(4) and data[40:44]==bytes(4):
            struct.pack_into('<I',data,4,len(data)-8);struct.pack_into('<I',data,40,len(data)-44)
            capture.write_bytes(data)
        with wave.open(str(capture),'rb') as f:
            assert f.getframerate()==48000 and f.getnchannels()==2
            stereo=np.frombuffer(f.readframes(f.getnframes()),dtype='<i2').astype(np.float64).reshape(-1,2)
        assert np.array_equal(stereo[:,0],stereo[:,1]),'System voices must duplicate mono to stereo'
        signal=stereo[:,0]
        matches={}
        for name in ('startup','shutdown','error','warning','info','complete','connect','disconnect','empty'):
            with wave.open(str(ROOT/'assets/sounds'/f'{name}.wav'),'rb') as f:
                reference=np.frombuffer(f.readframes(f.getnframes()),dtype='<i2').astype(np.float64)
            energy=float(reference@reference);best=0.
            for phase in range(3):
                recorded=signal[phase::3];m=len(reference);n=1<<(len(recorded)+m-2).bit_length()
                corr=np.fft.irfft(np.fft.rfft(recorded,n)*np.fft.rfft(reference[::-1],n),n)[m-1:len(recorded)]
                cumulative=np.concatenate(([0.],np.cumsum(recorded*recorded)))
                local=cumulative[m:]-cumulative[:-m]
                score=corr/np.sqrt(np.maximum(local*energy,1.))
                best=max(best,float(score.max()))
            matches[name]=round(best,4)
            assert best>.9,f'{name}: actual audio does not match the supplied voice ({best:.3f})'
        shutil.copyfile(capture,out/'system-sounds.wav')
        (out/'system-sounds-matches.json').write_text(json.dumps(matches,indent=2)+'\n')
        # Rebooting the same private disk restores the saved system volume.
        g=Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=disk)
        try:
            assert 'System sounds volume: 40%' in g.command('sounds','System sounds volume:')
            g.type('sounds test error\n')  # Missing hardware remains harmless.
            g.command('version','Xinyi kernel version')
        finally:g.close()
    print('PASS system sounds: all nine recorded voice waveforms, notification alias, error debounce, mute, saved volume, completed shutdown clip and missing-device fallback')


if __name__=='__main__':main()
