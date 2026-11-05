#![allow(dead_code)]
#[path="../src/bmp.rs"]mod bmp;
#[path="../src/pixel_protocol.rs"]mod pixels;
fn sample(bits:u16,top:bool)->Vec<u8> {
    let stride=4;let mut b=vec![0;54+2*stride];let len=b.len() as u32;
    b[..2].copy_from_slice(b"BM");b[2..6].copy_from_slice(&len.to_le_bytes());b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());b[18..22].copy_from_slice(&1i32.to_le_bytes());b[22..26].copy_from_slice(&(if top {-2i32}else{2}).to_le_bytes());
    b[26]=1;b[28..30].copy_from_slice(&bits.to_le_bytes());
    let rows=if top {[[0,0,255,99],[255,0,0,88]]}else{[[255,0,0,88],[0,0,255,99]]};
    b[54..58].copy_from_slice(&rows[0]);b[58..62].copy_from_slice(&rows[1]);b
}
#[test]fn bmp_orientation_padding_and_unused_alpha_are_correct() {
    for bits in [24,32] {for top in [false,true] {
        let b=sample(bits,top);let f=bmp::parse(&b).unwrap();
        assert_eq!(f.pixel(&b,0,0),0xff0000);assert_eq!(f.pixel(&b,0,1),0x0000ff);
        assert_eq!(f.fit(640,400),(1,2));assert_eq!(f.fit(1,1),(1,1));
    }}
    let builtin=include_bytes!("../assets/images/sample.bmp");let f=bmp::parse(builtin).unwrap();
    assert_eq!((f.width,f.height),(320,192));assert_eq!(f.fit(160,400),(160,96));assert_eq!(f.fit(640,96),(160,96));
}
#[test]fn invalid_headers_extents_formats_and_arbitrary_input_fail_without_panics() {
    let b=sample(24,false);
    for n in 0..b.len() {assert!(bmp::parse(&b[..n]).is_none());}
    for (at,value) in [(2,0),(10,u32::MAX),(14,41),(18,0),(18,2049),(22,0x80000000),(22,2049),(30,1)] {
        let mut bad=b.clone();bad[at..at+4].copy_from_slice(&value.to_le_bytes());assert!(bmp::parse(&bad).is_none());
    }
    for bits in [0,1,4,8,16] {assert!(bmp::parse(&sample(bits,false)).is_none());}
    let mut seed=17u32;
    for _ in 0..5000 {
        let mut data=b.clone();seed=seed.wrapping_mul(1664525).wrapping_add(1013904223);let at=seed as usize%data.len();data[at]=(seed>>24) as u8;
        if let Some(f)=bmp::parse(&data) {let _=f.pixel(&data,f.width-1,f.height-1);}
    }
}
#[test]fn packed_tiles_require_complete_bounded_destination_rectangles() {
    let mut r=pixels::Blit{x:639,y:543,width:1,height:1,pixels:0};
    assert_eq!(r.count(640,544),Some(1));r.width=2;assert_eq!(r.count(640,544),None);
    r.x=u32::MAX;assert_eq!(r.count(640,544),None);
    r=pixels::Blit{x:0,y:0,width:512,height:4,pixels:0};assert_eq!(r.count(792,544),Some(2048));
    r.height=5;assert_eq!(r.count(792,544),None);r.width=u32::MAX;r.height=u32::MAX;assert_eq!(r.count(792,544),None);
    r.width=0;r.height=1;assert_eq!(r.count(792,544),None);
}
