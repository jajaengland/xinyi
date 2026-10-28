#[path="../src/ui_font.rs"]mod ui_font;
#[test]fn ascii_has_blank_margins_and_complete_unique_glyphs(){
 let font=&ui_font::FONT;
 assert!(font[..32*16].iter().all(|&v|v==0));assert!(font[127*16..].iter().all(|&v|v==0));
 for c in 32..127 {let glyph=&font[c*16..c*16+16];assert_eq!(glyph[0],0);assert_eq!(glyph[1],0);assert_eq!(glyph[15],0);assert!(glyph.iter().all(|v|v&0x81==0));
 if c==32 {assert!(glyph.iter().all(|&v|v==0));}else{assert!(glyph.iter().any(|&v|v!=0));}
 for other in 32..c {assert_ne!(glyph,&font[other*16..other*16+16],"duplicate characters {c}/{other}");}
 }
}
#[test]fn baseline_descenders_and_ambiguous_characters_are_distinct(){
 let font=&ui_font::FONT;
 for c in b'A'..=b'Z' {assert!(font[c as usize*16+13..c as usize*16+16].iter().all(|&v|v==0));}
 for c in b"gjpqy" {assert!(font[*c as usize*16+13..*c as usize*16+15].iter().any(|&v|v!=0));}
 for (a,b) in [(b'0',b'O'),(b'1',b'l'),(b'I',b'l'),(b'5',b'S')] {assert_ne!(&font[a as usize*16..a as usize*16+16],&font[b as usize*16..b as usize*16+16]);}
}
