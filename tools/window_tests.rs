#![allow(dead_code)]
#[path="../src/mouse_packet.rs"]mod mouse_packet;
#[path="../src/desktop_ui.rs"]mod desktop_ui;
#[path="../src/window_protocol.rs"]mod window_protocol;
#[path="../src/app_window_ui.rs"]mod app_window_ui;
#[path="../src/input_order.rs"]mod input_order;
use window_protocol::{Draw,Event,CLOSE,POINTER,RESET};
use app_window_ui::State;
fn at(s:&mut State,x:i32,y:i32,buttons:u8)->(bool,Option<Event>) {s.update(mouse_packet::Event{dx:(x-s.x) as i16,dy:(y-s.y) as i16,buttons,lost:false})}
#[test] fn draw_bounds_reject_overflow_and_bad_formats() {
    assert!(Draw::rect(639,399,1,1,0xffffff).valid());
    for c in [Draw::rect(u32::MAX,0,2,1,0),Draw::rect(639,399,2,1,0),Draw::rect(0,0,1,1,0x1000000),Draw{kind:2,..Draw::default()},Draw::text(0,385,0,b"a"),Draw::text(633,0,0,b"a")] {assert!(!c.valid());}
    assert!(Draw::text(0,384,0,b"hello").valid());
}
#[test] fn close_is_a_cancellable_request_and_never_closes_the_surface() {
    let mut s=State::new(0);assert!(at(&mut s,710,103,1).1.is_none());
    assert_eq!(at(&mut s,710,103,0).1.unwrap().kind,CLOSE);assert!(s.window.visible);
    at(&mut s,710,103,1);assert!(at(&mut s,750,550,0).1.is_none());assert!(s.window.visible);
}
#[test] fn chrome_drag_does_not_leak_client_clicks_and_clamps_on_release() {
    let mut s=State::new(0);at(&mut s,100,103,1);assert!(at(&mut s,799,599,0).1.is_none());
    assert_eq!((s.window.rect.x,s.window.rect.y),(138,130));
}
#[test] fn client_events_are_local_and_release_capture_survives_outside() {
    let mut s=State::new(0);let e=at(&mut s,90,160,1).1.unwrap();assert_eq!((e.kind,e.x,e.y,e.buttons),(POINTER,10,38,1));
    let e=at(&mut s,0,0,0).1.unwrap();assert_eq!((e.x,e.y,e.buttons),(-80,-122,0));
    at(&mut s,0,0,1);assert!(!s.focused);at(&mut s,0,0,0);assert!(at(&mut s,400,300,0).1.is_none());
}
#[test] fn lost_input_cancels_close_and_drag_capture() {
    let mut s=State::new(0);at(&mut s,710,103,1);
    assert_eq!(s.update(mouse_packet::Event{lost:true,buttons:1,..mouse_packet::Event::default()}).1.unwrap().kind,RESET);
    assert!(at(&mut s,710,103,0).1.is_none());
}

#[test] fn mixed_input_sequence_orders_across_wrap() {
    assert!(input_order::before(u32::MAX-1,1));assert!(!input_order::before(1,u32::MAX-1));
    let a=input_order::next();let b=input_order::next();assert!(input_order::before(a,b));
}
