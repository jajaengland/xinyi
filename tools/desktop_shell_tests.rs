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

#[test] fn rearranging_swaps_and_survives_directory_refresh() {
 let mut s=Shell::new();let mut files=[desktop_shell::DesktopEntry::EMPTY;20];
 files[0].name[0]=b'A';files[0].length=1;files[1].name[0]=b'B';files[1].length=1;
 s.update_files(files,2);s.arrange(0,1);assert_eq!(&s.slots[..2],&[1,0]);
 s.arrange(0,19);assert_eq!(&s.slots[..2],&[19,0]);s.update_files(files,2);assert_eq!(&s.slots[..2],&[19,0]);
 files.swap(0,1);s.update_files(files,2);assert_eq!(&s.slots[..2],&[0,19]);
 files[1]=desktop_shell::DesktopEntry::EMPTY;s.update_files(files,1);files[1].name[0]=b'C';files[1].length=1;s.update_files(files,2);assert_eq!(&s.slots[..2],&[0,1]);
 assert_eq!(Shell::nearest_slot(-100,-100),0);assert_eq!(Shell::nearest_slot(799,599),19);
}

#[test] fn layout_roundtrip_rejects_corruption_without_changing_positions() {
 let mut s=Shell::new();let mut files=[desktop_shell::DesktopEntry::EMPTY;20];files[0].name[0]=b'A';files[0].length=1;files[1].name[0]=b'B';files[1].length=1;s.update_files(files,2);s.arrange(0,19);
 let mut data=[0;305];let n=s.encode_layout(&mut data);let mut fresh=Shell::new();fresh.update_files(files,2);assert!(fresh.apply_layout(&data[..n]));assert_eq!(&fresh.slots[..2],&[19,1]);
 let before=fresh.slots;data[34]=19;assert!(!fresh.apply_layout(&data[..n]));assert_eq!(fresh.slots,before);assert!(!fresh.apply_layout(&data[..4]));
}

#[test] fn desktop_context_menu_clamps_captures_and_dismisses() {
 let mut s=Shell::new();assert!(s.input(799,543,2,false).0);s.input(799,543,0,false);
 let r=s.context_rect().unwrap();assert!(r.x+r.w<=800&&r.y+r.h<=544);assert_eq!(click(&mut s,r.x+20,r.y+40),Some(Action::OpenDesktop(21)));assert!(s.context.is_none());
 s.input(700,300,2,false);s.input(700,300,0,false);assert_eq!(s.key(27),(true,None));assert!(s.context.is_none());
 s.background=false;s.input(700,300,2,false);assert!(s.context.is_none());s.input(700,300,0,false);
 s.background=true;s.input(700,300,2,false);s.input(700,300,0,false);assert!(s.input(500,100,1,false).0);assert!(s.context.is_none());assert!(s.input(500,100,0,false).1.is_none());
}

#[test] fn desktop_multi_selection_and_group_arrangement_remain_unique() {
 let mut s=Shell::new();let mut files=[desktop_shell::DesktopEntry::EMPTY;20];for i in 0..4 {files[i].name[0]=b'A'+i as u8;files[i].length=1;}s.update_files(files,4);
 s.modifiers=1<<17;assert!(click(&mut s,180,60).is_none());assert!(click(&mut s,180,164).is_none());assert_eq!(s.selection,3);
 s.modifiers=0;s.arrange_group(0,6);assert_eq!(&s.slots[..4],&[6,7,2,3]);
 s.arrange_group(0,19);assert_eq!(&s.slots[..2],&[18,19]);
 s.slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19];s.arrange_group(0,2);assert_eq!(&s.slots[..4],&[2,3,0,1]);
 s.modifiers=1<<17;click(&mut s,180,268);assert_eq!(s.selection,2);
 s.modifiers=1<<16;click(&mut s,180,60);assert_eq!(s.selection,13);
 s.modifiers=0;click(&mut s,700,500);assert_eq!(s.selection,0);
}

#[test] fn desktop_keyboard_selection_is_focus_scoped_and_uses_grid() {
 let mut s=Shell::new();let mut files=[desktop_shell::DesktopEntry::EMPTY;20];for i in 0..4 {files[i].name[0]=b'A'+i as u8;files[i].length=1;}s.update_files(files,4);
 s.slots[..4].copy_from_slice(&[0,1,5,6]);assert!(s.key(0x104).0);assert_eq!(s.selection,1);s.key(0x103);assert_eq!(s.selection,4);s.key(0x101|(1<<16));assert_eq!(s.selection,12);assert_eq!(s.key(10),(true,None));s.key(0x104);assert_eq!(s.key(10),(true,Some(Action::OpenDesktop(0))));
 s.key(65|(1<<17));assert_eq!(s.selection,15);s.key(27);assert_eq!(s.selection,0);
 s.keyboard=false;assert_eq!(s.key(0x104),(false,None));assert_eq!(s.key(65|(1<<17)),(false,None));
}

#[test] fn desktop_clipboard_shortcuts_are_focus_scoped() {
 let mut s=Shell::new();for (key,op) in [(b'c',0),(b'x',1),(b'v',2)] {assert_eq!(s.key(key as u32|(1<<17)),(true,Some(Action::FileAction(op))));}
 s.keyboard=false;assert_eq!(s.key(b'c' as u32|(1<<17)),(false,None));
}

#[test] fn desktop_context_clipboard_rows_match_selection() {
 let mut s=Shell::new();s.file_count=1;s.files[0].name[0]=b'A';s.files[0].length=1;
 s.input(180,60,2,false);s.input(180,60,0,false);let r=s.context_rect().unwrap();assert_eq!(r.h,150);assert_eq!(click(&mut s,r.x+10,r.y+75),Some(Action::FileAction(0)));
 s.input(700,500,2,false);s.input(700,500,0,false);let r=s.context_rect().unwrap();assert_eq!(r.h,90);assert_eq!(click(&mut s,r.x+10,r.y+75),Some(Action::FileAction(2)));
}

#[test]fn desktop_recycle_is_a_confirmation_request_and_focus_scoped(){
 let mut s=Shell::new();assert_eq!(s.key(0x106),(true,Some(Action::FileAction(3))));s.keyboard=false;assert_eq!(s.key(0x106),(false,None));
 s.file_count=1;s.files[0].name[0]=b'A';s.files[0].length=1;s.input(180,60,2,false);s.input(180,60,0,false);let r=s.context_rect().unwrap();assert_eq!(click(&mut s,r.x+10,r.y+135),Some(Action::FileAction(3)));
}
