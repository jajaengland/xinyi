#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
use alloc::vec::Vec;
#[path="../../src/app_package.rs"]mod app_package;
mod package;
mod access;
use xinyi::app_manifest;
#[path="../../sdk/xinyi.rs"]mod xinyi;
#[path="../files/app_review.rs"]mod review;
use xinyi::{directory,window::{self,Draw},widgets::{Button,Rect,TextInput},gui,key,app_manifest::Manifest};
#[global_allocator]static HEAP:xinyi::heap::Allocator=xinyi::heap::Allocator::new();
#[alloc_error_handler]fn oom(_:core::alloc::Layout)->! {xinyi::exit(12)}
#[derive(Clone,Copy)]
struct Entry {id:[u8;8],len:usize,metadata:Option<Manifest>}
impl Entry {fn id(&self)->&[u8] {&self.id[..self.len]}fn title(&self)->&[u8] {self.metadata.as_ref().map_or(self.id(),|m|m.title())}}
struct Manager {entries:Vec<Entry>,selected:usize,offset:usize,volume:u32,status:&'static[u8],buttons:[Button;7],confirm:[Button;2],pending:Option<Entry>, extra:[Button;4], modal:[Button;2], mode:u8, operation:u16, source:TextInput<105>, candidate:Option<Manifest>, backup:[u8;8], backup_len:usize,archive:Option<Vec<u8>>,defaults_button:Button,preferences:Option<xinyi::associations::defaults::Table>,access_button:Button,access_choices:[Button;4],saved_access:Option<access::model::Restriction>}
impl Manager {
    fn refresh(&mut self) {
        self.pending=None;self.mode=0;self.archive=None;self.backup_len=0;self.entries.clear();self.volume=directory::volume();self.selected=0;self.offset=0;
        if directory::cd(b"/APPS")<0 {self.status=xinyi::gui::report(2,b"No /APPS folder on this drive.");return;}
        if self.entries.try_reserve(64).is_err() {self.status=xinyi::gui::report(2,b"Not enough memory to read installed apps.");return;}
        let size=xinyi::files::stream_size_on(b"/APPS/_UPD.ID",self.volume);
        if size>0&&size<=8&&xinyi::files::read_at_on(b"/APPS/_UPD.ID",0,&mut self.backup[..size as usize],self.volume)==size {self.backup_len=size as usize;}
        let mut offset=0u16;let mut listing=directory::Listing::new();
        loop {
            let result=directory::list(b".",offset,&mut listing);
            if result<0||self.volume!=directory::volume() {self.entries.clear();self.status=xinyi::gui::report(2,b"Drive changed or applications could not be read. Refresh to retry.");return;}
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
        let Some(entry)=self.entries.get(self.selected).copied() else {self.status=xinyi::gui::report(1,b"No application selected.");return false;};
        if self.volume!=directory::volume() {self.status=xinyi::gui::report(2,b"Drive changed. Refresh before managing applications.");return false;}
        if index==2 {
            if entry.id().eq_ignore_ascii_case(b"APPMGR") {self.status=xinyi::gui::report(2,b"Applications cannot remove its own running bundle.");return false;}
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
        self.status=if xinyi::launch_on(&command[..length],self.volume)<0 {xinyi::gui::report(2,b"Could not launch. Close another app or refresh the list.")}else{b"Launch requested."};false
    }
    fn remove(&mut self) {
        let Some(entry)=self.pending.take() else{return;};
        let result=directory::app_action(entry.id(),1,self.volume);
        if result==0 {self.refresh();self.status=xinyi::gui::report(0,b"App removed. Your documents were kept.");}
        else {self.status=match result {-16=>xinyi::gui::report(1,b"App or folder is in use. Close it before removing the bundle."),-116=>xinyi::gui::report(2,b"Drive changed. Refresh before trying again."),_=>xinyi::gui::report(2,b"Removal failed. Open the app folder to inspect the bundle.")};}
    }
    fn permissions(&mut self) {
        let Some(e)=self.entries.get(self.selected) else{return;};
        if e.id().eq_ignore_ascii_case(b"APPMGR"){self.status=xinyi::gui::report(1,b"Applications keeps its management access so you can restore other apps.");return;}
        if e.metadata.is_none(){self.status=xinyi::gui::report(1,b"Valid metadata is required before changing permissions.");return;}
        match access::load(self.volume){Ok(t)=>self.saved_access=t.get(e.id()),Err(_)=>{self.status=xinyi::gui::report(2,b"Could not read /ACCESS.CFG. Inspect it using the console.");return;}}
        self.mode=4;for b in &mut self.access_choices{b.reset(1);}self.status=b"Choose a restriction. Close and reopen the app for it to take effect.";
    }
    fn save_access(&mut self,index:usize) {
        use access::model::Restriction::*;
        let value=[Some(ReadOnly),Some(Media),Some(Isolated),None][index];
        let Some(e)=self.entries.get(self.selected) else{return;};
        let metadata=match review::load(e.id(),self.volume){Ok(m)=>m,Err(_)=>{self.status=xinyi::gui::report(2,b"App metadata changed or drive unavailable. Refresh before changing access.");return;}};
        let mask=match metadata.access{app_manifest::Access::Trusted=>63,app_manifest::Access::ReadOnly=>1,app_manifest::Access::Media=>33,app_manifest::Access::Isolated=>0};
        if value.is_some_and(|v|v.mask()&!mask!=0){self.status=xinyi::gui::report(1,b"This choice exceeds the app's declared access. Choose a narrower profile.");return;}
        let result=access::save(e.id(),value,self.volume);
        if result<0 {self.status=xinyi::gui::report(2,b"Could not save access restrictions. Check drive and /ACCESS.CFG.");return;}
        self.saved_access=value;self.mode=0;self.status=xinyi::gui::report(0,b"Access saved. Close and reopen the app to apply it.");
    }
    fn defaults(&mut self) {
        if self.volume!=directory::volume(){self.status=xinyi::gui::report(2,b"Drive changed. Refresh before choosing defaults.");return;}
        if self.entries.get(self.selected).and_then(|e|e.metadata).is_none(){self.status=xinyi::gui::report(1,b"Choose an application with valid metadata first.");return;}
        self.preferences=match xinyi::associations::load_defaults(self.volume){Ok(t)=>Some(t),Err(_)=>{self.status=xinyi::gui::report(2,b"Could not read /DEFAULTS.CFG. Inspect the configuration in Files.");return;}};
        self.source.set(b"TXT");self.mode=3;self.archive=None;for b in &mut self.modal {b.reset(1);}
        self.status=b"Enter a file extension, or - for no extension. Escape cancels.";
    }
    fn save_default(&mut self,reset:bool) {
        let ext=&self.source.bytes[..self.source.len];
        if !xinyi::associations::defaults::extension(ext){self.status=xinyi::gui::report(2,b"Use 1-3 letters or digits, or -. BIN and ELF cannot be reassigned.");return;}
        let Some(entry)=self.entries.get(self.selected) else{return;};
        if !reset&&!entry.metadata.is_some_and(|m|m.handles(if ext==b"-"{b""}else{ext})) {self.status=xinyi::gui::report(1,b"This application does not declare support for that file type.");return;}
        let result=xinyi::associations::set_default(ext,if reset{None}else{Some(entry.id())},self.volume);
        if result<0 {self.status=if result==-116{xinyi::gui::report(2,b"Drive changed. Refresh before choosing defaults.")}else{xinyi::gui::report(2,b"Could not save defaults. Configuration may be full, invalid or read-only.")};return;}
        self.mode=0;self.preferences=None;self.status=if reset{xinyi::gui::report(0,b"Preference reset. Manifest-based handler selection restored.")}else{xinyi::gui::report(0,b"Default application saved for this file type on this drive.")};
    }
    fn begin(&mut self,index:usize) {
        if self.volume!=directory::volume() {self.status=xinyi::gui::report(2,b"Drive changed. Refresh before managing applications.");return;}
        self.operation=[0,2,3,4][index];self.candidate=None;self.archive=None;
        if index<2 {
            self.source.set(b"/");self.mode=1;
            self.status=b"Enter an absolute bundle folder or .XAP file. Enter reviews it.";
        } else {
            if self.backup_len==0 {self.status=xinyi::gui::report(1,b"No retained update backup on this drive.");return;}
            let mut path=[0;14];path[..6].copy_from_slice(b"/APPS/");path[6..6+self.backup_len].copy_from_slice(&self.backup[..self.backup_len]);
            self.source.set(&path[..6+self.backup_len]);self.mode=2;
            self.status=b"Close the backed-up app before restoring or discarding its backup.";
        }
        for b in &mut self.modal {b.reset(1);}
    }
    fn review_source(&mut self) {
        let path=&self.source.bytes[..self.source.len];
        if path.first()!=Some(&b'/')||path.last()==Some(&b'/')||path.iter().any(|b|*b==b'\\') {self.status=xinyi::gui::report(1,b"Use an absolute folder path, such as /DOWNLOAD/HELLO.");return;}
        self.archive=None;
        let mut package_id=[0;8];let mut package_len=0;
        let result=if path.len()>=4&&path[path.len()-4..].eq_ignore_ascii_case(b".XAP") {
            match package::load(path,self.volume) {
                Ok(bytes)=>{let p=app_package::Package::parse(&bytes).unwrap();package_len=p.id.len();package_id[..package_len].copy_from_slice(p.id);let metadata=p.manifest;self.archive=Some(bytes);Ok(metadata)},
                Err(code)=>Err(code),
            }
        } else {review::load_folder(path,path.rsplit(|b|*b==b'/').next().unwrap_or(b""),self.volume)};
        let id=if package_len!=0 {&package_id[..package_len]}else{path.rsplit(|b|*b==b'/').next().unwrap_or(b"")};
        match result {
            Ok(metadata)=>{
                if self.operation==2 {
                    let Some(old)=self.entries.iter().find(|e|e.id().eq_ignore_ascii_case(id)).and_then(|e|e.metadata) else {self.status=xinyi::gui::report(1,b"Update requires an installed app with valid metadata.");return;};
                    if !xinyi::app_manifest::permits_update(&old,&metadata) {self.status=xinyi::gui::report(1,b"Update must have a newer version without additional access.");return;}
                }
                self.candidate=Some(metadata);self.mode=2;self.status=if self.archive.is_some() {b"Review this package snapshot, then confirm. Escape cancels."}else{b"Review the bundle, then confirm. Escape cancels."};
            },Err(code)=>self.status=Self::failure(code),
        }
    }
    fn failure(code:i32)->&'static[u8] {match code {
        -74=>xinyi::gui::report(2,b"Corrupt or unsupported package. Nothing was installed."),
        -7=>xinyi::gui::report(1,b"Package exceeds the initial 768 KiB limit or is too short."),
        -12=>xinyi::gui::report(2,b"Not enough memory to review this package."),
        -117=>xinyi::gui::report(2,b"Extraction cleanup failed. Check installed apps and /PKGIN before retrying."),
        -16=>xinyi::gui::report(1,b"App or folder is in use. Close it and try again."),
        -116=>xinyi::gui::report(2,b"Drive changed. Refresh before trying again."),
        -17=>xinyi::gui::report(1,b"Bundle or backup already exists. Resolve it before retrying."),
        -8=>xinyi::gui::report(2,b"Invalid bundle, version or access. Check its executable and manifest."),
        -2=>xinyi::gui::report(2,b"Bundle, manifest or retained backup was not found."),
        -28=>xinyi::gui::report(2,b"Drive or application catalog is full."),
        -125=>xinyi::gui::report(1,b"Operation cancelled. Inspect the bundle before retrying."),
        -22=>xinyi::gui::report(2,b"Invalid folder or backup owner. Use a bundle outside /APPS."),
        _=>xinyi::gui::report(2,b"Operation failed. Inspect the source and /APPS recovery folders."),
    }}
    fn execute(&mut self,w:u32) {
        if self.operation<=2&&self.archive.is_none() {
            let approved=self.candidate;self.mode=1;
            self.review_source();
            if self.mode!=2||self.candidate!=approved {self.status=xinyi::gui::report(2,b"Bundle changed or failed review. Review it again before confirming.");self.mode=1;return;}
        }
        self.status=b"Working... copying and verifying files. Please wait.";self.mode=0;self.paint(w);
        let result=if let Some(bytes)=self.archive.take() {match package::install(&bytes,self.operation,self.volume) {Ok(())=>0,Err(code)=>code}}else {directory::app_action(&self.source.bytes[..self.source.len],self.operation,self.volume)};
        if result==0 {let operation=self.operation;self.refresh();self.status=match operation {0=>xinyi::gui::report(0,b"Application installed."),2=>xinyi::gui::report(0,b"Application updated. Previous version retained for rollback."),3=>xinyi::gui::report(0,b"Previous application version restored."),_=>xinyi::gui::report(0,b"Backup discarded. Current version kept.")};}
        else {self.status=Self::failure(result);}
    }
    fn paint(&self,w:u32) {
        window::draw(w,&[Draw::rect(0,0,640,400,0xf3f6f8),Draw::rect(0,0,640,32,0x203e52),Draw::label(12,8,616,0xffffff,b"Applications - installed bundles"),Draw::rect(8,76,264,264,0xffffff),Draw::rect(288,76,344,272,0xffffff),Draw::rect(0,372,640,1,0xc6d3dc),Draw::label(12,380,616,0x20384d,self.status)]);
        for (i,label) in [b"Launch" as &[u8],b"Open folder",b"Remove",b"Refresh",b"Close",b"Previous",b"Next"].iter().enumerate() {gui::button_flat(w,&self.buttons[i],label,i==0);}
        for (i,label) in [b"Install" as &[u8],b"Update",b"Restore",b"Discard backup"].iter().enumerate() {gui::button_flat(w,&self.extra[i],label,false);}
        gui::button_flat(w,&self.defaults_button,b"Defaults",false);
        for (slot,entry) in self.entries.iter().skip(self.offset).take(16).enumerate() {
            let y=80+slot as u32*16;let selected=self.selected==self.offset+slot;
            window::draw(w,&[Draw::rect(8,y,264,16,if selected {0x2b5268}else{0xffffff}),Draw::label(16,y,248,if selected {0xffffff}else{0x20384d},entry.title())]);
        }
        if let Some(entry)=self.entries.get(self.selected) {
            window::draw(w,&[Draw::label(300,88,320,0x20384d,entry.title()),Draw::text(300,112,0,entry.id())]);
            if let Some(metadata)=entry.metadata {
                let mut digits=[b'0';10];let mut version=metadata.version;for n in digits.iter_mut().rev(){*n+=(version%10) as u8;version/=10;}
                let start=digits.iter().position(|&b|b!=b'0').unwrap_or(9);let(profile,permissions)=review::profile(metadata.access);
                window::draw(w,&[Draw::text(300,136,0,b"Version:"),Draw::text(380,136,0,if metadata.version==0 {b"Legacy"}else{&digits[start..]}),Draw::label(300,164,320,0x20384d,profile),Draw::paragraph(300,188,320,64,0x20384d,permissions),Draw::paragraph(300,260,320,24,0x536c80,metadata.description())]);
            } else {window::draw(w,&[Draw::paragraph(300,148,320,64,0x536c80,b"Metadata is missing or invalid. This bundle can still be inspected or removed.")]);}
            window::draw(w,&[Draw::paragraph(300,312,320,32,0x536c80,b"Profiles limit services; they are not a complete security sandbox.")]);
        }
        gui::button_flat(w,&self.access_button,b"Permissions",false);
        if let Some(entry)=self.pending {
            gui::dialog_flat(w,b"Remove application?",Rect::new(104,132,432,136));
            window::draw(w,&[Draw::label(120,172,400,0x20384d,entry.title()),Draw::label(120,196,400,0x536c80,b"Deletes this app bundle. Your documents remain.")]);
            gui::button_flat(w,&self.confirm[0],b"Remove",true);gui::button_flat(w,&self.confirm[1],b"Cancel",false);
        }
        if self.mode==4 {
            gui::dialog_flat(w,b"Application permissions",Rect::new(40,76,560,284));
            if let Some(e)=self.entries.get(self.selected){window::draw(w,&[Draw::label(56,116,528,0x20384d,e.title()),Draw::text(56,148,0,b"Saved:"),Draw::label(136,148,448,0x20384d,access::label(self.saved_access)),Draw::text(56,176,0,b"Declared:"),Draw::label(136,176,448,0x20384d,e.metadata.map_or(b"Unknown",|m|review::profile(m.access).0)),Draw::label(56,204,528,0x536c80,b"Applies at next launch. Escape cancels.")]);}
            for (i,label) in [b"Read-only" as &[u8],b"Media",b"Isolated",b"Declared"].iter().enumerate(){gui::button_flat(w,&self.access_choices[i],label,false);}
            window::draw(w,&[Draw::paragraph(56,280,528,32,0x536c80,b"Read-only: files. Media: files and sound. Isolated: neither."),Draw::paragraph(56,312,528,32,0x536c80,b"Declared clears your restriction. Profiles are not a complete sandbox.")]);
        } else if self.mode!=0 {
            gui::dialog_flat(w,if self.mode==3 {b"Choose default application"}else if self.operation==0 {b"Install application"}else if self.operation==2 {b"Update application"}else {b"Retained update backup"},Rect::new(40,76,560,284));
            if self.mode==1||self.mode==3 {
                window::draw(w,&[Draw::label(56,120,528,0x20384d,if self.mode==3{b"File extension (1-3 characters), or - for no extension:"}else{b"Bundle folder or package file (for example /HELLO.XAP):"})]);
                if self.mode==3 {
                    if let Some(entry)=self.entries.get(self.selected) {window::draw(w,&[Draw::label(56,196,528,0x20384d,entry.title())]);}
                    let current=self.preferences.as_ref().and_then(|t|t.get(&self.source.bytes[..self.source.len]));
                    window::draw(w,&[Draw::text(56,224,0,b"Current:"),Draw::text(136,224,0,current.unwrap_or(b"Automatic")),Draw::paragraph(56,256,528,32,0x536c80,b"Set chooses this app. Reset restores automatic matching. Escape cancels.")]);
                }
                gui::text_input(w,Rect::new(56,148,528,28),&self.source,true);
            } else {
                window::draw(w,&[Draw::paragraph(56,116,528,32,0x20384d,&self.source.bytes[..self.source.len.min(96)])]);
                if let Some(m)=self.candidate {
                    let (profile,permissions)=review::profile(m.access);
                    let mut digits=[b'0';10];let mut v=m.version;for d in digits.iter_mut().rev(){*d+=(v%10) as u8;v/=10;}
                    window::draw(w,&[Draw::label(56,152,528,0x20384d,m.title()),Draw::text(56,176,0,b"New version:"),Draw::text(168,176,0,&digits),Draw::label(56,200,528,0x20384d,profile),Draw::paragraph(56,224,528,48,0x20384d,permissions)]);
                    if self.operation==2 {
                        let id=self.archive.as_ref().map_or_else(||self.source.bytes[..self.source.len].rsplit(|b|*b==b'/').next().unwrap_or(b""),|p|{let end=p[8..16].iter().position(|c|*c==0).unwrap_or(8);&p[8..8+end]});
                        if let Some(old)=self.entries.iter().find(|e|e.id().eq_ignore_ascii_case(id)).and_then(|e|e.metadata) {
                            let mut digits=[b'0';10];let mut v=old.version;for d in digits.iter_mut().rev(){*d+=(v%10) as u8;v/=10;}
                            window::draw(w,&[Draw::label(56,280,144,0x536c80,b"Installed version:"),Draw::text(200,280,0,&digits),Draw::label(304,280,280,0x536c80,review::profile(old.access).0)]);
                        }
                    }
                } else {window::draw(w,&[Draw::paragraph(56,160,528,80,0x20384d,if self.operation==3 {b"Restore the previous version and remove the current bundle. Documents outside the bundle remain."}else {b"Permanently discard the previous version. The currently installed application remains."})]);}
            }
            gui::button_flat(w,&self.modal[0],if self.mode==3 {b"Set default"}else if self.mode==1 {b"Review"}else {b"Confirm"},true);gui::button_flat(w,&self.modal[1],if self.mode==3{b"Reset type"}else{b"Cancel"},false);
        }
        window::present(w);
    }
}
fn main()->u32 {
    let w=window::open(b"Applications");if w<0{return 1;}let w=w as u32;
    let mut manager=Manager{entries:Vec::new(),selected:0,offset:0,volume:directory::volume(),status:b"Loading installed applications...",pending:None,mode:0,operation:0,source:TextInput::new(),candidate:None,backup:[0;8],backup_len:0,archive:None,preferences:None,saved_access:None,access_button:Button::new(Rect::new(300,288,160,24)),access_choices:[Button::new(Rect::new(56,232,112,28)),Button::new(Rect::new(184,232,112,28)),Button::new(Rect::new(312,232,112,28)),Button::new(Rect::new(440,232,112,28))],defaults_button:Button::new(Rect::new(520,40,112,28)),
        extra:[Button::new(Rect::new(280,344,72,24)),Button::new(Rect::new(356,344,72,24)),Button::new(Rect::new(432,344,72,24)),Button::new(Rect::new(508,344,124,24))],
        modal:[Button::new(Rect::new(56,316,128,28)),Button::new(Rect::new(200,316,128,28))],
        buttons:[Button::new(Rect::new(8,40,88,28)),Button::new(Rect::new(104,40,112,28)),Button::new(Rect::new(224,40,96,28)),Button::new(Rect::new(328,40,96,28)),Button::new(Rect::new(432,40,80,28)),Button::new(Rect::new(8,344,120,24)),Button::new(Rect::new(144,344,120,24))],
        confirm:[Button::new(Rect::new(128,228,112,28)),Button::new(Rect::new(264,228,112,28))]};
    manager.paint(w);manager.refresh();let mut mouse=0;
    loop {
        manager.paint(w);let mut e=window::Event::default();if window::event(w,&mut e,true)<0||e.kind==window::CLOSE{return 0;}
        if e.kind==window::RESET {mouse=e.buttons;for b in manager.buttons.iter_mut().chain(manager.confirm.iter_mut()).chain(manager.extra.iter_mut()).chain(manager.modal.iter_mut()){b.reset(e.buttons);}manager.defaults_button.reset(e.buttons);manager.access_button.reset(e.buttons);for b in &mut manager.access_choices{b.reset(e.buttons);}continue;}
        if manager.mode==4 {
            if e.kind==window::KEY {match e.code as u16 {27=>manager.mode=0,49=>manager.save_access(0),50=>manager.save_access(1),51=>manager.save_access(2),48=>manager.save_access(3),_=>()}}
            if e.kind==window::POINTER {let mut choice=None;for(i,b)in manager.access_choices.iter_mut().enumerate(){if b.pointer(e.x,e.y,e.buttons){choice=Some(i);}}if let Some(i)=choice{manager.save_access(i);}mouse=e.buttons;}
            continue;
        }
        if manager.mode!=0 {
            let mut accept=false;
            if e.kind==window::KEY {
                match e.code as u16 {27=>manager.mode=0,10=>accept=true,_=>{if manager.mode==1||manager.mode==3 {gui::edit_key(&mut manager.source,e.code);}}}
            }
            if e.kind==window::POINTER {
                let mut clicked=None;for (i,b) in manager.modal.iter_mut().enumerate() {if b.pointer(e.x,e.y,e.buttons){clicked=Some(i);}}
                match clicked {Some(0)=>accept=true,Some(1)=>{if manager.mode==3{manager.save_default(true);}else{manager.mode=0;}},_=>()}mouse=e.buttons;
            }
            if accept {if manager.mode==3 {manager.save_default(false);}else if manager.mode==1 {manager.review_source();}else if manager.mode==2 {manager.execute(w);}}
            if manager.mode==0 {manager.archive=None;}
            continue;
        }
        if manager.pending.is_some() {
            if e.kind==window::KEY {match e.code as u16 {89|121=>manager.remove(),78|110|27=>manager.pending=None,_=>()}}
            if e.kind==window::POINTER {let mut action=None;for(i,b)in manager.confirm.iter_mut().enumerate(){if b.pointer(e.x,e.y,e.buttons){action=Some(i);}}match action {Some(0)=>manager.remove(),Some(1)=>manager.pending=None,_=>()}mouse=e.buttons;}
            continue;
        }
        if e.kind==window::SCROLL {manager.select((manager.selected as i32+(e.code as i32).clamp(-8,8)*3).max(0) as usize);}
        if e.kind==window::KEY {
            let code=e.code as u16;match code {
                27=>return 0,10=>{manager.action(0);},key::DELETE=>{manager.action(2);},key::F5=>manager.refresh(),
                key::UP=>manager.select(manager.selected.saturating_sub(1)),key::DOWN=>manager.select(manager.selected+1),
                key::HOME=>manager.select(0),key::END=>manager.select(manager.entries.len().saturating_sub(1)),
                key::PAGE_UP=>{manager.action(5);},key::PAGE_DOWN=>{manager.action(6);},_=>(),
            }
            if e.code&key::CTRL!=0&&matches!(code,80|112){manager.permissions();}
            if e.code&key::CTRL!=0&&matches!(code,68|100){manager.defaults();}
            if e.code&key::CTRL!=0&&matches!(code,81|113){return 0;}
        }
        if e.kind==window::POINTER {
            if manager.access_button.pointer(e.x,e.y,e.buttons){manager.permissions();mouse=e.buttons;continue;}
            if manager.defaults_button.pointer(e.x,e.y,e.buttons){manager.defaults();mouse=e.buttons;continue;}
            let mut action=None;for(i,b)in manager.buttons.iter_mut().enumerate(){if b.pointer(e.x,e.y,e.buttons){action=Some(i);}}
            if let Some(i)=action {if manager.action(i){return 0;}}
            let mut extra=None;for(i,b)in manager.extra.iter_mut().enumerate(){if b.pointer(e.x,e.y,e.buttons){extra=Some(i);}}
            if let Some(i)=extra {manager.begin(i);}
            if e.buttons&1!=0&&mouse&1==0&&(8..272).contains(&e.x)&&(80..336).contains(&e.y) {manager.select(manager.offset+(e.y-80) as usize/16);}
            mouse=e.buttons;
        }
    }
}
