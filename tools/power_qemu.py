"""Verify ACPI power-off and halted fallback in private VMs."""
import argparse,shutil,tempfile,time
from pathlib import Path
from smoke import Guest,ROOT

def main():
 p=argparse.ArgumentParser();p.add_argument('--disk',type=Path,required=True);p.add_argument('--kernel',type=Path,required=True);a=p.parse_args()
 with tempfile.TemporaryDirectory(prefix='power-',dir=ROOT/'target/smoke') as tmp:
  d=Path(tmp);image=d/'system.img';shutil.copyfile(a.disk,image);qemu=r'C:\Program Files\qemu\qemu-system-i386.exe'
  g=Guest(qemu,None,d,disk=image)
  try:
   assert 'Power: static ACPI S5 available' in (d/'debug.log').read_text();g.command('touch power.txt','Created:');g.command('write power.txt persisted','Wrote');g.type('shutdown\n');g.process.wait(timeout=10);assert g.process.returncode==0
  finally:g.close()
  g=Guest(qemu,None,d,disk=image)
  try:
   g.command('cat power.txt','persisted');g.type('terminal\n');deadline=time.monotonic()+15
   while 'Multiwindow: open' not in (d/'debug.log').read_text():
    assert time.monotonic()<deadline;time.sleep(.05)
   time.sleep(.2);g.type('shutdown\n');time.sleep(.2);assert g.process.poll() is None,'Desktop shutdown skipped confirmation'
   g.type('shutdown confirm\n');g.process.wait(timeout=10);assert g.process.returncode==0
  finally:g.close()
  g=Guest(qemu,a.kernel.resolve(),d,attach_disk=False,extra_args=['-machine','q35'])
  try:g.type('shutdown\n');g.process.wait(timeout=10);assert g.process.returncode==0
  finally:g.close()
  g=Guest(qemu,None,d,disk=image,extra_args=['-machine','pc,acpi=off'])
  try:g.type('shutdown\n');g.wait_for('safe to turn off the computer.');assert g.process.poll() is None;assert 'Power: halted;' in (d/'debug.log').read_text()
  finally:g.close()
 print('PASS shutdown: ACPI pc/q35, persisted file after power cycle, no-ACPI halted fallback')
if __name__=='__main__':main()
