#[path="../src/mouse_packet.rs"]mod mouse_packet;
#[path="../src/desktop_ui.rs"]mod desktop_ui;
#[path="../src/window_protocol.rs"]mod window_protocol;
#[path="../src/canvas.rs"]mod canvas;
#[path="../src/message_dialog.rs"]mod message_dialog;
use message_dialog::Notice;use window_protocol::*;
#[test]fn bounded_requests_and_priority(){let mut n=Notice::new();assert!(!n.set(3,b"bad"));assert!(!n.set(0,b""));assert!(!n.set(1,&[b'x';193]));assert!(!n.set(2,b"bad\n"));assert!(n.set(2,b"failed"));assert!(n.set(0,b"success"));assert_eq!(&n.bytes[..n.len],b"failed");}
#[test]fn modal_input_and_capture_never_click_through(){let mut n=Notice::new();n.set(1,b"warning");assert!(n.event(Event{kind:SCROLL,code:1,..Event::default()},640,400).is_none());let b=Notice::button(640,400);n.event(Event{kind:POINTER,x:b.x+5,y:b.y+5,buttons:1,..Event::default()},640,400);assert!(n.event(Event{kind:POINTER,x:0,y:0,..Event::default()},640,400).is_none());assert!(n.active);n.event(Event{kind:POINTER,x:b.x+5,y:b.y+5,buttons:1,..Event::default()},640,400);let e=n.event(Event{kind:POINTER,x:b.x+5,y:b.y+5,..Event::default()},640,400).unwrap();assert_eq!(e.kind,RESET);assert!(!n.active);}
#[test]fn long_messages_page_and_escape_dismisses(){let mut n=Notice::new();n.set(0,&[b'A';192]);assert!(n.event(Event{kind:KEY,code:10,..Event::default()},320,144).is_none());assert!(n.offset>0&&n.offset<192);assert!(n.active);assert_eq!(n.event(Event{kind:KEY,code:27,..Event::default()},320,144).unwrap().kind,RESET);assert!(!n.active);}
#[test]fn rendering_clips_at_small_client_boundaries(){let mut n=Notice::new();n.set(2,b"An error with details");let mut pixels=[0;320*144];n.paint(&mut canvas::Canvas::new(&mut pixels,320,144,320).unwrap(),desktop_ui::Rect{x:0,y:0,w:320,h:144});assert!(pixels.iter().any(|p|*p==0x923d46));}

#[test]fn smallest_window_keeps_the_dialog_and_controls_inside_client(){let r=Notice::rect(160,80);let b=Notice::button(160,80);let body=Notice::body(160,80);assert!(r.x>=0&&r.y>=0&&r.x+r.w<=160&&r.y+r.h<=80);assert!(b.y>=body.y+body.h);let mut n=Notice::new();n.set(0,&[b'W';192]);let mut pages=0;while n.active{n.event(Event{kind:KEY,code:10,..Event::default()},160,80);pages+=1;assert!(pages<=192);}assert!(pages>1);}

#[test]fn sounds_only_announce_new_visible_messages(){let mut n=Notice::new();assert!(n.announces(2,b"error"));n.set(2,b"error");assert!(!n.announces(2,b"error"));assert!(!n.announces(0,b"information"));assert!(n.announces(2,b"another error"));n.event(Event{kind:KEY,code:27,..Event::default()},640,400);assert!(n.announces(2,b"error"));}
