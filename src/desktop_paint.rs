//! Code-drawn desktop icons and chrome. All text uses shared proportional metrics.
use crate::{canvas::{Canvas,text::{self,Font}},desktop_shell::{Shell,Hit},desktop_ui::Rect};
pub const INK:u32=0x20384d;
pub const BORDER:u32=0x94aabb;
pub const PANEL:u32=0xf3f6f8;
pub const ACCENT:u32=0x8bd1cf;
const MUTED:u32=0x536c80;
fn mix(a:u32,b:u32,amount:u32)->u32 {
    let channel=|shift:u32| ->u32 {(((a>>shift)&255)*(100-amount)+((b>>shift)&255)*amount)/100};
    channel(16)<<16|channel(8)<<8|channel(0)
}
fn tint(color:u32)->u32 {mix(color,crate::theme_model::ink(color),12)}
// Small stepped corners stay crisp at every supported bitmap scale.
fn tile(c:&mut Canvas<'_>,r:Rect,color:u32) {
    c.rect(r.x+2,r.y,r.w-4,r.h,color);c.rect(r.x,r.y+2,r.w,r.h-4,color);
}
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
    tile(c,r,if active {0x2b5268}else if hover {0x28455d}else{0x1c3346});
    if active {c.rect(r.x+8,r.y+r.h-3,r.w-16,2,ACCENT);}
}
pub fn background(c:&mut Canvas<'_>,s:&Shell,color:u32) {
    c.rect(0,0,crate::display_layout::width(),Shell::work_area().h,color);
    let image=crate::wallpaper::paint(c);
    let subtle=tint(color);
    let (w,h)=(crate::display_layout::width(),Shell::work_area().h);
    // Quiet diagonal bands follow the work area rather than a fixed resolution.
    if !image {for y in 0..h {
        let x=w*2/3+(y-h/2)/3;
        c.rect(x,y,w/7,1,mix(color,0x8bd1cf,7));
        c.rect(x+w/7,y,w/9,1,mix(color,0x8bd1cf,3));
    }
    }
    let ink=if image{0xffffff}else{crate::theme_model::ink(color)};
    let brand_x=w-200;let brand_y=h-100;
    if !image {c.rect(brand_x,brand_y,24,3,mix(color,ink,45));
    c.label(brand_x,brand_y+14,160,ink,b"Xinyi");
    c.label(brand_x,brand_y+36,160,mix(color,ink,65),b"Your workspace");}
    for i in [0,1,2,4] {
        let Some(index)=s.shortcut_app(i) else {continue;};let entry=&s.catalog.entries[index];let r=Shell::shortcut(i);
        if s.hit()==Some(Hit::Shortcut(i)) {tile(c,r,subtle);c.rect(r.x+30,r.y+r.h-4,36,2,ACCENT);}
        icon(c,r.x+28,r.y+8,40,entry.icon as usize);
        let width=text::width(entry.title(),Font::Ui).min(88);
        if image{c.rect(r.x,r.y+58,96,20,0x20364f);}c.label(r.x+(96-width as i32)/2,r.y+60,width,ink,entry.title());
    }
    let r=Shell::shortcut(3);if s.hit()==Some(Hit::DesktopFolder){tile(c,r,subtle);c.rect(r.x+30,r.y+r.h-4,36,2,ACCENT);}
    icon(c,r.x+28,r.y+8,40,0);if image{c.rect(r.x,r.y+58,96,20,0x20364f);}c.label(r.x+22,r.y+60,72,ink,b"Desktop");
    for i in 0..s.file_count {
        let e=&s.files[i];let r=s.file_rect(i);
        if s.selection&(1<<i)!=0||s.hit()==Some(Hit::Desktop(i)) {tile(c,r,subtle);if s.selection&(1<<i)!=0 {c.rect(r.x+2,r.y+10,2,r.h-20,ACCENT);}}
        icon(c,r.x+28,r.y+8,40,if e.folder {0}else{3});
        let label=&e.name[..e.length];let width=text::width(label,Font::Ui).min(92);
        if image{c.rect(r.x,r.y+58,96,20,0x20364f);}c.label(r.x+(96-width as i32)/2,r.y+60,92,ink,label);
    }
}
pub fn chrome(c:&mut Canvas<'_>,s:&Shell) {
    c.rect(0,Shell::bar().y,Shell::bar().w,Shell::bar().h,0x142637);c.rect(0,Shell::bar().y,Shell::bar().w,1,0x49647a);
    c.rect(102,Shell::bar().y+20,1,24,0x365064);
    let r=Shell::apps();button(c,r,s.open,s.hit()==Some(Hit::Apps));
    for (x,y) in [(r.x+12,r.y+9),(r.x+20,r.y+9),(r.x+12,r.y+17),(r.x+20,r.y+17)] {c.rect(x,y,5,5,ACCENT);}
    c.label(r.x+34,r.y+8,46,0xffffff,b"Apps");
    for (i,t) in s.tasks.iter().enumerate() {
        let Some(t)=t else {continue;};let r=Shell::task(i);let active=s.focused==Some(t.pid);
        button(c,r,active,s.hit()==Some(Hit::Task(i)));
        let kind=s.find(&t.name[..t.length]).map(|i|s.catalog.entries[i].icon as usize).unwrap_or(3);
        icon(c,r.x+8,r.y+8,16,kind);
        let title=s.find(&t.name[..t.length]).map(|i|s.catalog.entries[i].title()).unwrap_or(&t.name[..t.length]);
        c.label(r.x+30,r.y+8,(r.w-36) as u32,0xffffff,title);

    }
    let r=Shell::sound();button(c,r,s.sound_open,s.hit()==Some(Hit::Sound));icon(c,r.x+6,r.y+4,24,4);
    if s.volume==0 {c.line((r.x+6,r.y+6),(r.x+29,r.y+25),0xf3b2a7);}
    c.label(Shell::bar().w-160,Shell::bar().y+14,72,0xffffff,&s.clock);c.label(Shell::bar().w-160,Shell::bar().y+32,72,0xaec3d4,b"Local");
    let r=Shell::exit();button(c,r,false,s.hit()==Some(Hit::Exit));c.label(r.x+16,r.y+8,44,0xffffff,b"Shell");
    if s.open {
        let r=s.menu();c.rect(r.x+4,r.y+4,r.w,r.h,0x102333);c.rect(r.x,r.y,r.w,r.h,BORDER);c.rect(r.x+1,r.y+1,r.w-2,r.h-2,PANEL);
        c.rect(r.x+1,r.y+1,r.w-2,50,0x203e52);c.rect(r.x+16,r.y+38,32,2,ACCENT);c.label(r.x+16,r.y+12,210,0xffffff,b"Applications");
        for h in [Hit::Shutdown,Hit::Restart]{let b=s.power_button(h);button(c,b,false,s.hit()==Some(h));c.label(b.x+8,b.y+6,(b.w-16) as u32,0xffffff,if h==Hit::Shutdown{b"Shut down"}else{b"Restart"});}
        c.rect(r.x+16,r.y+56,r.w-32,28,0x759daa);c.rect(r.x+17,r.y+57,r.w-34,26,0xffffff);
        let label=if s.query_len==0 {b"Type to find an app..." as &[u8]}else{&s.query[..s.query_len]};
        c.label(r.x+24,r.y+62,(r.w-52) as u32,if s.query_len==0 {MUTED}else{INK},label);
        if s.query_len>0 {let x=r.x+24+text::width(label,Font::Ui) as i32;c.rect(x,r.y+62,1,16,0x2f78aa);}
        let(items,n)=s.filtered();
        if n==0 {c.paragraph(r.x+24,r.y+110,(r.w-48) as u32,64,MUTED,b"No applications match. Try a shorter name.");}
        for (slot,&index) in items[..n].iter().skip(s.app_page*16).take(16).enumerate() {
            let entry=&s.catalog.entries[index];let card=s.card(slot);
            let selected=s.menu_selection.map_or(s.hit()==Some(Hit::App(index)),|selected|selected==index);
            tile(c,Rect{w:card.w-3,h:card.h-3,..card},if selected {0xdceef0}else{0xffffff});
            if selected {c.rect(card.x,card.y+5,2,card.h-13,0x367c85);}
            icon(c,card.x+8,card.y+8,24,entry.icon as usize);
            let mut version=[b'0';11];let mut value=entry.version;
            for i in (1..11).rev() {version[i]+= (value%10) as u8;value/=10;}
            let at=version[1..].iter().position(|b|*b!=b'0').map_or(10,|i|i+1);version[at-1]=b'v';
            let label=&version[at-1..];let vw=if entry.version>0 {text::width(label,Font::Ui) as i32+8}else{0};
            c.label(card.x+42,card.y+3,(card.w-48-vw) as u32,INK,entry.title());
            if vw>0 {c.label(card.x+card.w-vw,card.y+3,vw as u32,MUTED,label);}
            c.label(card.x+42,card.y+21,(card.w-48) as u32,MUTED,entry.description());
        }
        if n>16 {
            for next in [false,true] {
                let rect=s.page_button(next);let enabled=if next {(s.app_page+1)*16<n}else{s.app_page>0};
                if enabled {button(c,rect,false,s.hit()==Some(if next {Hit::NextPage}else{Hit::PreviousPage}));}else{c.rect(rect.x,rect.y,rect.w,rect.h,0xcbd6df);}
                c.label(rect.x+8,rect.y+2,52,if enabled {0xffffff}else{0x708493},if next {b"Next"}else{b"Prev"});
            }
            let page=[b'1'+s.app_page as u8,b'/',b'0'+n.div_ceil(16) as u8];c.label(r.x+330,r.y+442,56,INK,&page);
            c.label(r.x+88,r.y+442,232,MUTED,if s.length>0 {&s.message[..s.length]}else if s.catalog.truncated {b"First 64 apps shown"}else{b"PgUp/PgDn | Enter opens"});
        } else {c.label(r.x+16,r.y+r.h-20,(r.w-32) as u32,MUTED,if s.length>0 {&s.message[..s.length]}else if s.catalog.truncated {b"Catalog full: first 64 apps shown"}else{b"Enter opens  |  Arrows select  |  Escape closes"});}
    }
    if s.open {if let Some(restart)=s.power {let r=s.menu();let d=Rect{x:r.x+24,y:r.y+132,w:432,h:204};c.rect(d.x+4,d.y+4,d.w,d.h,0x102333);c.rect(d.x,d.y,d.w,d.h,0xf3f6f8);c.rect(d.x,d.y,d.w,30,0x203e52);c.label(d.x+12,d.y+7,408,0xffffff,if restart{b"Restart Xinyi?"}else{b"Shut down Xinyi?"});c.paragraph(d.x+16,d.y+48,400,80,INK,b"All running applications will close. Unsaved work will be lost. Save your work before continuing.");for h in [Hit::PowerConfirm,Hit::PowerCancel]{let b=s.power_button(h);button(c,b,false,s.hit()==Some(h));c.label(b.x+10,b.y+6,(b.w-20) as u32,0xffffff,if h==Hit::PowerCancel{b"Cancel (Esc)"}else if restart{b"Restart (Y)"}else{b"Shut down (Y)"});}}}
    if s.sound_open {
        let r=Shell::sound_panel();c.rect(r.x+3,r.y+3,r.w,r.h,0x102333);c.rect(r.x,r.y,r.w,r.h,BORDER);c.rect(r.x+1,r.y+1,r.w-2,r.h-2,PANEL);
        c.label(r.x+12,r.y+12,190,INK,b"System sounds");
        let percent=[b'0'+(s.volume/100) as u8,b'0'+(s.volume/10%10) as u8,b'0'+(s.volume%10) as u8,b'%'];
        c.label(r.x+12,r.y+38,180,MUTED,&percent);c.rect(r.x+12,r.y+62,196,4,0xd3dfe8);c.rect(r.x+12,r.y+62,(s.volume*196/100) as i32,4,0x357da9);
        for (hit,label) in [(Hit::Less,b"-" as &[u8]),(Hit::More,b"+"),(Hit::Mute,if s.volume==0 {b"On"}else{b"Mute"})] {
            let b=Shell::volume_button(hit);c.rect(b.x,b.y,b.w,b.h,if s.hit()==Some(hit){0xc6dfef}else{0xe0eaf2});c.label(b.x+8,b.y+7,(b.w-12) as u32,INK,label);
        }
    }
    if let Some(r)=s.context_rect() {
        c.rect(r.x+3,r.y+3,r.w,r.h,0x102333);c.rect(r.x,r.y,r.w,r.h,BORDER);c.rect(r.x+1,r.y+1,r.w-2,r.h-2,PANEL);
        for row in 0..(r.h/30) as usize {let y=r.y+row as i32*30;
            if s.hit()==Some(Hit::DesktopMenu(row)){c.rect(r.x+1,y+1,r.w-2,28,0xc6dfef);}
            let label:&[u8]=if row>=2 {if s.context.unwrap().2.is_none(){b"Paste"}else if row==2{b"Copy"}else if row==3{b"Cut"}else{b"Recycle"}}else if row==1 {b"Reset icon positions" as &[u8]}else if s.context.unwrap().2.is_some(){b"Open"}else{b"Open Desktop"};
            c.label(r.x+10,y+7,156,INK,label);
        }
    }

}
