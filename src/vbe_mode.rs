// Checked subset of VBE 2/3 linear framebuffer mode information.
#[derive(Clone,Copy)]
pub struct Mode {pub number:u16,pub base:u32,pub pitch:u32,pub bpp:u8}
fn word(data:&[u8],at:usize)->u16 {u16::from_le_bytes([data[at],data[at+1]])}
fn dword(data:&[u8],at:usize)->u32 {u32::from_le_bytes([data[at],data[at+1],data[at+2],data[at+3]])}
pub fn parse_mode(number:u16,version:u16,data:&[u8;256])->Option<Mode> {
    if word(data,0)&0x91!=0x91||word(data,18)!=800||word(data,20)!=600||data[24]!=1||data[27]!=6 {return None;}
    let bpp=data[25];if !matches!(bpp,16|24|32) {return None;}
    let linear=version>=0x0300&&word(data,50)!=0;
    let pitch=word(data,if linear {50}else{16}) as u32;
    let masks=if linear {54}else{31};
    let expected=if bpp==16 {[5,11,6,5,5,0]}else{[8,16,8,8,8,0]};
    if data[masks..masks+6]!=expected {return None;}
    let base=dword(data,40);let bytes=bpp as u32/8;
    let alignment=if bpp==32 {4}else if bpp==16 {2}else{1};
    if base<0x80000000||base%alignment!=0||pitch<800*bytes||pitch>8192||pitch%alignment!=0||base.checked_add(pitch*600).is_none() {return None;}
    Some(Mode{number,base,pitch,bpp})
}
