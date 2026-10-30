#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"] mod xinyi;
use xinyi::window::{self,Draw};
fn main()->u32 {
    let w=window::open_sized(b"Text ABI validation",320,160);assert!(w>0);let w=w as u32;
    assert_eq!(window::draw(w,&[Draw::rect(0,0,320,160,0xaabbee)]),1);
    let mut invalid=Draw::label(8,8,100,0,b"x");invalid.pointer=u32::MAX;
    assert_eq!(window::draw(w,&[Draw::rect(0,0,320,160,0),invalid]),-14);
    assert_eq!(window::draw(w,&[Draw::label(u32::MAX,0,100,0,b"x")]),-22);
    assert_eq!(window::draw(w,&[Draw::paragraph(0,159,100,2,0,b"x")]),-22);
    assert_eq!(window::draw(w,&[Draw::label(0,0,100,0,b"bad\nline")]),-22);
    assert_eq!(window::draw(w,&[Draw::paragraph(0,0,100,50,0,b"bad\x01byte")]),-22);
    assert_eq!(window::draw(w,&[Draw::label(0,0,100,0,&[b'x';97])]),-22);
    assert_eq!(window::draw(w,&[
        Draw::label(8,8,100,0,b"Wide iii words and clipped labels"),
        Draw::paragraph(8,40,100,33,0,b"one two three\nLongWordsWithoutSpacesAlsoWrapSafely"),
        Draw::text(136,8,0,b"Mono 01"),
        Draw::label(318,96,2,0,b"clipped"),
        Draw::paragraph(280,142,40,18,0,b"last line clip"),
    ]),5);
    assert_eq!(window::present(w),0);
    loop {
        let mut e=window::Event::default();assert!(window::event(w,&mut e,true)>=0);
        if e.kind==window::CLOSE || e.kind==window::KEY {return 0;}
    }
}
