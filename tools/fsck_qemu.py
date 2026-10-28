"""Audit and repair only disposable image copies; verify bytes across cold boots."""
import argparse
from pathlib import Path
import struct
import tempfile
from smoke import Guest, ROOT


def layout(data):
    start=struct.unpack_from('<I',data,454)[0]
    b=start*512
    reserved,entries,spf=(struct.unpack_from('<H',data,b+i)[0] for i in (14,17,22))
    return start+reserved, spf, start+reserved+data[b+16]*spf


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--disk',type=Path,required=True)
    p.add_argument('--usb',action='store_true',help='Test BIOS USB instead of BIOS/native IDE')
    a=p.parse_args()
    (ROOT/'target/smoke').mkdir(exist_ok=True)
    qemu=r'C:\Program Files\qemu\qemu-system-i386.exe'
    with tempfile.TemporaryDirectory(prefix='fsck-',dir=ROOT/'target/smoke') as tmp:
        directory=Path(tmp);image=directory/'system.img'
        pristine=a.disk.read_bytes();fat,spf,root=layout(pristine)
        extra=[]
        if a.usb:
            extra=['-drive',f'if=none,id=stick,file={image.as_posix()},format=raw',
                   '-device','piix3-usb-uhci,id=uhci',
                   '-device','usb-storage,bus=uhci.0,drive=stick,bootindex=1']
        for native in ([False] if a.usb else [False,True]):
            image.write_bytes(pristine)
            g=Guest(qemu,None,directory,disk=image,attach_disk=not a.usb,extra_args=extra)
            try:
                if native:g.command('use 1','ready')
                g.command('fsck','Read-only scan; no changes made.')
                assert 'fsck: clean' in g.screen(),g.screen()
                g.command('fsck repair-orphans','unmount')
                g.command('unmount','Unmounted.')
                g.command('fsck repair-orphans','No orphan clusters to reclaim.')
            finally:g.close()
            assert image.read_bytes()==pristine,'Read-only commands changed the disk'

            # One full orphan and one half-updated orphan, beyond the APPS allocation.
            damaged=bytearray(pristine)
            free=[i for i in range(1000,2000) if struct.unpack_from('<H',damaged,fat*512+i*2)[0]==0][:2]
            assert len(free)==2
            for copy in [0,1]:struct.pack_into('<H',damaged,(fat+copy*spf)*512+free[0]*2,0xffff)
            struct.pack_into('<H',damaged,fat*512+free[1]*2,0xffff)
            image.write_bytes(damaged)
            g=Guest(qemu,None,directory,disk=image,attach_disk=not a.usb,extra_args=extra)
            try:
                if native:g.command('use 1','ready')
                g.command('fsck','Read-only scan; no changes made.')
                assert '2 orphan' in g.screen(),g.screen()
                assert 'Damaged volume detached' in g.screen(),g.screen()
                g.command('fsck repair-orphans','To apply, enter: fsck confirm')
                g.command('pwd','/') # Any intervening command cancels the preview.
                g.command('fsck confirm','fsck failed: run fsck repair-orphans')
            finally:g.close()
            assert image.read_bytes()==damaged,'Preview/canceled confirmation wrote disk bytes'
            g=Guest(qemu,None,directory,disk=image,attach_disk=not a.usb,extra_args=extra)
            try:
                if native:g.command('use 1','ready')
                g.command('unmount','Unmounted.')
                g.command('fsck repair-orphans','To apply, enter: fsck confirm')
                # Guest.command issues clear first, which correctly cancels a pending repair.
                g.type('fsck confirm\n');g.wait_for('Reclaimed 2 orphan clusters.',timeout=30)
                g.command('mount','Mounted FAT16')
                g.command('fsck','Read-only scan; no changes made.')
                assert 'fsck: clean' in g.screen(),g.screen()
            finally:g.close()
            assert image.read_bytes()==pristine,'Cleanup changed bytes outside lost FAT entries'

            # Corrupt the live APPS directory chain in only the second FAT.
            damaged=bytearray(pristine)
            apps=next(p for p in range(root*512,(root+32)*512,32) if damaged[p:p+11]==b'APPS       ')
            cluster=struct.unpack_from('<H',damaged,apps+26)[0]
            old=struct.unpack_from('<H',damaged,(fat+spf)*512+cluster*2)[0]
            struct.pack_into('<H',damaged,(fat+spf)*512+cluster*2,0 if old else 0xffff)
            image.write_bytes(damaged)
            g=Guest(qemu,None,directory,disk=image,attach_disk=not a.usb,extra_args=extra)
            try:
                if native:g.command('use 1','ready')
                g.command('unmount','Unmounted.')
                g.command('fsck repair-orphans','automatic repair refused')
                g.command('fsck confirm','fsck failed: run fsck repair-orphans')
            finally:g.close()
            assert image.read_bytes()==damaged,'Refused live-chain repair wrote the disk'
    print('PASS fsck: '+('BIOS USB' if a.usb else 'BIOS/native IDE')+', byte-identical read-only audit, canceled preview, orphan cleanup, cold persistence, live mirror refusal')


if __name__=='__main__':main()
