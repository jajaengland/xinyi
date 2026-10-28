// Serialized legacy firmware calls; available only with our BIOS-loader handoff.
use core::{arch::asm,sync::atomic::{AtomicBool,AtomicU32,Ordering}};
const CONTEXT:*mut Registers=0x6000 as *mut Registers;
const THUNK:&[u8]=include_bytes!(concat!(env!("OUT_DIR"),"/bios_thunk.bin"));
static AVAILABLE:AtomicBool=AtomicBool::new(false);
static BUSY:AtomicBool=AtomicBool::new(false);
static BIOS_MASKS:AtomicU32=AtomicU32::new(0xffff);
static CALLS:AtomicU32=AtomicU32::new(0);
#[derive(Clone,Copy,Default)]
#[repr(C)]
pub struct Registers {pub eax:u32,pub ebx:u32,pub ecx:u32,pub edx:u32,pub esi:u32,pub edi:u32,pub ebp:u32,pub flags:u32,pub ds:u16,pub es:u16,pub interrupt:u16,pub reserved:u16}
const _:()=assert!(core::mem::size_of::<Registers>()==40);
pub unsafe fn input(port:u16)->u8 {let value:u8;asm!("in al, dx",in("dx")port,out("al")value,options(nomem,nostack,preserves_flags));value}
pub unsafe fn output(port:u16,value:u8) {asm!("out dx, al",in("dx")port,in("al")value,options(nomem,nostack,preserves_flags));}
unsafe fn pic(master:u8,slave:u8,masks:u16) {
    for (port,value) in [(0x20,0x11),(0xa0,0x11),(0x21,master),(0xa1,slave),(0x21,4),(0xa1,2),(0x21,1),(0xa1,1),(0x21,masks as u8),(0xa1,(masks>>8) as u8)] {
        output(port,value);output(0x80,0);
    }
}
pub fn init() {
    if crate::multiboot::bios_drive().is_none() {return;}
    unsafe {
        BIOS_MASKS.store(input(0x21) as u32|((input(0xa1) as u32)<<8),Ordering::Relaxed);
        core::ptr::copy_nonoverlapping(THUNK.as_ptr(),0x7000 as *mut u8,THUNK.len());
    }
    AVAILABLE.store(true,Ordering::Release);
}
pub fn available()->bool {AVAILABLE.load(Ordering::Acquire)}
pub fn invoke(mut registers:Registers)->Option<Registers> {
    if !available()||!matches!(registers.interrupt,0x10|0x13)||BUSY.swap(true,Ordering::AcqRel) {return None;}
    let count=CALLS.fetch_add(1,Ordering::Relaxed);
    if count<12 {crate::debug::write_fmt(format_args!("BIOS call {:02x} ax={:04x}\n",registers.interrupt,registers.eax));}
    let flags:u32;
    unsafe {
        asm!("pushfd","pop {0}","cli",out(reg)flags);
        let masks=input(0x21) as u16|((input(0xa1) as u16)<<8);
        output(0x70,0x80);
        // Firmware IRQ vectors and its PIT rate. Mask keyboard and mouse IRQs
        // so BIOS input handlers cannot steal pending kernel input.
        pic(8,0x70,(BIOS_MASKS.load(Ordering::Relaxed) as u16|0x1002)&!1);
        output(0x43,0x36);output(0x40,0);output(0x40,0);
        CONTEXT.write(registers);
        let entry:extern "C" fn()=core::mem::transmute(0x7000usize);
        let _ = crate::monotonic::ticks();
        entry();
        let _ = crate::monotonic::ticks();
        registers=CONTEXT.read();
        pic(0x20,0x28,masks);
        crate::interrupts::init_pit();
        // PIC initialization can discard an already-pending keyboard edge.
        // Drain its hardware FIFO before enabling interrupts again.
        crate::keyboard::handle_irq();
        output(0x70,0);
        BUSY.store(false,Ordering::Release);
        if flags&0x200!=0 {asm!("sti",options(nostack));}
    }
    if count<12 {crate::debug::write_fmt(format_args!("BIOS returned ax={:04x} flags={:04x}\n",registers.eax,registers.flags));}
    Some(registers)
}
