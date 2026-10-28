#![allow(dead_code)]
#[path="../src/pty_model.rs"]mod pty_model;
use pty_model::*;
#[test]fn expanded_view_includes_history_and_clear_preserves_a_blank_live_screen(){let mut c=Console::new();for _ in 0..60{c.write(b"line\n");}let mut out=[Cell{ch:0,color:0};2640];c.snapshot_rows(0,33,&mut out);assert_eq!(c.padding(33),8);assert_eq!(out[0].ch,b'l');c.clear();c.snapshot_rows(0,33,&mut out);assert!(out.iter().all(|v|v.ch==b' '));assert_eq!(c.padding(33),0);assert!(c.scroll_limit(33)>0);c.snapshot_rows(20,33,&mut out);assert!(out.iter().any(|v|v.ch==b'l'));}

#[test]fn maximized_viewport_exposes_the_entire_34th_row(){
 let mut c=Console::new();for _ in 0..60{c.write(b"line\n");}
 let mut out=[Cell{ch:0,color:0};2720];c.snapshot_rows(0,34,&mut out);
 assert_eq!(c.padding(34),9);assert!(&out[33*80..]==&c.cells[24*80..]);
 c.clear();c.snapshot_rows(0,34,&mut out);assert!(out.iter().all(|v|v.ch==b' '));
}
