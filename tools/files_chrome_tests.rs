#![allow(dead_code)]
#[path="../programs/files/chrome.rs"] mod chrome;
#[test]fn commands_fit_small_and_large_clients_and_ignore_gaps_and_address_text() {
    for width in [640,792,1016,1272] {
        let mut rects=Vec::new();
        for (i,(action,_)) in chrome::PRIMARY.iter().enumerate() {let r=chrome::primary(width,i);assert_eq!(chrome::hit(width,400,(r.x+r.w/2) as i32,16),Some(*action));rects.push(r);}
        for pair in rects.windows(2) {assert!(pair[0].x+pair[0].w<=pair[1].x);}
        assert!(rects.last().unwrap().x+rects.last().unwrap().w<=width-8);
        for (i,(action,_)) in chrome::NAV.iter().enumerate() {let r=chrome::nav(i);assert_eq!(chrome::hit(width,400,(r.x+8) as i32,44),Some(*action));}
        assert_eq!(chrome::hit(width,400,240,44),None,"The address is not a hidden command");
        assert_eq!(chrome::hit(width,400,-1,12),None);assert_eq!(chrome::hit(width,400,10,32),None);
        for next in [false,true] {let r=chrome::page(width,400,next);assert_eq!(chrome::hit(width,400,(r.x+8) as i32,376),Some(if next {7}else{6}));}
    }
}
