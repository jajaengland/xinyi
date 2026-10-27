#[path="../src/vbe_mode.rs"] mod vbe_mode;
use vbe_mode::parse_mode;
fn info(bpp:u8)->[u8;256] {
    let mut d=[0;256];
    d[0..2].copy_from_slice(&0x91u16.to_le_bytes());
    d[18..20].copy_from_slice(&800u16.to_le_bytes());
    d[20..22].copy_from_slice(&600u16.to_le_bytes());
    d[24]=1;d[25]=bpp;d[27]=6;
    d[16..18].copy_from_slice(&(800u16*(bpp as u16/8)+16).to_le_bytes());
    d[31..37].copy_from_slice(if bpp==16 {&[5,11,6,5,5,0]}else{&[8,16,8,8,8,0]});
    d[40..44].copy_from_slice(&0xe0000000u32.to_le_bytes());d
}
#[test] fn direct_rgb_formats_and_padded_stride() {
    for bpp in [16,24,32] {let d=info(bpp);let m=parse_mode(0x115,0x200,&d).unwrap();
        assert_eq!((m.number,m.base,m.pitch,m.bpp),(0x115,0xe0000000,800*bpp as u32/8+16,bpp));}
}
#[test] fn rejects_unsupported_layouts() {
    for (offset,value) in [(0,0x11),(18,0),(20,0),(24,2),(25,15),(27,4),(31,7),(32,0)] {
        let mut d=info(32);d[offset]=value;assert!(parse_mode(0x115,0x200,&d).is_none(),"offset {offset}");}
}
#[test] fn rejects_short_or_unaligned_pitch_and_bad_address() {
    for pitch in [0u16,3199,3201,8196] {let mut d=info(32);d[16..18].copy_from_slice(&pitch.to_le_bytes());assert!(parse_mode(1,0x200,&d).is_none());}
    for address in [0u32,0x40000000,0xe0000001,0xfffff000] {let mut d=info(32);d[40..44].copy_from_slice(&address.to_le_bytes());assert!(parse_mode(1,0x200,&d).is_none());}
}
#[test] fn vbe3_uses_linear_fields_and_rejects_wrong_masks() {
    let mut d=info(32);d[16..18].copy_from_slice(&1u16.to_le_bytes());
    d[50..52].copy_from_slice(&3328u16.to_le_bytes());d[54..60].copy_from_slice(&[8,16,8,8,8,0]);
    assert_eq!(parse_mode(1,0x300,&d).unwrap().pitch,3328);
    assert!(parse_mode(1,0x200,&d).is_none());d[54]=5;assert!(parse_mode(1,0x300,&d).is_none());
}
