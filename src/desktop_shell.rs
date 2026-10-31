//! Desktop layout and captured launcher/task controls. No hardware or allocation.
use crate::{app_catalog::Catalog,desktop_ui::Rect};
pub const TASKBAR_Y:i32=544;
pub const QUERY:usize=24;
#[derive(Clone,Copy)]
pub struct Task {pub pid:u32,pub name:[u8;8],pub length:usize}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Hit {Apps,App(usize),Shortcut(usize),Desktop(usize),DesktopFolder,DesktopMenu(usize),Task(usize),Exit,Sound,Less,More,Mute}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Action {Launch(usize),OpenDesktop(usize),FileAction(u8),Focus(u32),Exit,Volume(u32)}
#[derive(Clone,Copy,PartialEq,Eq)]
pub struct DesktopEntry {pub name:[u8;12],pub length:usize,pub folder:bool}
impl DesktopEntry {pub const EMPTY:Self=Self{name:[0;12],length:0,folder:false};}
pub struct Shell {
    pub keyboard:bool,nav_anchor:Option<usize>,pub selection:u32,pub modifiers:u32,gesture_modifiers:u32,toggle:Option<usize>,pub context:Option<(i32,i32,Option<usize>)>,pub layout_loaded:bool,pub layout_dirty:bool,pub slots:[u8;20],pub files:[DesktopEntry;20],pub file_count:usize,pub file_volume:u32,pub selected:Option<usize>,pub press:(i32,i32),
    pub catalog:Catalog,pub tasks:[Option<Task>;4],pub focused:Option<u32>,
    pub open:bool,pub sound_open:bool,pub capture:Option<Hit>,pub menu_selection:Option<usize>,
    pub query:[u8;QUERY],pub query_len:usize,pub volume:u32,pub clock:[u8;5],
    pub background:bool,pub x:i32,pub y:i32,buttons:u8,blocked:bool,
    pub message:[u8;88],pub length:usize,
}
impl Shell {
    pub const fn new()->Self {Self{keyboard:true,nav_anchor:None,selection:0,modifiers:0,gesture_modifiers:0,toggle:None,context:None,layout_loaded:false,layout_dirty:false,slots:[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19],files:[DesktopEntry::EMPTY;20],file_count:0,file_volume:0,selected:None,press:(0,0),catalog:Catalog::new(),tasks:[None;4],focused:None,open:false,
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
    pub fn file_rect(&self,i:usize)->Rect {Self::desktop_file(self.slots[i] as usize)}
    pub fn nearest_slot(x:i32,y:i32)->usize {
        (((x-132)/104).clamp(0,3)*5+((y-24)/104).clamp(0,4)) as usize
    }
    pub fn arrange(&mut self,i:usize,slot:usize) {
        if i>=self.file_count||slot>=20{return;}
        let old=self.slots[i];
        if let Some(other)=(0..self.file_count).find(|&j|self.slots[j] as usize==slot){self.slots[other]=old;}
        self.slots[i]=slot as u8;self.selected=Some(i);self.layout_dirty=true;
    }
    pub fn select_file(&mut self,i:usize) {
        self.nav_anchor=None;
        let ctrl=self.modifiers&(1<<17)!=0;let shift=self.modifiers&(1<<16)!=0;
        self.toggle=None;
        if shift {let anchor=self.selected.unwrap_or(i);let lo=self.slots[anchor].min(self.slots[i]);let hi=self.slots[anchor].max(self.slots[i]);
            if !ctrl {self.selection=0;}for j in 0..self.file_count {if (lo..=hi).contains(&self.slots[j]) {self.selection|=1<<j;}}
        }else if ctrl {if self.selection&(1<<i)!=0 {self.toggle=Some(i);}else{self.selection|=1<<i;}self.selected=Some(i);}
        else {if self.selection&(1<<i)==0 {self.selection=1<<i;}self.selected=Some(i);}
    }
    pub fn arrange_group(&mut self,anchor:usize,slot:usize) {
        let mask=self.selection;if mask.count_ones()<=1 {self.arrange(anchor,slot);return;}
        let mut min_x=3;let mut max_x=0;let mut min_y=4;let mut max_y=0;
        for i in 0..self.file_count {if mask&(1<<i)!=0 {let x=self.slots[i] as i32/5;let y=self.slots[i] as i32%5;min_x=min_x.min(x);max_x=max_x.max(x);min_y=min_y.min(y);max_y=max_y.max(y);}}
        let dx=(slot as i32/5-self.slots[anchor] as i32/5).clamp(-min_x,3-max_x);
        let dy=(slot as i32%5-self.slots[anchor] as i32%5).clamp(-min_y,4-max_y);
        let mut old=[false;20];let mut target=[false;20];let mut next=self.slots;
        for i in 0..self.file_count {if mask&(1<<i)!=0 {old[self.slots[i] as usize]=true;next[i]=(self.slots[i] as i32+dx*5+dy) as u8;target[next[i] as usize]=true;}}
        for i in 0..self.file_count {if mask&(1<<i)==0&&target[self.slots[i] as usize] {let free=(0..20).find(|&s|old[s]&&!target[s]).unwrap();next[i]=free as u8;old[free]=false;}}
        self.slots=next;self.layout_dirty=true;
    }
    pub fn update_files(&mut self,files:[DesktopEntry;20],count:usize) {
        let mut slots=[255u8;20];let mut used=[false;20];
        for i in 0..count {if let Some(old)=(0..self.file_count).find(|&j|self.files[j]==files[i]) {let slot=self.slots[old] as usize;slots[i]=slot as u8;used[slot]=true;}}
        for i in 0..count {if slots[i]==255 {let slot=used.iter().position(|&v|!v).unwrap();slots[i]=slot as u8;used[slot]=true;}}
        self.files=files;self.file_count=count;self.slots=slots;self.selected=None;self.selection=0;self.nav_anchor=None;
    }
    pub fn encode_layout(&self,out:&mut [u8;305])->usize {
        out.fill(0);out[..4].copy_from_slice(b"XDL1");out[4]=self.file_count as u8;
        for i in 0..self.file_count {let at=5+i*15;let e=self.files[i];out[at..at+12].copy_from_slice(&e.name);out[at+12]=e.length as u8;out[at+13]=e.folder as u8;out[at+14]=self.slots[i];}
        5+self.file_count*15
    }
    pub fn apply_layout(&mut self,data:&[u8])->bool {
        if data.len()<5||&data[..4]!=b"XDL1" {return false;}
        let count=data[4] as usize;if count>20||data.len()!=5+count*15{return false;}
        let mut records=[DesktopEntry::EMPTY;20];let mut saved=[0u8;20];let mut seen=[false;20];
        for i in 0..count {let at=5+i*15;let n=data[at+12] as usize;let slot=data[at+14] as usize;
            if !(1..=12).contains(&n)||data[at+13]>1||slot>=20||seen[slot]||!data[at..at+n].iter().all(|b|b.is_ascii_graphic()&&!b"/\\:".contains(b))||data[at+n..at+12].iter().any(|&b|b!=0){return false;}
            let mut name=[0;12];name.copy_from_slice(&data[at..at+12]);let entry=DesktopEntry{name,length:n,folder:data[at+13]!=0};
            if records[..i].iter().any(|e|e.name==entry.name){return false;}records[i]=entry;saved[i]=slot as u8;seen[slot]=true;
        }
        let mut slots=[255u8;20];let mut used=[false;20];
        for i in 0..self.file_count {if let Some(j)=records[..count].iter().position(|e|*e==self.files[i]){slots[i]=saved[j];used[saved[j] as usize]=true;}}
        for i in 0..self.file_count {if slots[i]==255 {let j=used.iter().position(|v|!*v).unwrap();slots[i]=j as u8;used[j]=true;}}
        self.slots=slots;true
    }
    pub fn menu(&self)->Rect {Rect{x:12,y:80,w:480,h:464}}
    pub fn card(&self,slot:usize)->Rect {let r=self.menu();Rect{x:r.x+16+(slot%2) as i32*228,y:r.y+88+(slot/2) as i32*44,w:220,h:40}}
    pub fn find(&self,name:&[u8])->Option<usize> {self.catalog.entries[..self.catalog.len].iter().position(|e|e.label[..e.label_len].eq_ignore_ascii_case(name))}
    pub fn shortcut_app(&self,i:usize)->Option<usize> {if i==4 {return self.find(b"TRASH");}self.find([b"FILES" as &[u8],b"TERMINAL",b"SETTINGS"].get(i).copied()?)}
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
    pub fn context_rect(&self)->Option<Rect> {self.context.map(|(x,y,_)|Rect{x,y,w:176,h:if self.context.unwrap().2.is_some(){150}else{90}})}
    pub fn hit(&self)->Option<Hit> {
        if let Some(r)=self.context_rect() {if r.contains(self.x,self.y){return Some(Hit::DesktopMenu(((self.y-r.y)/30) as usize));}return None;}

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
        if self.background {for i in [0,1,2,4] {if self.shortcut_app(i).is_some()&&Self::shortcut(i).contains(self.x,self.y) {return Some(Hit::Shortcut(i));}}}
        if self.background&&Self::shortcut(3).contains(self.x,self.y) {return Some(Hit::DesktopFolder);}
        if self.background {for i in 0..self.file_count {if self.file_rect(i).contains(self.x,self.y) {return Some(Hit::Desktop(i));}}}
        None
    }
    pub fn reset(&mut self,buttons:u8) {self.toggle=None;self.context=None;self.open=false;self.sound_open=false;self.capture=None;self.menu_selection=None;self.buttons=buttons;self.blocked=false;}
    pub fn next_task(&self,reverse:bool)->Option<u32> {
        let current=self.tasks.iter().position(|t|t.is_some_and(|t|Some(t.pid)==self.focused));let start=current.unwrap_or(if reverse {0}else{3});
        for offset in 1..=4 {let i=if reverse {(start+4-offset)%4}else{(start+offset)%4};if let Some(t)=self.tasks[i] {return Some(t.pid);}}None
    }
    pub fn key(&mut self,code:u32)->(bool,Option<Action>) {
        let key=code as u16;
        if key==27&&code&(1<<17)!=0&&code&(1<<18)==0 {self.show_apps();return (true,None);}
        if self.context.is_some() {if key==27 {self.context=None;self.capture=None;}return (true,None);}
        if self.sound_open&&key==27 {self.reset(self.buttons);return (true,None);}
        if !self.open {
            if !self.keyboard {return (false,None);}
            if key==27 {if self.selection==0&&self.selected.is_none(){return (false,None);}self.selection=0;self.selected=None;self.nav_anchor=None;return (true,None);}
            if key==0x106 {return (true,Some(Action::FileAction(3)));}
            if code&(1<<17)!=0 {match key {67|99=>return (true,Some(Action::FileAction(0))),88|120=>return (true,Some(Action::FileAction(1))),86|118=>return (true,Some(Action::FileAction(2))),_=>()}}
            if matches!(key,65|97)&&code&(1<<17)!=0 {self.selection=(1u32<<self.file_count)-1;self.selected=self.selected.or_else(||if self.file_count>0 {Some(0)}else{None});return (true,None);}
            if key==10 {return (true,self.selected.filter(|&i|self.selection==1<<i).map(Action::OpenDesktop));}
            if key==32 {if let Some(i)=self.selected {self.selection^=1<<i;}return (true,None);}
            if (0x100..=0x105).contains(&key)&&self.file_count>0 {
                let current=self.selected.unwrap_or_else(||(0..self.file_count).min_by_key(|&i|self.slots[i]).unwrap());
                let at=self.slots[current] as i32;let x=at/5;let y=at%5;
                let next=if key==0x104 {(0..self.file_count).min_by_key(|&i|self.slots[i]).unwrap()}else if key==0x105 {(0..self.file_count).max_by_key(|&i|self.slots[i]).unwrap()}else if self.selected.is_none(){current}else{
                    (0..self.file_count).filter_map(|i|{let dx=self.slots[i] as i32/5-x;let dy=self.slots[i] as i32%5-y;
                        let (forward,side)=match key {0x100=>(-dy,dx.abs()),0x101=>(dy,dx.abs()),0x102=>(-dx,dy.abs()),_=>(dx,dy.abs())};
                        if forward>0 {Some((side*100+forward,i))}else{None}
                    }).min().map_or(current,|(_,i)|i)
                };
                if code&(1<<16)!=0 {let anchor=*self.nav_anchor.get_or_insert(current);let lo=self.slots[anchor].min(self.slots[next]);let hi=self.slots[anchor].max(self.slots[next]);if code&(1<<17)==0 {self.selection=0;}for i in 0..self.file_count {if (lo..=hi).contains(&self.slots[i]){self.selection|=1<<i;}}}
                else {self.nav_anchor=None;if code&(1<<17)==0 {self.selection=1<<next;}}
                self.selected=Some(next);return (true,None);
            }
            return (false,None);
        }
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
        let previous=(self.open,self.sound_open,self.capture,self.hit(),self.menu_selection,self.context,self.selection);
        if (x,y)!=(self.x,self.y)||buttons!=self.buttons||lost {self.menu_selection=None;}
        self.x=x;self.y=y;let pressed=buttons&!self.buttons;let released=self.buttons&!buttons;self.buttons=buttons;
        let consumed=self.context.is_some()||self.open||self.sound_open||self.capture.is_some()||self.blocked||y>=TASKBAR_Y||matches!(self.hit(),Some(Hit::Shortcut(_)|Hit::Desktop(_)|Hit::DesktopFolder));
        if lost {self.reset(buttons);return (consumed,None,true);}
        if pressed&2!=0&&y<TASKBAR_Y&&self.background&&!self.open&&!self.sound_open {
            let item=match self.hit(){Some(Hit::Desktop(i))=>Some(i),_=>None};
            self.context=Some((x.clamp(0,624),y.clamp(0,544-if item.is_some(){150}else{90}),item));self.capture=None;self.selected=item;if let Some(i)=item {if self.selection&(1<<i)==0 {self.selection=1<<i;}}else{self.selection=0;}return (true,None,true);
        }
        if pressed&1!=0&&self.context.is_some()&&self.hit().is_none() {self.context=None;self.capture=None;self.blocked=true;return (true,None,true);}
        if pressed&1!=0 {self.gesture_modifiers=self.modifiers;self.press=(x,y);self.blocked=consumed;self.capture=self.hit();if let Some(Hit::Desktop(i))=self.capture {self.select_file(i);}else if self.capture.is_none()&&self.background {self.selection=0;self.selected=None;}if self.capture.is_none() {if self.open&&!self.menu().contains(x,y) {self.open=false;}if self.sound_open&&!Self::sound_panel().contains(x,y) {self.sound_open=false;}}}
        let mut action=None;
        if released&1!=0 {
            if let Some(hit)=self.capture {if Some(hit)==self.hit() {match hit {
                Hit::Apps=>self.show_apps(),Hit::App(i)=>{self.open=false;action=Some(Action::Launch(i));},
                Hit::Shortcut(i)=>action=self.shortcut_app(i).map(Action::Launch),
                Hit::DesktopMenu(row)=>{let item=self.context.and_then(|(_,_,i)|i);self.context=None;action=Some(if row==0 {Action::OpenDesktop(item.unwrap_or(20))}else if row==1 {Action::OpenDesktop(21)}else{Action::FileAction(if item.is_none(){2}else if row==2 {0}else if row==3 {1}else{3})});},
                Hit::DesktopFolder=>action=Some(Action::OpenDesktop(20)),
                Hit::Desktop(i)=>{if let Some(j)=self.toggle.take(){self.selection^=1<<j;}if self.gesture_modifiers&((1<<16)|(1<<17))==0&&self.selection.count_ones()==1 {action=Some(Action::OpenDesktop(i));}},
                Hit::Task(i)=>action=self.tasks[i].map(|t|Action::Focus(t.pid)),Hit::Exit=>action=Some(Action::Exit),
                Hit::Sound=>{self.sound_open=!self.sound_open;self.open=false;},
                Hit::Less=>{self.volume=self.volume.saturating_sub(10);action=Some(Action::Volume(self.volume));},
                Hit::More=>{self.volume=(self.volume+10).min(100);action=Some(Action::Volume(self.volume));},
                Hit::Mute=>{self.volume=if self.volume==0 {60}else{0};action=Some(Action::Volume(self.volume));},
            }}}self.capture=None;self.blocked=false;
        }(consumed,action,previous!=(self.open,self.sound_open,self.capture,self.hit(),self.menu_selection,self.context,self.selection))
    }
}
