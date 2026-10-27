// VESA BIOS Extensions 2/3 mode information, queried through the BIOS bridge.
// https://www.phatcode.net/res/221/files/vbe20.pdf
use crate::bios::{self,Registers};
use crate::loader::Frames;
pub use crate::vbe_mode::Mode;
use crate::vbe_mode::parse_mode;
static mut CACHED:Option<Mode>=None;
fn video(ax:u32,bx:u32,cx:u32,dx:u32,es:u16,di:u32)->Option<Registers> {
    bios::invoke(Registers{eax:ax,ebx:bx,ecx:cx,edx:dx,es,edi:di,interrupt:0x10,..Registers::default()})
}
fn word(data:&[u8],at:usize)->u16 {u16::from_le_bytes([data[at],data[at+1]])}
fn dword(data:&[u8],at:usize)->u32 {u32::from_le_bytes([data[at],data[at+1],data[at+2],data[at+3]])}
pub fn find_mode()->Result<Mode,&'static str> {
    if let Some(mode)=unsafe {*core::ptr::addr_of!(CACHED)} {return Ok(mode);}
    if !bios::available() {return Err("Graphics backend unavailable: boot the BIOS disk/USB image for VESA support.");}
    unsafe {core::ptr::write_bytes(0x18000 as *mut u8,0,768);core::ptr::copy_nonoverlapping(b"VBE2".as_ptr(),0x18000 as *mut u8,4);}
    let result=video(0x4f00,0,0,0,0x1800,0).ok_or("BIOS video bridge unavailable.")?;
    let info=unsafe {core::slice::from_raw_parts(0x18000 as *const u8,512)};
    if result.eax&0xffff!=0x004f||&info[..4]!=b"VESA" {return Err("VESA controller query failed.");}
    let version=word(info,4);if version<0x0200 {return Err("VESA 2.0 linear framebuffer support is required.");}
    let far=dword(info,14);let address=(far>>16)*16+(far&0xffff);
    if !(0x500..=0xffdfe).contains(&address) {return Err("VESA mode-list pointer is invalid.");}
    let mut modes=[0xffffu16;256];let mut length=0;
    for i in 0..256 {let mode=unsafe {core::ptr::read_unaligned((address as *const u16).add(i))};
        if mode==0xffff {break;}modes[length]=mode;length+=1;}
    let mut best:Option<Mode>=None;
    for &number in &modes[..length] {
        unsafe {core::ptr::write_bytes(0x18200 as *mut u8,0,256);}
        if video(0x4f01,0,number as u32,0,0x1800,512).is_some_and(|r|r.eax&0xffff==0x4f) {
            let data=unsafe {&*(0x18200 as *const [u8;256])};
            if let Some(mode)=parse_mode(number,version,data) {
                if best.is_none_or(|old|mode.bpp>old.bpp) {best=Some(mode);}
            }
        }
    }
    let mode=best.ok_or("BIOS has no supported 800x600 linear RGB mode (16/24/32 bpp).")?;
    unsafe {*core::ptr::addr_of_mut!(CACHED)=Some(mode);}
    Ok(mode)
}
pub fn font()->Result<Frames,&'static str> {
    let result=video(0x1130,0x0600,0,0,0,0).ok_or("BIOS font query failed.")?;
    let address=(result.es as u32)*16+(result.ebp&0xffff);
    if result.ecx&0xffff!=16||address<0x500||address.checked_add(4096).is_none_or(|end|end>0x100000) {return Err("BIOS 8x16 font pointer is invalid.");}
    let frames=Frames::new(1).ok_or("Not enough memory for the font.")?;
    unsafe {core::ptr::copy_nonoverlapping(address as *const u8,frames.base() as *mut u8,4096);}
    Ok(frames)
}
pub fn set_mode(mode:Mode)->bool {video(0x4f02,mode.number as u32|0x4000,0,0,0,0).is_some_and(|r|r.eax&0xffff==0x4f)}
pub fn text_mode()->bool {video(3,0,0,0,0,0).is_some()}
