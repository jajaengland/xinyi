//! Code-drawn desktop icons and chrome. All text uses shared proportional metrics.
use crate::{canvas::{Canvas,text::{self,Font}},desktop_shell::{Shell,Hit},desktop_ui::Rect};
const INK:u32=0x20384d;
const MUTED:u32=0x536c80;
fn tint(color:u32)->u32 {((color&0xfefefe)>>1)+0x182838}
fn icon(c:&mut Canvas<'_>,x:i32,y:i32,size:i32,kind:usize) {
    let u=size/8;
    let colors=[0xe7b550,0x43879b,0x7191b9,0x83acc2,0xb38bc2,0x73ab8a,0x7694cf,0xc99875];
    let accent=colors[kind.min(7)];
    match kind {
        0=>{c.rect(x,y+u,3*u,u,accent);c.rect(x,y+2*u,8*u,5*u,accent);c.rect(x+u,y+3*u,7*u,4*u,0xf1ce7b);}
        1=>{c.rect(x,y+u,8*u,6*u,0x23465b);c.line((x+u,y+3*u),(x+2*u,y+4*u),0xb7e5ed);c.line((x+2*u,y+4*u),(x+u,y+5*u),0xb7e5ed);c.rect(x+4*u,y+5*u,2*u,u/2+1,0xb7e5ed);}
        2=>{c.rect(x+3*u,y,2*u,8*u,accent);c.rect(x,y+3*u,8*u,2*u,accent);c.rect(x+u,y+u,6*u,6*u,accent);c.rect(x+3*u,y+3*u,2*u,2*u,0xd9e7f1);}
        3=>{c.rect(x+u,y,6*u,8*u,0xe9f2f8);c.rect(x+u,y,6*u,u,accent);for i in [3,5,6] {c.rect(x+2*u,y+i*u,4*u,(u/2).max(1),0x6d93af);}}
        4=>{c.rect(x,y+3*u,2*u,2*u,accent);c.rect(x+2*u,y+2*u,u,4*u,accent);c.rect(x+3*u,y+u,u,6*u,accent);c.line((x+5*u,y+2*u),(x+6*u,y+3*u),accent);c.line((x+6*u,y+3*u),(x+6*u,y+5*u),accent);c.line((x+6*u,y+5*u),(x+5*u,y+6*u),accent);}
        5=>{c.rect(x,y+2*u,8*u,4*u,accent);c.rect(x+u,y+u,2*u,6*u,accent);c.rect(x+5*u,y+u,2*u,6*u,accent);c.rect(x+u,y+3*u,2*u,u,0x244c43);c.rect(x+5*u,y+3*u,u,u,0xe4f1eb);c.rect(x+6*u,y+4*u,u,u,0xe4f1eb);}
        6=>{c.rect(x,y,8*u,8*u,accent);c.label(x+u,y+(size-16)/2,(size-2*u) as u32,0xffffff,b"Aa");}
        _=>{c.rect(x+u,y+u,6*u,6*u,accent);c.rect(x+2*u,y+2*u,4*u,4*u,0xf1d5bb);for i in [2,4,6] {c.rect(x+i*u,y,u,8*u,accent);c.rect(x,y+i*u,8*u,u,accent);}}
    }
}
fn button(c:&mut Canvas<'_>,r:Rect,active:bool,hover:bool) {
    c.rect(r.x,r.y,r.w,r.h,if active {0x285e83}else if hover {0x28455d}else{0x1d3449});
}
pub fn background(c:&mut Canvas<'_>,s:&Shell,color:u32) {
    c.rect(0,0,800,544,color);
    let subtle=tint(color);
    for (x,y,w,h) in [(628,56,100,6),(722,56,6,96),(596,88,100,6),(690,88,6,96)] {c.rect(x,y,w,h,subtle);}
    let ink=crate::theme_model::ink(color);
    c.label(620,212,132,ink,b"Xinyi");
    for i in 0..3 {
        let Some(index)=s.shortcut_app(i) else {continue;};let entry=&s.catalog.entries[index];let r=Shell::shortcut(i);
        if s.hit()==Some(Hit::Shortcut(i)) {c.rect(r.x,r.y,r.w,r.h,subtle);}
        icon(c,r.x+28,r.y+8,40,entry.icon as usize);
        let width=text::width(entry.title(),Font::Ui).min(88);
        c.label(r.x+(96-width as i32)/2,r.y+60,width,ink,entry.title());
    }
    let r=Shell::shortcut(3);if s.hit()==Some(Hit::DesktopFolder){c.rect(r.x,r.y,r.w,r.h,subtle);}
    icon(c,r.x+28,r.y+8,40,0);c.label(r.x+22,r.y+60,72,ink,b"Desktop");
    for i in 0..s.file_count {
        let e=&s.files[i];let r=Shell::desktop_file(i);
        if s.selected==Some(i)||s.hit()==Some(Hit::Desktop(i)) {c.rect(r.x,r.y,r.w,r.h,subtle);}
        icon(c,r.x+28,r.y+8,40,if e.folder {0}else{3});
        let label=&e.name[..e.length];let width=text::width(label,Font::Ui).min(92);
        c.label(r.x+(96-width as i32)/2,r.y+60,92,ink,label);
    }
}
pub fn chrome(c:&mut Canvas<'_>,s:&Shell) {
    c.rect(0,544,800,56,0x142637);c.rect(0,544,800,1,0x49647a);
    let r=Shell::apps();button(c,r,s.open,s.hit()==Some(Hit::Apps));
    for (x,y) in [(r.x+12,r.y+9),(r.x+20,r.y+9),(r.x+12,r.y+17),(r.x+20,r.y+17)] {c.rect(x,y,5,5,0x8bd1ef);}
    c.label(r.x+34,r.y+8,46,0xffffff,b"Apps");
    for (i,t) in s.tasks.iter().enumerate() {
        let Some(t)=t else {continue;};let r=Shell::task(i);let active=s.focused==Some(t.pid);
        button(c,r,active,s.hit()==Some(Hit::Task(i)));c.rect(r.x+8,r.y+9,3,14,0x8bd1ef);
        let title=s.find(&t.name[..t.length]).map(|i|s.catalog.entries[i].title()).unwrap_or(&t.name[..t.length]);
        c.label(r.x+18,r.y+8,(r.w-24) as u32,0xffffff,title);
        if active {c.rect(r.x+10,r.y+r.h-3,r.w-20,2,0x8bd1ef);}
    }
    let r=Shell::sound();button(c,r,s.sound_open,s.hit()==Some(Hit::Sound));icon(c,r.x+6,r.y+4,24,4);
    if s.volume==0 {c.line((r.x+6,r.y+6),(r.x+29,r.y+25),0xf3b2a7);}
    c.label(640,558,72,0xffffff,&s.clock);c.label(640,576,72,0xaec3d4,b"Local");
    let r=Shell::exit();button(c,r,false,s.hit()==Some(Hit::Exit));c.label(r.x+16,r.y+8,44,0xffffff,b"Shell");
    if s.open {
        let r=s.menu();c.rect(r.x+4,r.y+4,r.w,r.h,0x102333);c.rect(r.x,r.y,r.w,r.h,0x8299ad);c.rect(r.x+1,r.y+1,r.w-2,r.h-2,0xf1f5f8);
        c.rect(r.x+1,r.y+1,r.w-2,50,0xe0eaf2);c.label(r.x+16,r.y+16,360,INK,b"Applications");c.label(r.x+394,r.y+16,72,MUTED,b"Xinyi");
        c.rect(r.x+16,r.y+56,r.w-32,28,0x7099b9);c.rect(r.x+17,r.y+57,r.w-34,26,0xffffff);
        let label=if s.query_len==0 {b"Type to find an app..." as &[u8]}else{&s.query[..s.query_len]};
        c.label(r.x+24,r.y+62,(r.w-52) as u32,if s.query_len==0 {MUTED}else{INK},label);
        if s.query_len>0 {let x=r.x+24+text::width(label,Font::Ui) as i32;c.rect(x,r.y+62,1,16,0x2f78aa);}
        let(items,n)=s.filtered();
        if n==0 {c.paragraph(r.x+24,r.y+110,(r.w-48) as u32,64,MUTED,b"No applications match. Try a shorter name.");}
        for (slot,&index) in items[..n].iter().enumerate() {
            let entry=&s.catalog.entries[index];let card=s.card(slot);
            if s.menu_selection.map_or(s.hit()==Some(Hit::App(index)),|selected|selected==index) {c.rect(card.x,card.y,card.w,card.h,0xd9eaf6);c.rect(card.x,card.y,2,card.h,0x357da9);}
            icon(c,card.x+8,card.y+8,24,entry.icon as usize);
            c.label(card.x+42,card.y+3,(card.w-48) as u32,INK,entry.title());
            c.label(card.x+42,card.y+21,(card.w-48) as u32,MUTED,entry.description());
        }
        c.label(r.x+16,r.y+r.h-20,(r.w-32) as u32,MUTED,if s.length>0 {&s.message[..s.length]}else if s.catalog.truncated {b"Catalog full: first 16 apps shown"}else{b"Enter opens  |  Arrows select  |  Escape closes"});
    }
    if s.sound_open {
        let r=Shell::sound_panel();c.rect(r.x+3,r.y+3,r.w,r.h,0x102333);c.rect(r.x,r.y,r.w,r.h,0x8299ad);c.rect(r.x+1,r.y+1,r.w-2,r.h-2,0xf1f5f8);
        c.label(r.x+12,r.y+12,190,INK,b"System sounds");
        let percent=[b'0'+(s.volume/100) as u8,b'0'+(s.volume/10%10) as u8,b'0'+(s.volume%10) as u8,b'%'];
        c.label(r.x+12,r.y+38,180,MUTED,&percent);c.rect(r.x+12,r.y+62,196,4,0xd3dfe8);c.rect(r.x+12,r.y+62,(s.volume*196/100) as i32,4,0x357da9);
        for (hit,label) in [(Hit::Less,b"-" as &[u8]),(Hit::More,b"+"),(Hit::Mute,if s.volume==0 {b"On"}else{b"Mute"})] {
            let b=Shell::volume_button(hit);c.rect(b.x,b.y,b.w,b.h,if s.hit()==Some(hit){0xc6dfef}else{0xe0eaf2});c.label(b.x+8,b.y+7,(b.w-12) as u32,INK,label);
        }
    }
}
