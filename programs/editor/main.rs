#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
mod model;
use model::Document;
use xinyi::key;
#[derive(Clone,Copy)]enum Action{Quit,New,Open}
#[derive(Clone,Copy)]enum Mode{Edit,Name(bool),Confirm(Action),Overwrite}
struct Editor {
    doc:Document,name:[u8;105],name_len:usize,prompt:[u8;105],prompt_len:usize,
    mode:Mode,after_save:Option<Action>,top:usize,message:&'static [u8],
}
impl Editor {
    fn new()->Self {Self{doc:Document::new(),name:[0;105],name_len:0,prompt:[0;105],prompt_len:0,mode:Mode::Edit,after_save:None,top:0,message:b"Ready - files up to 1024 bytes"}}
    fn action(&mut self,action:Action)->bool {
        match action {Action::Quit=>return true,Action::New=>{self.doc=Document::new();self.name_len=0;self.top=0;self.message=b"New file";self.mode=Mode::Edit;},
            Action::Open=>{self.mode=Mode::Name(false);self.prompt_len=0;self.message=b"Open file: Enter accepts, Esc cancels";}}
        false
    }
    fn request(&mut self,action:Action)->bool {
        if self.doc.dirty(){self.mode=Mode::Confirm(action);self.message=b"Unsaved changes: S save, D discard, C cancel";false}else{self.action(action)}
    }
    fn save(&mut self,name:[u8;105],length:usize)->bool {
        if length==0 {self.mode=Mode::Name(true);self.prompt_len=0;self.message=b"Save as: enter a filename (ASCII 8.3 paths)";return false;}
        let result=xinyi::save_file(&name[..length],&self.doc.data[..self.doc.length]);
        if result<0 {self.message=match result{-13=>b"Save failed: file is read-only",-28=>b"Save failed: disk full",-19=>b"Save failed: no mounted drive",-22=>b"Save failed: invalid filename",_=>b"Save failed: disk error; document retained"};self.mode=Mode::Edit;self.after_save=None;return false;}
        self.name=name;self.name_len=length;self.doc.mark_saved();self.message=b"Saved to disk";self.mode=Mode::Edit;
        if let Some(action)=self.after_save.take(){self.action(action)}else{false}
    }
    fn open(&mut self,name:[u8;105],length:usize)->bool {
        let mut data=[0;1024];let result=xinyi::read_file(&name[..length],&mut data);
        if result>=0 {if !self.doc.load(&data[..result as usize]) {self.message=b"Cannot open: unsupported text or expanded size over 1024";return false;}}
        else if result==-2 {self.doc=Document::new();self.message=b"New file - save to create it";}
        else {self.message=if result==-27 {b"Cannot open: file exceeds 1024 bytes"}else{b"Cannot open file; current document retained"};return false;}
        self.name=name;self.name_len=length;self.top=0;self.mode=Mode::Edit;
        if result>=0 {self.message=b"Opened (CRLF normalized, tabs expanded)";}true
    }
    fn event(&mut self,event:u32)->bool {
        let code=event as u16;
        match self.mode {
            Mode::Confirm(action)=>match code {
                115|83=>{self.after_save=Some(action);return self.save(self.name,self.name_len);}
                100|68=>return self.action(action),
                99|67|27=>{self.mode=Mode::Edit;self.message=b"Cancelled";}
                _=>(),
            },
            Mode::Overwrite=>match code {
                121|89=>return self.save(self.prompt,self.prompt_len),
                110|78|27=>{self.mode=Mode::Edit;self.after_save=None;self.message=b"Save cancelled";}
                _=>(),
            },
            Mode::Name(save)=>match code {
                27=>{self.mode=Mode::Edit;self.after_save=None;self.message=b"Cancelled";}
                8=>self.prompt_len=self.prompt_len.saturating_sub(1),
                10 if self.prompt_len!=0=>{
                    if save {
                        let mut data=[0;1024];let exists=xinyi::read_file(&self.prompt[..self.prompt_len],&mut data);
                        if (exists>=0||exists==-27)&&(self.prompt_len!=self.name_len||self.prompt[..self.prompt_len]!=self.name[..self.name_len]) {
                            self.mode=Mode::Overwrite;self.message=b"Replace existing file? Y yes / N cancel";
                        }else if exists>=0||exists==-2 {return self.save(self.prompt,self.prompt_len);}
                        else {self.message=b"Cannot check destination; save cancelled";}
                    }else{self.open(self.prompt,self.prompt_len);self.mode=Mode::Edit;}
                }
                32..=126 if self.prompt_len<105&&event&(key::CTRL|key::ALT)==0=>{self.prompt[self.prompt_len]=code as u8;self.prompt_len+=1;}
                _=>(),
            },
            Mode::Edit=>{
                if event&key::CTRL!=0 {
                    match code {
                        115|83=>{if event&key::SHIFT!=0 {self.mode=Mode::Name(true);self.prompt_len=0;self.after_save=None;self.message=b"Save as: enter a filename";}else{return self.save(self.name,self.name_len);}}
                        113|81=>return self.request(Action::Quit),
                        111|79=>return self.request(Action::Open),
                        110|78=>return self.request(Action::New),
                        122|90|121|89=>self.doc.undo(),
                        key::HOME=>self.doc.cursor=0,key::END=>self.doc.cursor=self.doc.length,
                        _=>(),
                    }
                }else if event&key::ALT==0 {
                    match code {
                        27=>return self.request(Action::Quit),
                        key::LEFT=>self.doc.cursor=self.doc.cursor.saturating_sub(1),
                        key::RIGHT=>self.doc.cursor=(self.doc.cursor+1).min(self.doc.length),
                        key::UP=>self.doc.vertical(-1),key::DOWN=>self.doc.vertical(1),
                        key::PAGE_UP=>self.doc.vertical(-22),key::PAGE_DOWN=>self.doc.vertical(22),
                        key::HOME=>{let(row,_)=self.doc.position(self.doc.cursor);self.doc.cursor=self.doc.offset(row,0);}
                        key::END=>{let(row,_)=self.doc.position(self.doc.cursor);self.doc.cursor=self.doc.offset(row,80);}
                        8=>self.doc.delete(true),key::DELETE=>self.doc.delete(false),
                        9=>{if !self.doc.insert(b"    "){self.message=b"File full: maximum 1024 bytes";}},
                        10=>{if !self.doc.insert(b"\n"){self.message=b"File full: maximum 1024 bytes";}},
                        32..=126=>{if !self.doc.insert(&[code as u8]) {self.message=b"File full: maximum 1024 bytes";}},
                        _=>(),
                    }
                }
            }
        }false
    }
    fn draw(&mut self) {
        let(row,column)=self.doc.position(self.doc.cursor);
        if row<self.top {self.top=row;}if row>=self.top+22 {self.top=row-21;}
        let mut header=[b' ';80];header[..13].copy_from_slice(b" Xinyi Editor");
        let name=if self.name_len==0 {b"Untitled" as &[u8]}else{&self.name[..self.name_len]};
        let length=name.len().min(57);header[15..15+length].copy_from_slice(&name[..length]);if self.doc.dirty(){header[77]=b'*';}
        xinyi::draw_text(0,0,0x70,&header);
        for y in 0..22 {xinyi::draw_text(0,y as u8+1,0x07,&self.doc.screen_row(self.top+y));}
        let mut status=[b' ';80];let message=self.message.len().min(80);status[..message].copy_from_slice(&self.message[..message]);
        xinyi::draw_text(0,23,0x70,&status);
        xinyi::draw_text(0,24,0x70,b"^S Save  ^Shift-S Save As  ^O Open  ^N New  ^Z/^Y Undo/Redo  ^Q/Esc Quit        ");
        if matches!(self.mode,Mode::Name(_)|Mode::Overwrite) {
            let mut text=[b' ';80];let start=self.prompt_len.saturating_sub(78);let shown=self.prompt_len-start;
            text[..shown].copy_from_slice(&self.prompt[start..self.prompt_len]);xinyi::draw_text(0,22,0x1f,&text);xinyi::move_cursor(shown as u16,22);
        }else{xinyi::move_cursor(column as u16,(row-self.top+1) as u16);}
    }
}
fn main()->u32 {
    let mut editor=Editor::new();let mut args=[0;128];let length=xinyi::arguments(&mut args);
    if length<0||length>105 {xinyi::print("Invalid editor filename.\n");return 1;}
    if length>0 {let mut name=[0;105];name[..length as usize].copy_from_slice(&args[..length as usize]);
        if !editor.open(name,length as usize) {xinyi::write(editor.message);xinyi::print("\n");return 1;}}
    xinyi::clear_screen(0x07);xinyi::show_cursor(true);
    loop {editor.draw();let event=xinyi::read_key(true);if event>0&&editor.event(event as u32){return 0;}}
}
