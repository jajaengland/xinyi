//! Desktop layout and captured launcher/task controls. No hardware or allocation.
use crate::{app_catalog::Catalog,desktop_ui::Rect};
pub const TASKBAR_Y:i32=544;
pub const QUERY:usize=24;
#[derive(Clone,Copy)]
pub struct Task {pub pid:u32,pub name:[u8;8],pub length:usize}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Hit {Apps,App(usize),Shortcut(usize),Desktop(usize),DesktopFolder,Task(usize),Exit,Sound,Less,More,Mute}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Action {Launch(usize),OpenDesktop(usize),Focus(u32),Exit,Volume(u32)}
#[derive(Clone,Copy,PartialEq,Eq)]
pub struct DesktopEntry {pub name:[u8;12],pub length:usize,pub folder:bool}
impl DesktopEntry {pub const EMPTY:Self=Self{name:[0;12],length:0,folder:false};}
pub struct Shell {
    pub files:[DesktopEntry;20],pub file_count:usize,pub file_volume:u32,pub selected:Option<usize>,pub press:(i32,i32),
    pub catalog:Catalog,pub tasks:[Option<Task>;4],pub focused:Option<u32>,
    pub open:bool,pub sound_open:bool,pub capture:Option<Hit>,pub menu_selection:Option<usize>,
    pub query:[u8;QUERY],pub query_len:usize,pub volume:u32,pub clock:[u8;5],
    pub background:bool,pub x:i32,pub y:i32,buttons:u8,blocked:bool,
    pub message:[u8;88],pub length:usize,
}
impl Shell {
    pub const fn new()->Self {Self{files:[DesktopEntry::EMPTY;20],file_count:0,file_volume:0,selected:None,press:(0,0),catalog:Catalog::new(),tasks:[None;4],focused:None,open:false,
        sound_open:false,capture:None,menu_selection:None,query:[0;QUERY],query_len:0,
        volume:60,clock:*b"--:--",background:true,x:400,y:300,buttons:0,blocked:false,message:[0;88],length:0}}
    pub fn status(&mut self,text:&[u8]) {self.length=text.len().min(88);self.message[..self.length].copy_from_slice(&text[..self.length]);}
    pub fn work_area()->Rect {Rect{x:0,y:0,w:800,h:TASKBAR_Y}}
    pub fn bar()->Rect {Rect{x:0,y:TASKBAR_Y,w:800,h:56}}
    pub fn apps()->Rect {Rect{x:12,y:560,w:88,h:32}}
    pub fn task(i:usize)->Rect {Rect{x:108+i as i32*116,y:560,w:108,h:32}}
    pub fn exit()->Rect {Rect{x:724,y:560,w:64,h:32}}
    pub fn sound()->Rect {Rect{x:580,y:560,w:36,h:32}}
    pub fn sound_panel()->Rect {Rect{x:568,y:420,w:220,h:124}}
    pub fn volume_button(hit:Hit)->Rect {Rect{x:match hit {Hit::Less=>580,Hit::More=>632,_=>684},y:498,w:44,h:30}}
    pub fn shortcut(i:usize)->Rect {Rect{x:20,y:24+i as i32*104,w:96,h:88}}
    pub fn desktop_file(i:usize)->Rect {Rect{x:132+(i/5) as i32*104,y:24+(i%5) as i32*104,w:96,h:88}}
    pub fn menu(&self)->Rect {Rect{x:12,y:80,w:480,h:464}}
    pub fn card(&self,slot:usize)->Rect {let r=self.menu();Rect{x:r.x+16+(slot%2) as i32*228,y:r.y+88+(slot/2) as i32*44,w:220,h:40}}
    pub fn find(&self,name:&[u8])->Option<usize> {self.catalog.entries[..self.catalog.len].iter().position(|e|e.label[..e.label_len].eq_ignore_ascii_case(name))}
    pub fn shortcut_app(&self,i:usize)->Option<usize> {self.find([b"FILES" as &[u8],b"TERMINAL",b"SETTINGS"].get(i).copied()?)}
    pub fn filtered(&self)->([usize;16],usize) {
        let mut out=[0;16];let mut n=0;
        for (i,e) in self.catalog.entries[..self.catalog.len].iter().enumerate() {
            let query=&self.query[..self.query_len];
            let has=|text:&[u8]|query.is_empty()||text.windows(query.len()).any(|w|w.eq_ignore_ascii_case(query));
            if has(&e.label[..e.label_len])||has(e.title()) {out[n]=i;n+=1;}
        }
        (out,n)
    }
    pub fn catalog_changed(&mut self) {self.capture=None;let (items,n)=self.filtered();self.menu_selection=if n>0 {Some(items[0])}else{None};}
    fn show_apps(&mut self) {self.open=!self.open;self.sound_open=false;self.capture=None;self.query_len=0;self.catalog_changed();}
    pub fn hit(&self)->Option<Hit> {
        if Self::apps().contains(self.x,self.y) {return Some(Hit::Apps);}
        if self.open {
            let (items,n)=self.filtered();
            for (slot,&index) in items[..n].iter().enumerate() {if self.card(slot).contains(self.x,self.y) {return Some(Hit::App(index));}}
            return None;
        }
        if Self::sound().contains(self.x,self.y) {return Some(Hit::Sound);}
        if self.sound_open {
            for hit in [Hit::Less,Hit::More,Hit::Mute] {if Self::volume_button(hit).contains(self.x,self.y) {return Some(hit);}}
            return None;
        }
        if Self::exit().contains(self.x,self.y) {return Some(Hit::Exit);}
        for i in 0..4 {if self.tasks[i].is_some()&&Self::task(i).contains(self.x,self.y) {return Some(Hit::Task(i));}}
        if self.background {for i in 0..3 {if self.shortcut_app(i).is_some()&&Self::shortcut(i).contains(self.x,self.y) {return Some(Hit::Shortcut(i));}}}
        if self.background&&Self::shortcut(3).contains(self.x,self.y) {return Some(Hit::DesktopFolder);}
        if self.background {for i in 0..self.file_count {if Self::desktop_file(i).contains(self.x,self.y) {return Some(Hit::Desktop(i));}}}
        None
    }
    pub fn reset(&mut self,buttons:u8) {self.open=false;self.sound_open=false;self.capture=None;self.menu_selection=None;self.buttons=buttons;self.blocked=false;}
    pub fn next_task(&self,reverse:bool)->Option<u32> {
        let current=self.tasks.iter().position(|t|t.is_some_and(|t|Some(t.pid)==self.focused));let start=current.unwrap_or(if reverse {0}else{3});
        for offset in 1..=4 {let i=if reverse {(start+4-offset)%4}else{(start+offset)%4};if let Some(t)=self.tasks[i] {return Some(t.pid);}}None
    }
    pub fn key(&mut self,code:u32)->(bool,Option<Action>) {
        let key=code as u16;
        if key==27&&code&(1<<17)!=0&&code&(1<<18)==0 {self.show_apps();return (true,None);}
        if self.sound_open&&key==27 {self.reset(self.buttons);return (true,None);}
        if !self.open {return (false,None);}
        let (items,n)=self.filtered();
        match key {
            27=>{self.open=false;self.menu_selection=None;self.capture=None;},
            0x100..=0x105 if n>0=>{
                let at=self.menu_selection.and_then(|i|items[..n].iter().position(|v|*v==i));
                let next=match key {0x104=>0,0x105=>n-1,0x100|0x102=>at.map_or(n-1,|i|(i+n-1)%n),_=>at.map_or(0,|i|(i+1)%n)};self.menu_selection=Some(items[next]);
            }
            10=>{self.open=false;self.capture=None;return (true,self.menu_selection.take().filter(|i|items[..n].contains(i)).map(Action::Launch));},
            8=>{self.query_len=self.query_len.saturating_sub(1);self.catalog_changed();},
            32..=126 if code&((1<<17)|(1<<18))==0=>{if self.query_len<QUERY {self.query[self.query_len]=key as u8;self.query_len+=1;self.catalog_changed();}}
            _=>(),
        }(true,None)
    }
    pub fn input(&mut self,x:i32,y:i32,buttons:u8,lost:bool)->(bool,Option<Action>,bool) {
        let previous=(self.open,self.sound_open,self.capture,self.hit(),self.menu_selection);
        if (x,y)!=(self.x,self.y)||buttons!=self.buttons||lost {self.menu_selection=None;}
        self.x=x;self.y=y;let pressed=buttons&!self.buttons;let released=self.buttons&!buttons;self.buttons=buttons;
        let consumed=self.open||self.sound_open||self.capture.is_some()||self.blocked||y>=TASKBAR_Y||matches!(self.hit(),Some(Hit::Shortcut(_)|Hit::Desktop(_)|Hit::DesktopFolder));
        if lost {self.reset(buttons);return (consumed,None,true);}
        if pressed&1!=0 {self.press=(x,y);self.blocked=consumed;self.capture=self.hit();if let Some(Hit::Desktop(i))=self.capture {self.selected=Some(i);}if self.capture.is_none() {if self.open&&!self.menu().contains(x,y) {self.open=false;}if self.sound_open&&!Self::sound_panel().contains(x,y) {self.sound_open=false;}}}
        let mut action=None;
        if released&1!=0 {
            if let Some(hit)=self.capture {if Some(hit)==self.hit() {match hit {
                Hit::Apps=>self.show_apps(),Hit::App(i)=>{self.open=false;action=Some(Action::Launch(i));},
                Hit::Shortcut(i)=>action=self.shortcut_app(i).map(Action::Launch),
                Hit::DesktopFolder=>action=Some(Action::OpenDesktop(20)),
                Hit::Desktop(i)=>action=Some(Action::OpenDesktop(i)),
                Hit::Task(i)=>action=self.tasks[i].map(|t|Action::Focus(t.pid)),Hit::Exit=>action=Some(Action::Exit),
                Hit::Sound=>{self.sound_open=!self.sound_open;self.open=false;},
                Hit::Less=>{self.volume=self.volume.saturating_sub(10);action=Some(Action::Volume(self.volume));},
                Hit::More=>{self.volume=(self.volume+10).min(100);action=Some(Action::Volume(self.volume));},
                Hit::Mute=>{self.volume=if self.volume==0 {60}else{0};action=Some(Action::Volume(self.volume));},
            }}}self.capture=None;self.blocked=false;
        }(consumed,action,previous!=(self.open,self.sound_open,self.capture,self.hit(),self.menu_selection))
    }
}
