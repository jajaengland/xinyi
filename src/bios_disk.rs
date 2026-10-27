use core::sync::atomic::{AtomicU32,Ordering};
use crate::{bios::{self,Registers},blockdev::BlockDevice};
pub struct BiosDisk {sectors:AtomicU32,error:AtomicU32}
pub static BOOT:BiosDisk=BiosDisk{sectors:AtomicU32::new(0),error:AtomicU32::new(0)};
impl BiosDisk {
    pub fn init(&self)->bool {
        if self.sectors.load(Ordering::Relaxed)!=0 {return true;}
        let Some(drive)=crate::multiboot::bios_drive() else{return false;};
        let Some(check)=bios::invoke(Registers{eax:0x4100,ebx:0x55aa,edx:drive as u32,interrupt:0x13,..Registers::default()}) else{return false;};
        if check.flags&1!=0||check.ebx&0xffff!=0xaa55||check.ecx&1==0 {return false;}
        unsafe {core::ptr::write_bytes(0x18000 as *mut u8,0,0x42);core::ptr::write_unaligned(0x18000 as *mut u16,0x42);}
        let Some(result)=bios::invoke(Registers{eax:0x4800,edx:drive as u32,ds:0x1800,interrupt:0x13,..Registers::default()}) else{return false;};
        if result.flags&1!=0 {self.error.store(result.eax>>8&255,Ordering::Relaxed);return false;}
        let (sectors,bytes)=unsafe {(core::ptr::read_unaligned(0x18010 as *const u64),core::ptr::read_unaligned(0x18018 as *const u16))};
        if bytes!=512||sectors==0||sectors>u32::MAX as u64 {return false;}
        self.sectors.store(sectors as u32,Ordering::Relaxed);true
    }
    fn transfer(&self,block:u32,write:bool)->bool {
        if block>=self.block_count() {return false;}
        let Some(drive)=crate::multiboot::bios_drive() else{return false;};
        let packet=[0x10u32|(1<<16),0x10000000,block,0];
        unsafe {core::ptr::copy_nonoverlapping(packet.as_ptr(),0x6100 as *mut u32,4);}
        let Some(result)=bios::invoke(Registers{eax:if write {0x4300}else{0x4200},edx:drive as u32,esi:0x6100,interrupt:0x13,..Registers::default()}) else{return false;};
        self.error.store(if result.flags&1!=0 {result.eax>>8&255}else{0},Ordering::Relaxed);
        result.flags&1==0
    }
    pub fn status(&self)->u32 {self.error.load(Ordering::Relaxed)}
}
impl BlockDevice for BiosDisk {
    fn read_block(&self,block:u32,buffer:&mut [u8])->bool {
        if buffer.len()<512||!self.transfer(block,false) {return false;}
        unsafe {core::ptr::copy_nonoverlapping(0x10000 as *const u8,buffer.as_mut_ptr(),512);}true
    }
    fn write_block(&self,block:u32,buffer:&[u8])->bool {
        if buffer.len()<512 {return false;}
        unsafe {core::ptr::copy_nonoverlapping(buffer.as_ptr(),0x10000 as *mut u8,512);}
        if !self.transfer(block,true)||!self.transfer(block,false) {return false;}
        let readback=unsafe {core::slice::from_raw_parts(0x10000 as *const u8,512)};
        readback==&buffer[..512]
    }
    fn block_size(&self)->usize {512}
    fn block_count(&self)->u32 {self.sectors.load(Ordering::Relaxed)}
    // Firmware writes complete synchronously and are read back. INT 13h has no
    // portable cache-flush guarantee; this is not a power-loss durability claim.
    fn flush(&self)->bool {self.status()==0}
}
