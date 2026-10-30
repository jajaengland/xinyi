#![allow(dead_code)]
#[path="../src/mouse_packet.rs"]mod mouse_packet;
#[path="../src/desktop_ui.rs"]mod desktop_ui;
#[path="../src/app_catalog.rs"]mod app_catalog;
#[path="../src/desktop_shell.rs"]mod desktop_shell;
use desktop_shell::{Shell,Task,Action};
fn click(s:&mut Shell,x:i32,y:i32)->Option<Action> {s.input(x,y,1,false);s.input(x,y,0,false).1}
#[test] fn menu_consumes_outside_click_and_opens_above_taskbar() {
 let mut s=Shell::new();s.catalog.add(b"COUNTER BIN",false);click(&mut s,40,576);assert!(s.open);assert_eq!(s.menu().y,80);
 let(consumed,action,_)=s.input(700,200,1,false);assert!(consumed);assert!(action.is_none());assert!(!s.open);
 assert!(s.input(700,200,0,false).0);click(&mut s,40,576);assert_eq!(click(&mut s,40,188),Some(Action::Launch(0)));
}
#[test] fn task_buttons_select_pid_and_release_capture_is_cancellable() {
 let mut s=Shell::new();s.tasks[2]=Some(Task{pid:7,name:*b"NOTEPAD ",length:7});
 assert_eq!(click(&mut s,400,576),Some(Action::Focus(7)));
 s.input(400,576,1,false);assert!(s.input(700,200,0,false).1.is_none());
 s.input(400,576,1,false);s.input(400,576,1,true);assert!(s.input(400,576,0,false).1.is_none());
 assert_eq!(click(&mut s,768,576),Some(Action::Exit));
}
#[test] fn empty_and_full_catalogs_fit_below_taskbar() {
 let mut s=Shell::new();click(&mut s,40,576);assert!(click(&mut s,40,188).is_none());
 for _ in 0..16 {s.catalog.add(b"COUNTER BIN",false);}let r=s.menu();assert!(r.y+r.h+6<=600);
}
fn task(pid:u32)->Task {Task{pid,name:*b"APP     ",length:3}}
#[test] fn keyboard_cycle_wraps_both_ways_and_skips_removed_slots() {
 let mut s=Shell::new();assert_eq!(s.next_task(false),None);assert_eq!(s.next_task(true),None);
 s.tasks=[Some(task(1)),None,Some(task(3)),Some(task(4))];
 assert_eq!(s.next_task(false),Some(1));assert_eq!(s.next_task(true),Some(4));
 s.focused=Some(4);assert_eq!(s.next_task(false),Some(1));assert_eq!(s.next_task(true),Some(3));
 s.focused=Some(1);assert_eq!(s.next_task(false),Some(3));assert_eq!(s.next_task(true),Some(4));
 s.tasks=[None,None,Some(task(3)),None];s.focused=Some(3);
 assert_eq!(s.next_task(false),Some(3));assert_eq!(s.next_task(true),Some(3));
}
#[test] fn apps_keyboard_navigation_launches_and_escape_does_not_reach_apps() {
 let mut s=Shell::new();for name in [b"COUNTER BIN",b"FILES   BIN",b"NOTEPAD BIN"] {s.catalog.add(name,false);}
 assert_eq!(s.key(27|(1<<17)),(true,None));assert!(s.open);assert_eq!(s.menu_selection,Some(0));
 s.key(0x100);assert_eq!(s.menu_selection,Some(2));s.key(0x101);assert_eq!(s.menu_selection,Some(0));
 s.key(0x105);assert_eq!(s.menu_selection,Some(2));s.key(0x104);s.key(0x101);
 assert_eq!(s.key(10),(true,Some(Action::Launch(1))));assert!(!s.open);
 assert_eq!(s.key(b'x' as u32),(false,None));s.key(27|(1<<17));
 assert_eq!(s.key(27),(true,None));assert!(!s.open);
 s.key(27|(1<<17));s.input(100,100,0,false);assert_eq!(s.menu_selection,None);
}
#[test] fn empty_apps_menu_accepts_navigation_without_launching() {
 let mut s=Shell::new();s.key(27|(1<<17));s.key(0x100);s.key(0x101);s.key(0x105);
 assert_eq!(s.key(10),(true,None));assert!(!s.open);
}
#[test] fn search_is_bounded_case_insensitive_and_never_launches_stale_results() {
 let mut s=Shell::new();for name in [b"COUNTER BIN",b"FILES   BIN",b"NOTEPAD BIN"] {s.catalog.add(name,false);}
 s.key(27|(1<<17));for b in b"note" {s.key(*b as u32);}
 assert_eq!(s.filtered().1,1);assert_eq!(s.key(10),(true,Some(Action::Launch(2))));
 s.key(27|(1<<17));for _ in 0..40 {s.key(b'z' as u32);}assert_eq!(s.query_len,24);assert_eq!(s.filtered().1,0);
 assert_eq!(s.key(10),(true,None));s.key(27|(1<<17));s.key(b'z' as u32);s.key(8);assert_eq!(s.filtered().1,3);
 let card=s.card(0);s.input(card.x+10,card.y+10,1,false);s.key(b'n' as u32);
 assert!(s.input(card.x+10,card.y+10,0,false).1.is_none());
}
#[test] fn hidden_desktop_shortcuts_cannot_steal_window_clicks() {
 let mut s=Shell::new();s.catalog.add(b"FILES   BIN",false);
 assert_eq!(click(&mut s,60,60),Some(Action::Launch(0)));
 s.background=false;assert_eq!(click(&mut s,60,60),None);
 s.background=true;s.input(60,60,1,false);s.input(60,60,1,true);assert!(s.input(60,60,0,false).1.is_none());
}
#[test] fn volume_popup_is_captured_bounded_and_outside_clicks_do_not_pass_through() {
 let mut s=Shell::new();click(&mut s,598,576);assert!(s.sound_open);
 assert_eq!(click(&mut s,700,512),Some(Action::Volume(0)));
 assert_eq!(click(&mut s,700,512),Some(Action::Volume(60)));
 for _ in 0..20 {click(&mut s,650,512);}assert_eq!(s.volume,100);
 for _ in 0..20 {click(&mut s,598,512);}assert_eq!(s.volume,0);
 assert!(s.input(300,200,1,false).0);assert!(!s.sound_open);assert!(s.input(300,200,0,false).0);
}

#[test] fn desktop_items_are_occluded_and_captured() {
 let mut s=Shell::new();s.file_count=1;s.files[0].name[..5].copy_from_slice(b"A.TXT");s.files[0].length=5;
 assert_eq!(click(&mut s,180,60),Some(Action::OpenDesktop(0)));
 s.background=false;assert!(click(&mut s,180,60).is_none());
 s.background=true;s.input(180,60,1,false);assert!(s.input(700,500,0,false).1.is_none());
 assert_eq!(click(&mut s,60,370),Some(Action::OpenDesktop(20)));
}
#[test] fn desktop_grid_fits_work_area_without_shortcut_overlap() {
 for i in 0..20 {let r=Shell::desktop_file(i);assert!(r.x>=132&&r.x+r.w<=548&&r.y+r.h<=544);for j in 0..i {let q=Shell::desktop_file(j);assert!(r.x>=q.x+q.w||q.x>=r.x+r.w||r.y>=q.y+q.h||q.y>=r.y+r.h);}}
}
