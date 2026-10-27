#[path="../src/mouse_packet.rs"] mod mouse_packet;
#[path="../src/mouse_ui.rs"] mod mouse_ui;
use mouse_packet::{Decoder,Event};use mouse_ui::{State,Target};
fn packet(d:&mut Decoder,bytes:[u8;3])->Event {assert!(d.feed(bytes[0]).is_none());assert!(d.feed(bytes[1]).is_none());d.feed(bytes[2]).unwrap()}
fn event(dx:i16,dy:i16,buttons:u8)->Event {Event{dx,dy,buttons,lost:false}}
#[test] fn signed_nine_bit_motion_and_inverted_screen_y() {
    let mut d=Decoder::new();assert_eq!(packet(&mut d,[8,200,255]),event(200,-255,0));
    assert_eq!(packet(&mut d,[0x3b,56,1]),event(-200,255,3));
}
#[test] fn sync_overflow_and_partial_reset() {
    let mut d=Decoder::new();assert!(d.feed(0).is_none());assert!(d.feed(0x07).is_none());
    assert_eq!(packet(&mut d,[0xc9,255,255]),event(0,0,1));
    d.feed(8);d.reset();assert_eq!(packet(&mut d,[0x28,0,255]),event(0,1,0));
}
#[test] fn button_capture_only_activates_on_release_inside() {
    let mut s=State::new(0);s.update(event(-100,-40,1));assert_eq!(s.capture,Some(Target::Button(0)));assert_eq!(s.clicks,[0,0]);
    s.update(event(0,0,1));s.update(event(0,0,0));assert_eq!(s.clicks,[1,0]);
    s.update(event(0,0,1));s.update(event(200,100,0));assert_eq!(s.clicks,[1,0]);
    s.update(event(0,0,1));s.update(event(-200,-100,0));assert_eq!(s.clicks,[1,0]);
}
#[test] fn menu_dismisses_and_reset_works_without_click_through() {
    let mut s=State::new(0);s.clicks=[2,3];s.update(event(0,0,2));assert_eq!(s.menu,Some((400,300)));
    s.update(event(0,0,0));s.update(event(20,15,1));s.update(event(0,0,0));assert_eq!(s.clicks,[0,0]);assert_eq!(s.menu,None);
    s.update(event(0,0,2));s.update(event(0,0,0));s.update(event(-120,-55,1));s.update(event(0,0,0));assert_eq!(s.clicks,[0,0]);assert_eq!(s.menu,None);
}
#[test] fn bounds_menu_placement_and_lost_events_cancel_capture() {
    let mut s=State::new(0);s.update(event(32000,32000,2));assert_eq!((s.x,s.y),(799,599));assert_eq!(s.menu,Some((620,524)));
    s.update(event(-32000,-32000,0));assert_eq!((s.x,s.y),(0,0));
    s.menu=None;s.update(event(300,260,1));s.update(Event{lost:true,buttons:1,..Event::default()});s.update(event(0,0,0));assert_eq!(s.clicks,[0,0]);
}
