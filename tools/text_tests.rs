#![allow(dead_code)]
#[path="../src/canvas.rs"] mod canvas;
#[path="../src/window_protocol.rs"] mod protocol;
use canvas::{Canvas,text::{self,Font}};
#[test]
fn measured_widths_fit_and_hit_testing_share_the_same_metrics() {
    assert!(text::width(b"iiii",Font::Ui)<text::width(b"WWWW",Font::Ui));
    assert_eq!(text::width(b"iiii",Font::Mono),32);
    for c in b'0'..=b'9' {assert_eq!(text::glyph(c,Font::Ui).advance,7);}
    assert_eq!(text::width(b"WW\ni",Font::Ui),text::width(b"WW",Font::Ui));
    for font in [Font::Mono,Font::Ui] {
        let sample=b"Illi WWW 123";
        for limit in 0..128 {
            let n=text::fit(sample,limit,font);
            assert!(text::width(&sample[..n],font)<=limit);
            if n<sample.len() {assert!(text::width(&sample[..n+1],font)>limit);}
            let (prefix,dots)=text::ellipsis(sample,limit,font);
            assert!(text::width(&sample[..prefix],font)+text::width(&b"..."[..dots],font)<=limit);
        }
        for i in 0..sample.len() {assert_eq!(text::hit_test(sample,text::width(&sample[..i],font),font),i);}
    }
}
#[test]
fn word_wrapping_preserves_words_explicit_lines_and_long_word_progress() {
    let lines:Vec<_>=text::Lines::new(b"one two three",7*8,Font::Mono).collect();
    assert_eq!((lines[0].start,lines[0].end,lines[0].width),(0,7,56));
    assert_eq!((lines[1].start,lines[1].end),(8,13));
    assert_eq!(text::height(b"A\n\nB\n",500,Font::Ui),64);
    assert_eq!(text::height(b"",500,Font::Ui),0);
    assert_eq!(text::Lines::new(b"abc",0,Font::Ui).count(),3);
    let mut seed=12u32;
    for _ in 0..500 {
        let mut bytes=[0;96];
        for b in &mut bytes {seed=seed.wrapping_mul(1664525).wrapping_add(1013904223);*b=match seed%8 {0=>b' ',1=>b'\n',_=>b'a'+(seed%26) as u8};}
        let width=seed%100;
        let mut recovered=Vec::new();let mut count=0;
        for line in text::Lines::new(&bytes,width,Font::Ui) {
            assert!(line.start<=line.end && line.end<=bytes.len());
            assert_eq!(text::width(&bytes[line.start..line.end],Font::Ui),line.width);
            assert!(line.width<=width || line.end-line.start==1);
            recovered.extend(bytes[line.start..line.end].iter().filter(|c|**c!=b' '&&**c!=b'\n').copied());
            count+=1;assert!(count<=97);
        }
        assert_eq!(recovered,bytes.into_iter().filter(|c|*c!=b' '&&*c!=b'\n').collect::<Vec<_>>());
    }
}
#[test]
fn clipped_labels_and_paragraphs_leave_adjacent_pixels_and_stride_padding_untouched() {
    for (x,y,width,height) in [(4,3,27,23),(-3,-2,22,19),(i32::MAX,0,20,16),(i32::MIN,0,u32::MAX,16)] {
        let mut pixels=[0xabcdef;40*40];
        {
            let mut c=Canvas::new(&mut pixels,35,40,40).unwrap();
            c.paragraph(x,y,width,height,0,b"Wide words\nllll words and more");
        }
        for row in 0..40 {for col in 0..40 {
            let inside=col<35 && col as i64>=x as i64 && (col as i64)<x as i64+width as i64 && row as i64>=y as i64 && (row as i64)<y as i64+height as i64;
            if !inside {assert_eq!(pixels[row*40+col],0xabcdef);}
        }}
    }
    let mut pixels=[0xffffffff;40*20];
    Canvas::new(&mut pixels,40,20,40).unwrap().label(4,2,12,0,b"A very long name");
    assert!(pixels.iter().any(|p|*p==0));
    for row in 0..20 {for col in 0..40 {if !(4..16).contains(&col) {assert_eq!(pixels[row*40+col],0xffffffff);}}}
}
#[test]
fn optimized_legacy_glyphs_match_the_reference_for_all_bytes_and_clipped_edges() {
    for byte in 0..=255u8 {for (x,y) in [(-3,-4),(0,0),(17,9),(i32::MAX,0),(i32::MIN,0)] {
        let(mut actual,mut expected)=([0u32;23*24],[0u32;23*24]);
        Canvas::new(&mut actual,20,24,23).unwrap().glyph(x,y,99,byte,&text::FONT);
        for row in 0..16 {for col in 0..8 {
            let(px,py)=(x.saturating_add(col),y.saturating_add(row));
            if (0..20).contains(&px)&&(0..24).contains(&py)&&text::FONT[byte as usize*16+row as usize]&(0x80>>col)!=0 {expected[py as usize*23+px as usize]=99;}
        }}
        assert_eq!(actual,expected,"{byte} at {x},{y}");
    }}
}
#[test]
fn draw_boxes_reject_overflow_and_preserve_the_legacy_abi() {
    assert!(protocol::Draw::label(0,0,20,0,b"long labels can clip").valid());
    assert!(protocol::Draw::paragraph(0,0,20,32,0,b"new\nline").valid());
    assert!(!protocol::Draw::label(u32::MAX,0,20,0,b"x").valid());
    assert!(!protocol::Draw::paragraph(0,u32::MAX,20,32,0,b"x").valid());
    assert!(!protocol::Draw::label(0,0,20,0,&[b'x';97]).valid());
    assert!(!protocol::Draw::text(639,0,0,b"x").valid());
    assert_eq!(core::mem::size_of::<protocol::Draw>(),32);
}
