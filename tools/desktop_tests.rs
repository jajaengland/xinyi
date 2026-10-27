#[path="../src/mouse_packet.rs"] mod mouse_packet;
#[path="../src/desktop_ui.rs"] mod desktop_ui;
use desktop_ui::{State,Target};use mouse_packet::Event;
fn at(s:&mut State,x:i32,y:i32,buttons:u8) {s.update(Event{dx:(x-s.x) as i16,dy:(y-s.y) as i16,buttons,lost:false});}
fn click(s:&mut State,x:i32,y:i32) {at(s,x,y,1);at(s,x,y,0);}
#[test] fn opaque_windows_block_covered_controls_and_clicks_raise() {
    let mut s=State::new(0);at(&mut s,340,240,0);assert_eq!(s.hit(),Some(Target::Client(1)));click(&mut s,340,240);assert_eq!(s.clicks,[0,0]);
    click(&mut s,150,330);assert_eq!(s.order,[1,0]);assert_eq!(s.focus,Some(0));at(&mut s,340,240,0);assert_eq!(s.hit(),Some(Target::Button(1)));click(&mut s,340,240);assert_eq!(s.clicks,[0,1]);
}
#[test] fn drag_preserves_grab_offset_clamps_and_applies_release_motion() {
    let mut s=State::new(0);at(&mut s,140,155,1);assert_eq!(s.capture,Some(Target::Title(0)));
    at(&mut s,240,205,1);assert_eq!((s.windows[0].rect.x,s.windows[0].rect.y),(200,190));
    at(&mut s,260,225,0);assert_eq!((s.windows[0].rect.x,s.windows[0].rect.y),(220,210));assert_eq!(s.capture,None);
    at(&mut s,260,225,1);at(&mut s,799,599,1);assert_eq!((s.windows[0].rect.x,s.windows[0].rect.y),(434,364));
    at(&mut s,0,0,1);assert_eq!((s.windows[0].rect.x,s.windows[0].rect.y),(0,24));
}
#[test] fn close_requires_matching_release_and_focuses_next_visible_window() {
    let mut s=State::new(0);at(&mut s,622,225,1);at(&mut s,700,500,0);assert!(s.windows[1].visible);
    click(&mut s,622,225);assert!(!s.windows[1].visible);assert_eq!(s.focus,Some(0));
    click(&mut s,442,155);assert!(!s.windows[0].visible);assert_eq!(s.focus,None);
}
#[test] fn lost_input_cancels_drag_and_close_capture() {
    let mut s=State::new(0);at(&mut s,140,155,1);let rect=s.windows[0].rect;
    s.update(Event{lost:true,buttons:1,..Event::default()});at(&mut s,300,300,1);assert_eq!(s.windows[0].rect,rect);at(&mut s,300,300,0);
    at(&mut s,622,225,1);s.update(Event{lost:true,buttons:1,..Event::default()});at(&mut s,622,225,0);assert!(s.windows[1].visible);
}
#[test] fn menu_restores_windows_without_resetting_counters_and_blocks_click_through() {
    let mut s=State::new(0);s.clicks=[4,5];s.windows[0].visible=false;s.windows[1].visible=false;
    at(&mut s,799,599,2);assert_eq!(s.menu,Some((614,518)));at(&mut s,799,599,0);
    click(&mut s,630,530);assert!(s.windows.iter().all(|w|w.visible));assert_eq!(s.clicks,[4,5]);assert_eq!(s.menu,None);
    at(&mut s,600,450,2);at(&mut s,600,450,0);click(&mut s,150,330);assert_eq!(s.focus,Some(1));assert_eq!(s.order,[0,1]);assert_eq!(s.menu,None);
}
#[test] fn body_capture_cannot_turn_into_a_control_click() {
    let mut s=State::new(0);click(&mut s,150,330);at(&mut s,140,200,1);at(&mut s,150,240,0);assert_eq!(s.clicks,[0,0]);
    at(&mut s,150,240,1);at(&mut s,700,500,0);assert_eq!(s.clicks,[0,0]);
}

#[test] fn apps_menu_captures_selection_and_blocks_windows_beneath() {
    let mut s=State::new(0);s.app_count=5;click(&mut s,40,11);assert!(s.apps_open);
    let r=s.apps_menu();at(&mut s,r.x+15,r.y+4+2*24+12,0);assert_eq!(s.hit(),Some(Target::App(2)));
    let (x,y)=(s.x,s.y);click(&mut s,x,y);assert_eq!(s.launch,Some(2));assert!(!s.apps_open);
    let mut s=State::new(0);s.app_count=5;click(&mut s,40,11);let focus=s.focus;
    click(&mut s,150,330);assert!(!s.apps_open);assert_eq!(s.focus,focus);assert_eq!(s.clicks,[0,0]);
}
#[test] fn launcher_does_not_launch_on_drag_out_or_lost_release() {
    let mut s=State::new(0);s.app_count=5;click(&mut s,40,11);let r=s.apps_menu();
    at(&mut s,30,r.y+16,1);at(&mut s,400,100,0);assert!(s.launch.is_none());
    click(&mut s,40,11);let r=s.apps_menu();at(&mut s,30,r.y+16,1);
    s.update(Event{lost:true,buttons:1,..Event::default()});at(&mut s,30,r.y+16,0);assert!(s.launch.is_none());
}
