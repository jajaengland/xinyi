#![allow(dead_code)]
#[path="../src/canvas.rs"]mod canvas;
use std::io::Write;
fn main() {
    let mut pixels=vec![0xaabbeeu32;320*160];
    let mut c=canvas::Canvas::new(&mut pixels,320,160,320).unwrap();
    c.label(8,8,100,0,b"Wide iii words and clipped labels");
    c.paragraph(8,40,100,33,0,b"one two three\nLongWordsWithoutSpacesAlsoWrapSafely");
    c.text(136,8,0,b"Mono 01",&canvas::text::FONT);
    c.label(318,96,2,0,b"clipped");
    c.paragraph(280,142,40,18,0,b"last line clip");
    for p in pixels {std::io::stdout().write_all(&p.to_le_bytes()).unwrap();}
}
