"""Record actual AC97 output, then exercise media UI, ownership and lifetime."""
import argparse,json,math,re,shutil,struct,subprocess,sys,tempfile,time,wave
from array import array
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='audio-',dir=out) as tmp:
        d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image);extra=d/'extra';(extra/'apps').mkdir(parents=True)
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(ROOT/'programs/audio_check/main.rs'),'--format','elf','--output',str(extra/'apps/audcheck.elf')],check=True)
        with wave.open(str(extra/'apps/demo.wav'),'wb') as f:
            f.setparams((1,2,22050,0,'NONE','not compressed'));f.writeframes(b''.join(int(math.sin(i*2*math.pi*880/22050)*15000).to_bytes(2,'little',signed=True) for i in range(22050)))
        (extra/'applications.json').write_text(json.dumps([{'name':'audcheck','file':'AUDCHECK.ELF'},{'name':'demo','file':'DEMO.WAV'}]));seed_apps(image,extra)
        qemu=r'C:\Program Files\qemu\qemu-system-i386.exe'
        g=Guest(qemu,None,d,disk=image)
        try:g.command('run --media audcheck missing','Missing audio checks passed!');g.wait_for('User program exited with code 0.')
        finally:g.close()
        def audio_args(path):return ['-audiodev',f'wav,id=pcm,path={path.as_posix()},out.frequency=48000,out.channels=2,out.format=s16','-device','AC97,audiodev=pcm']
        def finish(g):
            # QEMU must finalize the WAV RIFF lengths before we inspect the capture.
            try:g.call('quit');g.process.wait(timeout=10)
            finally:g.close()
        def finalize_capture(path):
            data=bytearray(path.read_bytes())
            assert len(data)>44 and data[:4]==b'RIFF' and data[8:16]==b'WAVEfmt ' and data[36:40]==b'data'
            assert (len(data)-44)%4==0
            # This Windows QEMU build leaves placeholder lengths even on QMP quit.
            # Complete only container metadata; never manufacture/alter PCM samples.
            if data[4:8]==bytes(4) and data[40:44]==bytes(4):
                struct.pack_into('<I',data,4,len(data)-8);struct.pack_into('<I',data,40,len(data)-44);path.write_bytes(data)
        pcm=d/'pcm.wav';g=Guest(qemu,None,d,disk=image,extra_args=audio_args(pcm))
        try:
            assert 'Audio: AC97 PCM ready' in (d/'debug.log').read_text(),(d/'debug.log').read_text()
            def frames():return int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            baseline=frames()
            g.command('run --media audcheck','PCM audio checks passed!');g.wait_for('User program exited with code 0.');assert frames()==baseline
            g.command('run --media audcheck fault','User program faulted:');assert frames()==baseline
            g.command('run --media audcheck','PCM audio checks passed!');g.wait_for('User program exited with code 0.');assert frames()==baseline
        finally:finish(g);shutil.copyfile(d/'debug.log',out/'audio-pcm-debug.log')
        finalize_capture(pcm)
        shutil.copyfile(pcm,out/'audio-pcm.wav')
        with wave.open(str(pcm),'rb') as f:
            assert f.getframerate()==48000 and f.getnchannels()==2 and f.getsampwidth()==2
            samples=array('h');samples.frombytes(f.readframes(f.getnframes()))
        nonzero=[v for v in samples[::2] if abs(v)>100]
        assert len(nonzero)>48000,'No actual PCM output or playback truncated'
        assert max(nonzero)>5000 and min(nonzero)<-5000,'PCM output clipped/muted or wrong sample interpretation'
        shutil.copyfile(pcm,out/'audio-pcm.wav')
        capture=d/'player.wav';g=Guest(qemu,None,d,disk=image,extra_args=audio_args(capture))
        try:
            def log():return (d/'debug.log').read_text()
            def until(fn,msg):
                end=time.monotonic()+45
                while not fn():
                    assert time.monotonic()<end,msg+'\n'+log()[-1600:];time.sleep(.04)
            baseline=frames();g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            files,_=inspect_tree(image,2048);names=sorted(k.split('/')[-1][:-4] for k in files if k.startswith('APPS/') and k.endswith(('.BIN','.ELF')))
            def launch(name):
                start=len(log());g.key('ctrl-esc');g.key('home')
                for _ in range(names.index(name)):g.key('down')
                g.key('ret');until(lambda:re.search(r'Desktop: launch pid=(\d+) path=/APPS/(?:[A-Z0-9_]+/)?'+name,log()[start:]),'launch '+name)
                pid=int(re.search(r'Desktop: launch pid=(\d+) ',log()[start:])[1]);until(lambda:f'Multiwindow: open pid={pid} ' in log(),'window '+name);return pid
            def window(pid):
                m=re.search(r'Multiwindow: open pid='+str(pid)+r' handle=\d+ surface=([0-9a-f]+) client=(\d+)x(\d+)',log());return int(m[1],16),int(m[2]),int(m[3])
            def memory(p,n):
                f=d/'memory.bin';g.call('human-monitor-command',{'command-line':f'pmemsave {p:#x} {n} "{f.as_posix()}"'});return f.read_bytes()
            counter=launch('COUNTER');player=launch('PLAYER');g.key('t');until(lambda:f'Audio: open pid={player} stream=1' in log(),'bundled dndt.wav did not start')
            pointer,w,_=window(counter);before=memory(pointer+w*92*4,w*18*4);terminal=launch('TERMINAL')
            # Restart the short bundled clip just before submitting the ownership probe.
            g.type('run --media audcheck busy');g.key('alt-shift-tab');g.key('t');g.key('alt-tab');g.key('ret');until(lambda:'Desktop: terminal launch pid=' in log(),'busy probe launch')
            probe=int(re.findall(r'Desktop: terminal launch pid=(\d+) ',log())[-1]);until(lambda:f'Scheduler: exit pid={probe} code=0' in log(),'foreign stream control not refused')
            assert memory(pointer+w*92*4,w*18*4)!=before,'Counter froze during audio'
            # Alt+Tab cycles stable root order Counter -> Player -> Terminal.
            g.key('alt-shift-tab');start=len(log());g.key('ctrl-o');g.type('/apps/demo.wav\n')
            until(lambda:f'Audio: open pid={player} stream=3' in log()[start:],'WAV did not start')
            until(lambda:f'Audio: closed pid={player} stream=3' in log()[start:],'WAV failed to drain')
            g.call('screendump',{'filename':str(out/'audio-player.png'),'format':'png'})
            start=len(log());g.key('t');until(lambda:f'Audio: open pid={player} stream=4' in log()[start:],'restart sound')
            g.key('ctrl-alt-k');until(lambda:f'Audio: owner {player} closed' in log()[start:],'forced termination left PCM running')
            until(lambda:f'Scheduler: forced exit pid={player}' in log(),'player did not exit')
            start=len(log());g.type('run --media audcheck\n');until(lambda:re.search(r'Desktop: terminal launch pid=(\d+)',log()[start:]),'new audio owner')
            probe=int(re.search(r'Desktop: terminal launch pid=(\d+)',log()[start:])[1]);until(lambda:f'Scheduler: exit pid={probe} code=0' in log(),'audio stream not reclaimed')
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={terminal} code=0' in log(),'terminal close')
            g.key('alt-f4');until(lambda:f'Scheduler: exit pid={counter} code=0' in log(),'counter close');time.sleep(.3)
            g.key('esc')
            g.wait_for('Desktop finished.');assert frames()==baseline,'audio/app buffers leaked'
        finally:finish(g);shutil.copyfile(d/'debug.log',out/'audio-player-debug.log')
        finalize_capture(capture)
        with wave.open(str(capture),'rb') as f:
            data=array('h');data.frombytes(f.readframes(f.getnframes()));assert sum(v*v for v in data)>1000000,'Player produced no audio'
        left=data[::2];window=4800
        cosine=[math.cos(2*math.pi*880*i/48000) for i in range(window)];sine=[math.sin(2*math.pi*880*i/48000) for i in range(window)]
        # Find the resampled 880 Hz WAV, rather than accepting only the bundled recording.
        detected=False
        for start in range(0,len(left)-window,window//2):
            c=sum(left[start+i]*cosine[i] for i in range(window));s=sum(left[start+i]*sine[i] for i in range(window))
            if math.hypot(c,s)*2/window>900:detected=True;break
        assert detected,'The selected mono 22050 Hz WAV was not converted to audible 48000 Hz stereo'

        shutil.copyfile(capture,out/'audio-player.wav')
    print('PASS AC97: actual recorded PCM, ring wrap/backpressure, invalid buffers, missing device, media permissions, exclusive ownership, WAV conversion, live desktop, fault/kill cleanup and frame balance')

if __name__=='__main__':main()
