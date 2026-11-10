#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
use xinyi::{window::{self,Draw},directory,trash,gui,widgets::{Button,Rect}};
struct Bin {ids:[usize;16],records:[Option<trash::model::Record>;16],count:usize,offset:u16,total:u32,selected:usize,volume:u32,message:&'static[u8],confirm:u8,confirm_buttons:[Button;2]}
impl Bin {
 fn refresh(&mut self){self.count=0;self.confirm=0;self.volume=directory::volume();let mut list=directory::Listing::new();let r=directory::list(b"/TRASH",self.offset,&mut list);if r == -2 {self.total=0;self.message=b"Recycle Bin is empty.";return;}if r<0 {self.message=xinyi::gui::report(2,b"Cannot read Recycle Bin.");return;}self.total=list.total;
  // Display at most 16 underlying directory entries per page, including damaged records.
  for e in &list.entries[..(list.count as usize).min(16)] {if let Some(id)=trash::model::id(&e.name[..e.length as usize]) {self.ids[self.count]=id;self.records[self.count]=trash::read(id,self.volume).ok();self.count+=1;}}
  self.selected=self.selected.min(self.count.saturating_sub(1));
 }
 fn restore(&mut self){if self.count==0{return;}let r=trash::restore(self.ids[self.selected],self.volume);self.refresh();self.message=match r {0=>xinyi::gui::report(0,b"Restored to its original folder."),1=>xinyi::gui::report(1,b"Restored; leftover record needs manual cleanup."),-17=>xinyi::gui::report(1,b"Original name exists. Move or rename that item first; nothing overwritten."),-2=>xinyi::gui::report(2,b"Original parent or recycled data is missing. Record preserved."),-16=>xinyi::gui::report(1,b"Unfinished recycle operation; preserve this record for recovery."),-116=>xinyi::gui::report(2,b"Drive changed. Refresh before restoring."),_=>xinyi::gui::report(2,b"Restore failed; recycled data has been preserved.")};}
 fn purge(&mut self){if self.count==0{return;}let r=trash::purge(self.ids[self.selected],self.volume);self.refresh();self.message=match r {0=>xinyi::gui::report(0,b"Selected item permanently deleted."),-27=>xinyi::gui::report(1,b"Folder exceeds the 32-entry deletion limit. Operation refused."),-16=>xinyi::gui::report(1,b"Unfinished recycle records are protected; preserve for recovery."),-116=>xinyi::gui::report(2,b"Drive changed. Refresh before deleting."),_=>xinyi::gui::report(2,b"Delete failed; record preserved. A marked purge can be retried.")};}
 fn paint(&self,w:u32){window::draw(w,&[Draw::rect(0,0,640,400,0xf3f6f8),Draw::rect(0,0,640,32,0x203e52),Draw::label(12,8,616,0xffffff,b"Recycle Bin"),Draw::rect(8,80,624,256,0xffffff),Draw::rect(0,344,640,1,0xc6d3dc),Draw::label(12,352,616,0,self.message),Draw::label(12,376,616,0x536c80,b"Enter: restore | Delete: forever | F5: refresh | Esc: close")]);
  for (x,width,label,primary) in [(8,144,b"Restore selected" as &[u8],true),(160,144,b"Delete forever",false),(312,88,b"Refresh",false),(408,56,b"Prev",false),(472,56,b"Next",false),(536,96,b"Close",false)] {
   gui::button_flat(w,&Button::new(Rect::new(x,42,width,28)),label,primary);
   if x==160 {gui::outline(w,Rect::new(x,42,width,28),0xa96c64);}
  }
  for i in 0..self.count {let y=84+i as u32*16;window::draw(w,&[Draw::rect(8,y,624,16,if i==self.selected {0x2b5268}else{0xffffff})]);let label=match &self.records[i] {Some(r)=>&r.path[..r.len],None=>b"Damaged record - preserved for manual recovery"};window::row(w,12,y as u32,if i==self.selected{0x2b5268}else{0xffffff},if i==self.selected {0xffffff}else {0},&label[..label.len().min(76)]);}
  if self.confirm!=0 {gui::dialog_flat(w,if self.confirm==1{b"Restore item?"}else{b"Delete forever?"},Rect::new(104,124,432,164));window::draw(w,&[Draw::paragraph(120,168,400,48,0x20384d,if self.confirm==1{b"Restore this item to its original path?"}else{b"Permanently delete this item? This cannot be undone."})]);gui::button_flat(w,&self.confirm_buttons[0],if self.confirm==1{b"Restore (Y)"}else{b"Delete (Y)"},true);gui::button_flat(w,&self.confirm_buttons[1],b"Cancel (N)",false);}window::present(w);
 }
}
fn main()->u32 {
 let handle=window::open(b"Recycle Bin");if handle<0{return 1;}let w=handle as u32;
 let mut bin=Bin{ids:[0;16],records:[None;16],count:0,offset:0,total:0,selected:0,volume:0,message:b"Select an item to restore or permanently delete.",confirm:0,confirm_buttons:[Button::new(Rect::new(120,236,144,28)),Button::new(Rect::new(280,236,144,28))]};bin.refresh();bin.paint(w);let mut buttons=0;
 loop {let mut e=window::Event::default();if window::event(w,&mut e,true)<0{return 1;}let mut action=0;
  if e.kind==window::CLOSE{return 0;}
  if e.kind==window::SCROLL&&bin.confirm==0 {bin.offset=(bin.offset as i32+(e.code as i32).clamp(-8,8)*3).clamp(0,bin.total.saturating_sub(16) as i32) as u16;bin.refresh();}if e.kind==window::KEY {let k=e.code as u16;
   if bin.confirm!=0 {match k {89|121=>{if bin.confirm==1 {bin.restore();}else{bin.purge();}},78|110|27=>bin.confirm=0,_=>()}bin.paint(w);continue;}
   match k {27=>return 0,10|82|114=>action=1,xinyi::key::DELETE=>action=5,xinyi::key::F5=>action=2,xinyi::key::PAGE_UP=>action=3,xinyi::key::PAGE_DOWN=>action=4,0x100=>bin.selected=bin.selected.saturating_sub(1),0x101=>bin.selected=(bin.selected+1).min(bin.count.saturating_sub(1)),_=>()}
  }
  if e.kind==window::RESET {buttons=e.buttons;for b in &mut bin.confirm_buttons{b.reset(e.buttons);}}
  if e.kind==window::POINTER&&bin.confirm!=0 {let mut choice=None;for(i,b)in bin.confirm_buttons.iter_mut().enumerate(){if b.pointer(e.x,e.y,e.buttons){choice=Some(i);}}match choice{Some(0)=>{if bin.confirm==1{bin.restore();}else{bin.purge();}},Some(1)=>bin.confirm=0,_=>()}buttons=e.buttons;bin.paint(w);continue;}
  if e.kind==window::POINTER {let pressed=e.buttons&!buttons;buttons=e.buttons;if pressed&1!=0 {if bin.confirm!=0 {bin.confirm=0;}else if (84..340).contains(&e.y) {let i=(e.y-84) as usize/16;if i<bin.count{bin.selected=i;}}else if (42..70).contains(&e.y) {action=match e.x {8..=152=>1,160..=304=>5,312..=400=>2,408..=464=>3,472..=528=>4,536..=639=>return 0,_=>0};}}}
  match action {1|5 if bin.count==0=>{bin.message=gui::report(0,b"Recycle Bin is empty.");},1 if bin.count>0=>{bin.confirm=1;xinyi::sounds::play(xinyi::sounds::Event::Warning);},5 if bin.count>0=>{bin.confirm=2;xinyi::sounds::play(xinyi::sounds::Event::Warning);},2=>bin.refresh(),3=>{bin.offset=bin.offset.saturating_sub(16);bin.refresh();},4 if bin.offset as u32+16<bin.total=>{bin.offset+=16;bin.refresh();},_=>()}
  bin.paint(w);
 }
}
