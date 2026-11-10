#![allow(dead_code)]
#[path="../src/display_layout.rs"]mod display_layout;
#[path="../src/app_catalog.rs"]mod app_catalog;
#[path="../src/mouse_packet.rs"]mod mouse_packet;
#[path="../src/desktop_ui.rs"]mod desktop_ui;
#[path="../src/window_protocol.rs"]mod window_protocol;
#[path="../src/app_window_ui.rs"]mod app_window_ui;
#[path="../src/desktop_shell.rs"]mod desktop_shell;
#[test]fn config_is_bounded_and_rejects_unknown_sizes() {
    for (w,h) in display_layout::MODES {let b=display_layout::encode(w,h);assert_eq!(display_layout::decode(&b),Some((w,h)));for n in 0..8 {assert!(display_layout::decode(&b[..n]).is_none());}}
    assert!(display_layout::decode(&display_layout::encode(1920,1080)).is_none());
    assert!(display_layout::decode(b"XDP2\x00\x04\x00\x03").is_none());
}
#[test]fn higher_resolution_work_area_controls_pointer_and_maximise_agree() {
    use desktop_shell::Shell;
    display_layout::set(1024,768);
    assert_eq!(Shell::work_area(),desktop_ui::Rect{x:0,y:0,w:1024,h:704});
    assert_eq!(Shell::bar(),desktop_ui::Rect{x:0,y:704,w:1024,h:64});
    let mut shell=Shell::new();let exit=Shell::exit();assert_eq!(shell.input(exit.x+8,exit.y+8,1,false).0,true);
    assert_eq!(shell.input(exit.x+8,exit.y+8,0,false).1,Some(desktop_shell::Action::Exit));
    let mut ui=app_window_ui::State::sized(0,640,400,56,98);ui.work_area=Shell::work_area();ui.limits=Some((640,400,1280,1024));
    let button=ui.maximize_button();let at=|s:&mut app_window_ui::State,buttons|s.update(mouse_packet::Event{dx:(button.x+8-s.x) as i16,dy:(button.y+8-s.y) as i16,buttons,lost:false,wheel:0});
    at(&mut ui,1);let event=at(&mut ui,0).1.unwrap();assert_eq!((event.x,event.y),(1016,672));ui.resized(event.x,event.y);
    assert_eq!(ui.client(),desktop_ui::Rect{x:4,y:28,w:1016,h:672});
    let mut legacy=app_window_ui::State::sized(0,656,400,56,98);legacy.work_area=Shell::work_area();legacy.limits=Some((656,400,792,544));
    let b=legacy.maximize_button();legacy.update(mouse_packet::Event{dx:(b.x+8-legacy.x) as i16,dy:(b.y+8-legacy.y) as i16,buttons:1,lost:false,wheel:0});
    let event=legacy.update(mouse_packet::Event{dx:0,dy:0,buttons:0,lost:false,wheel:0}).1.unwrap();legacy.resized(event.x,event.y);
    assert_eq!(legacy.window.rect.w,legacy.client().w+8);assert_eq!(legacy.window.rect.h,legacy.client().h+32);
    let mut dragged=app_window_ui::State::sized(0,640,400,56,98);dragged.work_area=Shell::work_area();
    dragged.update(mouse_packet::Event{dx:-250,dy:-190,buttons:1,lost:false,wheel:0});
    dragged.update(mouse_packet::Event{dx:900,dy:0,buttons:1,lost:false,wheel:0});
    assert!(dragged.window.rect.x>800,"Window drag must use the expanded work area");
    display_layout::set_scaled(1280,1024,125);
    assert_eq!((display_layout::width(),display_layout::height()),(1024,819));
    let mut motion=display_layout::PointerMotion::new();
    let mut sum=(0,0);
    for _ in 0..1280 {let (x,y)=motion.delta(1,1);sum.0+=x;sum.1+=y;}
    assert_eq!(sum,(1024,1023));
    for _ in 0..1280 {let (x,y)=motion.delta(-1,-1);sum.0+=x;sum.1+=y;}
    assert_eq!(sum,(0,0),"Small and reverse motion must retain fractional pixels");
    motion.reset();
    display_layout::set(800,600);
}

#[test]fn scaling_spans_cover_every_physical_pixel_once_and_sample_the_same_cell() {
    use display_layout::{logical_size,edge,sample};
    for (pw,ph) in display_layout::MODES {
        for percent in [100,125,150] {
            let Some((w,h))=logical_size(pw,ph,percent) else {continue;};
            for (physical,logical) in [(pw as usize,w as usize),(ph as usize,h as usize)] {
                assert_eq!(edge(0,physical,logical),0);assert_eq!(edge(logical,physical,logical),physical);
                for cell in 0..logical {
                    let (a,b)=(edge(cell,physical,logical),edge(cell+1,physical,logical));assert!(b>a);
                    for px in a..b {assert_eq!(sample(px,physical,logical),cell);}
                }
            }
        }
    }
    assert_eq!(logical_size(1280,1024,150),Some((853,682)));
    assert!(logical_size(800,600,125).is_none());assert!(logical_size(1280,720,125).is_none());
    for percent in [100,125,150] {
        let record=display_layout::encode_scale(percent);assert_eq!(display_layout::decode_scale(&record),Some(percent));
        for n in 0..5 {assert!(display_layout::decode_scale(&record[..n]).is_none());}
    }
    for invalid in [b"XUS1\x00" as &[u8],b"XUS1\xff",b"XUS2\x64",b"XUS1\x64\x00"] {assert!(display_layout::decode_scale(invalid).is_none());}
    assert!(logical_size(1280,1024,0).is_none());assert!(logical_size(1280,1024,200).is_none());
}
