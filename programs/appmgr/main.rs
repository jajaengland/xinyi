#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
use alloc::vec::Vec;
#[path="../../sdk/xinyi.rs"]mod xinyi;
#[path="../files/app_review.rs"]mod review;
use xinyi::{directory,window::{self,Draw},widgets::{Button,Rect},gui,key,app_manifest::Manifest};
#[global_allocator]static HEAP:xinyi::heap::Allocator=xinyi::heap::Allocator::new();
#[alloc_error_handler]fn oom(_:core::alloc::Layout)->! {xinyi::exit(12)}
#[derive(Clone,Copy)]
struct Entry {id:[u8;8],len:usize,metadata:Option<Manifest>}
impl Entry {fn id(&self)->&[u8] {&self.id[..self.len]}fn title(&self)->&[u8] {self.metadata.as_ref().map_or(self.id(),|m|m.title())}}
struct Manager {entries:Vec<Entry>,selected:usize,offset:usize,volume:u32,status:&'static[u8],buttons:[Button;7],confirm:[Button;2],pending:Option<Entry>}
impl Manager {
    fn refresh(&mut self) {
        self.pending=None;self.entries.clear();self.volume=directory::volume();self.selected=0;self.offset=0;
        if directory::cd(b"/APPS")<0 {self.status=b"No /APPS folder on this drive.";return;}
        if self.entries.try_reserve(64).is_err() {self.status=b"Not enough memory to read installed apps.";return;}
        let mut offset=0u16;let mut listing=directory::Listing::new();
        loop {
            let result=directory::list(b".",offset,&mut listing);
            if result<0||self.volume!=directory::volume() {self.entries.clear();self.status=b"Drive changed or applications could not be read. Refresh to retry.";return;}
            for item in &listing.entries[..listing.count as usize] {
                let name=&item.name[..item.length as usize];
                if item.flags&1==0||name.is_empty()||name.len()>8||!name[0].is_ascii_alphabetic()||!name.iter().all(|b|b.is_ascii_alphanumeric()||*b==b'_') {continue;}
                if self.entries.len()==64 {break;}
                let mut id=[0;8];id[..name.len()].copy_from_slice(name);
                self.entries.push(Entry{id,len:name.len(),metadata:review::load(name,self.volume).ok()});
            }
            let next=offset as u32+listing.count;
            if self.entries.len()==64||listing.count==0||next>=listing.total||next>=512 {break;}
            offset=next as u16;
        }
        self.entries.sort_unstable_by(|a,b|a.id().cmp(b.id()));
        self.status=if self.entries.len()==64 {b"Showing up to 64 installed bundles."}else{b"Select an app to inspect its version and access."};
    }
    fn select(&mut self,index:usize) {self.selected=index.min(self.entries.len().saturating_sub(1));self.offset=self.selected/16*16;}
    fn action(&mut self,index:usize)->bool {
        match index {
            3=>{self.refresh();return false;},4=>return true,
            5=>{self.select(self.offset.saturating_sub(16));return false;},
            6=>{if self.offset+16<self.entries.len() {self.select(self.offset+16);}return false;},_=>(),
        }
        let Some(entry)=self.entries.get(self.selected).copied() else {self.status=b"No application selected.";return false;};
        if self.volume!=directory::volume() {self.status=b"Drive changed. Refresh before managing applications.";return false;}
        if index==2 {
            if entry.id().eq_ignore_ascii_case(b"APPMGR") {self.status=b"Applications cannot remove its own running bundle.";return false;}
            self.pending=Some(entry);for b in &mut self.confirm {b.reset(1);}return false;
        }
        let mut command=[0;128];let mut length=0;
        if index==1 {let prefix=b"/APPS/FILES/FILES.BIN ";command[..prefix.len()].copy_from_slice(prefix);length=prefix.len();}
        command[length..length+6].copy_from_slice(b"/APPS/");length+=6;
        command[length..length+entry.len].copy_from_slice(entry.id());length+=entry.len;
        if index==0 {
            let Some(metadata)=entry.metadata else {self.status=b"Metadata is missing or invalid. Open the folder to inspect it.";return false;};
            command[length]=b'/';length+=1;command[length..length+metadata.executable_len].copy_from_slice(&metadata.executable[..metadata.executable_len]);length+=metadata.executable_len;
        }
        self.status=if xinyi::launch_on(&command[..length],self.volume)<0 {b"Could not launch. Close another app or refresh the list."}else{b"Launch requested."};false
    }
    fn remove(&mut self) {
        let Some(entry)=self.pending.take() else{return;};
        let result=directory::app_action(entry.id(),1,self.volume);
        if result==0 {self.refresh();self.status=b"App removed. Your documents were kept.";}
        else {self.status=match result {-16=>b"App or folder is in use. Close it before removing the bundle.",-116=>b"Drive changed. Refresh before trying again.",_=>b"Removal failed. Open the app folder to inspect the bundle."};}
    }
    fn paint(&self,w:u32) {
        window::draw(w,&[Draw::rect(0,0,640,400,0xf3f6f8),Draw::rect(0,0,640,32,0x203e52),Draw::label(12,8,616,0xffffff,b"Applications - installed bundles"),Draw::rect(8,76,264,264,0xffffff),Draw::rect(288,76,344,272,0xffffff),Draw::rect(0,372,640,1,0xc6d3dc),Draw::label(12,380,616,0x20384d,self.status)]);
        for (i,label) in [b"Launch" as &[u8],b"Open folder",b"Remove",b"Refresh",b"Close",b"Previous",b"Next"].iter().enumerate() {gui::button_flat(w,&self.buttons[i],label,i==0);}
        for (slot,entry) in self.entries.iter().skip(self.offset).take(16).enumerate() {
            let y=80+slot as u32*16;let selected=self.selected==self.offset+slot;
            window::draw(w,&[Draw::rect(8,y,264,16,if selected {0x2b5268}else{0xffffff}),Draw::label(16,y,248,if selected {0xffffff}else{0x20384d},entry.title())]);
        }
        if let Some(entry)=self.entries.get(self.selected) {
            window::draw(w,&[Draw::label(300,88,320,0x20384d,entry.title()),Draw::text(300,112,0,entry.id())]);
            if let Some(metadata)=entry.metadata {
                let mut digits=[b'0';10];let mut version=metadata.version;for n in digits.iter_mut().rev(){*n+=(version%10) as u8;version/=10;}
                let start=digits.iter().position(|&b|b!=b'0').unwrap_or(9);let(profile,permissions)=review::profile(metadata.access);
                window::draw(w,&[Draw::text(300,136,0,b"Version:"),Draw::text(380,136,0,if metadata.version==0 {b"Legacy"}else{&digits[start..]}),Draw::label(300,164,320,0x20384d,profile),Draw::paragraph(300,188,320,64,0x20384d,permissions),Draw::paragraph(300,260,320,48,0x536c80,metadata.description())]);
            } else {window::draw(w,&[Draw::paragraph(300,148,320,64,0x536c80,b"Metadata is missing or invalid. This bundle can still be inspected or removed.")]);}
            window::draw(w,&[Draw::paragraph(300,312,320,32,0x536c80,b"Profiles limit services; they are not a complete security sandbox.")]);
        }
        if let Some(entry)=self.pending {
            gui::dialog_flat(w,b"Remove application?",Rect::new(104,132,432,136));
            window::draw(w,&[Draw::label(120,172,400,0x20384d,entry.title()),Draw::label(120,196,400,0x536c80,b"Deletes this app bundle. Your documents remain.")]);
            gui::button_flat(w,&self.confirm[0],b"Remove",true);gui::button_flat(w,&self.confirm[1],b"Cancel",false);
        }
        window::present(w);
    }
}
fn main()->u32 {
    let w=window::open(b"Applications");if w<0{return 1;}let w=w as u32;
    let mut manager=Manager{entries:Vec::new(),selected:0,offset:0,volume:directory::volume(),status:b"Loading installed applications...",pending:None,
        buttons:[Button::new(Rect::new(8,40,88,28)),Button::new(Rect::new(104,40,112,28)),Button::new(Rect::new(224,40,96,28)),Button::new(Rect::new(328,40,96,28)),Button::new(Rect::new(432,40,80,28)),Button::new(Rect::new(8,344,120,24)),Button::new(Rect::new(144,344,120,24))],
        confirm:[Button::new(Rect::new(128,228,112,28)),Button::new(Rect::new(264,228,112,28))]};
    manager.paint(w);manager.refresh();let mut mouse=0;
    loop {
        manager.paint(w);let mut e=window::Event::default();if window::event(w,&mut e,true)<0||e.kind==window::CLOSE{return 0;}
        if e.kind==window::RESET {mouse=e.buttons;for b in manager.buttons.iter_mut().chain(manager.confirm.iter_mut()){b.reset(e.buttons);}continue;}
        if manager.pending.is_some() {
            if e.kind==window::KEY {match e.code as u16 {89|121=>manager.remove(),78|110|27=>manager.pending=None,_=>()}}
            if e.kind==window::POINTER {let mut action=None;for(i,b)in manager.confirm.iter_mut().enumerate(){if b.pointer(e.x,e.y,e.buttons){action=Some(i);}}match action {Some(0)=>manager.remove(),Some(1)=>manager.pending=None,_=>()}mouse=e.buttons;}
            continue;
        }
        if e.kind==window::KEY {
            let code=e.code as u16;match code {
                27=>return 0,10=>{manager.action(0);},key::DELETE=>{manager.action(2);},key::F5=>manager.refresh(),
                key::UP=>manager.select(manager.selected.saturating_sub(1)),key::DOWN=>manager.select(manager.selected+1),
                key::HOME=>manager.select(0),key::END=>manager.select(manager.entries.len().saturating_sub(1)),
                key::PAGE_UP=>{manager.action(5);},key::PAGE_DOWN=>{manager.action(6);},_=>(),
            }
            if e.code&key::CTRL!=0&&matches!(code,81|113){return 0;}
        }
        if e.kind==window::POINTER {
            let mut action=None;for(i,b)in manager.buttons.iter_mut().enumerate(){if b.pointer(e.x,e.y,e.buttons){action=Some(i);}}
            if let Some(i)=action {if manager.action(i){return 0;}}
            if e.buttons&1!=0&&mouse&1==0&&(8..272).contains(&e.x)&&(80..336).contains(&e.y) {manager.select(manager.offset+(e.y-80) as usize/16);}
            mouse=e.buttons;
        }
    }
}
