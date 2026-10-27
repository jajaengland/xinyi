#![allow(dead_code)]
#[path="../src/mouse_packet.rs"]mod mouse_packet;
#[path="../src/desktop_ui.rs"]mod desktop_ui;
#[path="../src/app_catalog.rs"]mod app_catalog;
#[path="../src/desktop_shell.rs"]mod desktop_shell;
use desktop_shell::{Shell,Task,Action};
fn click(s:&mut Shell,x:i32,y:i32)->Option<Action> {s.input(x,y,1,false);s.input(x,y,0,false).1}
#[test] fn menu_consumes_outside_click_and_opens_downward() {
 let mut s=Shell::new();s.catalog.add(b"COUNTER BIN",false);click(&mut s,40,11);assert!(s.open);assert_eq!(s.menu().y,30);
 let(consumed,action,_)=s.input(300,200,1,false);assert!(consumed);assert!(action.is_none());assert!(!s.open);
 assert!(s.input(300,200,0,false).0);click(&mut s,40,11);assert_eq!(click(&mut s,40,46),Some(Action::Launch(0)));
}
#[test] fn task_buttons_select_pid_and_release_capture_is_cancellable() {
 let mut s=Shell::new();s.tasks[2]=Some(Task{pid:7,name:*b"NOTEPAD ",length:7});
 assert_eq!(click(&mut s,400,11),Some(Action::Focus(7)));
 s.input(400,11,1,false);assert!(s.input(700,200,0,false).1.is_none());
 s.input(400,11,1,false);s.input(400,11,1,true);assert!(s.input(400,11,0,false).1.is_none());
 assert_eq!(click(&mut s,768,11),Some(Action::Exit));
}
#[test] fn empty_and_full_catalogs_fit_below_taskbar() {
 let mut s=Shell::new();click(&mut s,40,11);assert!(click(&mut s,40,46).is_none());
 for _ in 0..16 {s.catalog.add(b"COUNTER BIN",false);}let r=s.menu();assert!(r.y+r.h+6<=600);
}
