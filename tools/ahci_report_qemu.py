"""Verify the extended hardware report using two disposable Q35 SATA disks."""
import argparse
import re
import shutil
import tempfile
import time
from pathlib import Path

from smoke import Guest, ROOT
from seed_apps import inspect_tree
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disk", type=Path, required=True)
    source = parser.parse_args().disk
    output = ROOT / "target/smoke"
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=output, prefix='hardware-report-') as tmp:
        d = Path(tmp)
        image = d/'boot.img'
        shutil.copyfile(source,image)
        data_disk = d/'sata-data.img'
        data_disk.write_bytes(b'AHCI probe sentinel' + bytes(4*1024*1024-19))
        with data_disk.open('r+b') as fixture:
            fixture.seek(-2048, 2)
            fixture.write(bytes(range(256)) * 2 + b'A' * 512 + b'B' * 512 + b'C' * 512)
        before = data_disk.read_bytes()
        args = ['-machine','q35','-audiodev','none,id=hda','-device','intel-hda','-device','hda-output,audiodev=hda',
                '-drive',f'if=ide,index=1,file={data_disk.as_posix()},format=raw']
        guest = Guest(r'C:\Program Files\qemu\qemu-system-i386.exe',None,d,disk=image,attach_disk=True,extra_args=args,start_desktop=True)
        log = d/'debug.log'
        def until(check, label):
            deadline = time.monotonic()+20
            while time.monotonic() < deadline:
                try:
                    result = check()
                    if result:
                        return result
                except AssertionError:
                    pass
                time.sleep(.05)
            raise AssertionError(label+'\n'+log.read_text()[-2600:])
        try:
            guest.key('ctrl-esc');guest.type('hardware\n')
            match = until(lambda:re.search(r'Multiwindow: open pid=(\d+) .*client=600x400 pos=(\d+),(\d+)',log.read_text()),'Hardware window')
            pid, wx, wy = map(int, match.groups())
            time.sleep(.2)
            pos = [400,300]
            dest = [wx+8+502,wy+34+18]
            while pos != dest:
                delta = [max(-60,min(60,b-a)) for a,b in zip(pos,dest)]
                guest.call('input-send-event',{'events':[{'type':'rel','data':{'axis':axis,'value':n}} for axis,n in zip(('x','y'),delta)]})
                pos = [a+b for a,b in zip(pos,delta)]
                time.sleep(.04)
            for down in (True,False):
                guest.call('input-send-event',{'events':[{'type':'btn','data':{'button':'left','down':down}}]});time.sleep(.08)
            until(lambda:'Saved /HARDWARE.TXT on the mounted drive.' in log.read_text(),'Save Report button')
            first = until(lambda:inspect_tree(image,2048)[0].get('HARDWARE.TXT'),'Report written')
            for expected in [b'CPU:', b'Firmware-reported usable RAM:', b'Boot: Legacy BIOS', b'Storage: BIOS boot drive mounted', b'HD Audio codec 0: 1af4:0012', b'8086:2668', b'[active audio]']:
                assert expected in first, (expected, first)
            assert b'AHCI port status' in first, first
            port_map = re.search(rb'^00:1f\.2 ([.SdAx?\-]{32})$', first, re.M)
            assert port_map and port_map[1][:2] == b'SS', first
            assert b'.' in port_map[1], first
            assert b'Desktop timing (' in first and b'Composition: mean ' in first and b'Framebuffer and cursor: mean ' in first, first
            assert b'port 0: QEMU HARDDISK' in first and b'port 1: QEMU HARDDISK' in first, first
            assert first.count(b'LBA 0 read OK;') == 2 and b'55AA signature present' in first and b'55AA signature absent' in first, first
            assert first.count(b'Final four sectors: bulk and individual reads match') == 2, first
            guest.dismiss_dialogs(pid,wait=True)
            guest.key('ctrl-s')
            until(lambda:'Saved /HARDWR01.TXT on the mounted drive.' in log.read_text(),'Second report')
            files = inspect_tree(image,2048)[0]
            assert files['HARDWARE.TXT'] == first and b'SATA diagnostic' not in files['HARDWR01.TXT']
            (output/'ahci-report.txt').write_bytes(first)
        finally:
            guest.close()
        assert data_disk.read_bytes() == before, 'Secondary SATA disk changed'
    print('PASS extended report: two SATA IDENTIFY results, two LBA 0 reads, final four-sector comparisons, graphics timing, passive export, secondary disk byte-identical')


if __name__ == "__main__":
    main()
