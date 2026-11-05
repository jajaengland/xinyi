"""Measure the same verified desktop storage workload on a private disk copy."""
import argparse,json,re,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT
from seed_apps import seed_apps
from folders_qemu import inspect_tree

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--disk',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/storage-perf';out.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='check-',dir=out) as temporary:
        d=Path(temporary);image=d/'system.img';shutil.copyfile(a.disk,image);pkg=d/'package';(pkg/'apps').mkdir(parents=True)
        source=d/'bench.rs';source.write_text('''#![no_std]
#![no_main]
#[path=SDK]mod x;
fn decimal(out:&mut[u8],mut value:u32){for b in out.iter_mut().rev(){*b=b'0'+(value%10)as u8;value/=10;}}
static mut DATA:[u8;16384]=[0;16384];
static mut READ:[u8;16384]=[0;16384];
fn main()->u32 {
 let data=unsafe{&mut *core::ptr::addr_of_mut!(DATA)};for(i,b)in data.iter_mut().enumerate(){*b=((i*17+11)%251)as u8;}
 let start=x::ticks();
 for i in 0..8 {let name=[b'B',b'0'+i,b'.',b'D',b'A',b'T'];if x::files::save(&name,data)<0{return 1;}}
 let written=x::ticks();let mut listing=x::directory::Listing::new();
 for _ in 0..32 {if x::directory::list(b".",0,&mut listing)<0||listing.count!=8{return 2;}}
 let listed=x::ticks();let read=unsafe{&mut *core::ptr::addr_of_mut!(READ)};
 for i in 0..8 {let name=[b'B',b'0'+i,b'.',b'D',b'A',b'T'];if x::files::read(&name,read)!=16384||read!=data{return 3;}}
 let done=x::ticks();let mut report=[b'0';43];
 decimal(&mut report[..10],written.wrapping_sub(start));report[10]=b',';
 decimal(&mut report[11..21],listed.wrapping_sub(written));report[21]=b',';
 decimal(&mut report[22..32],done.wrapping_sub(listed));report[32]=b',';
 decimal(&mut report[33..43],done.wrapping_sub(start));
 if x::files::save(b"RESULT.TXT",&report)<0{return 4;}0
}
'''.replace('SDK',json.dumps(str(ROOT/'sdk/xinyi.rs').replace('\\','/'))))
        subprocess.run([sys.executable,str(ROOT/'tools/build_program.py'),str(source),'--format','elf','--output',str(pkg/'apps/iobench.elf')],check=True)
        (pkg/'applications.json').write_text(json.dumps([{'file':'IOBENCH.ELF'}]));seed_apps(image,pkg,'TOOLS')
        qemu=shutil.which('qemu-system-i386') or r'C:\Program Files\qemu\qemu-system-i386.exe';g=Guest(qemu,None,d,disk=image)
        try:
            g.command('mkdir perf','Created directory');g.command('cd /perf','xinyi:/PERF>')
            start=len((d/'debug.log').read_text());g.type('desktop\n')
            deadline=time.monotonic()+180
            while 'Desktop scheduler: ready' not in (d/'debug.log').read_text()[start:]:
                assert time.monotonic()<deadline;time.sleep(.05)
            g.key('ctrl-esc');g.type('terminal\n');time.sleep(.5);g.type('run /tools/iobench.elf\n')
            while True:
                assert 'Scheduler: fault' not in (d/'debug.log').read_text(),(d/'debug.log').read_text()[-1800:]
                try:
                    files=inspect_tree(image,2048)[0]
                    if 'PERF/RESULT.TXT' in files:break
                except AssertionError:pass
                assert time.monotonic()<deadline,(d/'debug.log').read_text()[-1800:];time.sleep(.1)
            ticks=list(map(int,files['PERF/RESULT.TXT'].decode().split(',')))
            expected=bytes((i*17+11)%251 for i in range(16384))
            for i in range(8):assert files[f'PERF/B{i}.DAT']==expected
            report=dict(zip(['write_ticks','list_ticks','read_ticks','total_ticks'],ticks));report['tick_hz']=100;report['verified_bytes']=8*16384
            a.output.write_text(json.dumps(report,indent=2));print(json.dumps(report))
        finally:g.close();shutil.copyfile(d/'debug.log',out/(a.output.stem+'-debug.log'))

if __name__=='__main__':main()
