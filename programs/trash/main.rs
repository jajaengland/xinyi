#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
use xinyi::{window::{self,Draw},directory,trash};
struct Bin {ids:[usize;16],records:[Option<trash::model::Record>;16],count:usize,offset:u16,total:u32,selected:usize,volume:u32,message:&'static[u8],confirm:u8}
impl Bin {
 fn refresh(&mut self){self.count=0;self.confirm=0;self.volume=directory::volume();let mut list=directory::Listing::new();let r=directory::list(b"/TRASH",self.offset,&mut list);if r == -2 {self.total=0;self.message=b"Recycle Bin is empty.";return;}if r<0 {self.message=b"Cannot read Recycle Bin.";return;}self.total=list.total;
  // Display at most 16 underlying directory entries per page, including damaged records.
  for e in &list.entries[..(list.count as usize).min(16)] {if let Some(id)=trash::model::id(&e.name[..e.length as usize]) {self.ids[self.count]=id;self.records[self.count]=trash::read(id,self.volume).ok();self.count+=1;}}
  self.selected=self.selected.min(self.count.saturating_sub(1));
 }
 fn restore(&mut self){if self.count==0{return;}let r=trash::restore(self.ids[self.selected],self.volume);self.refresh();self.message=match r {0=>b"Restored to its original folder.",1=>b"Restored; leftover record needs manual cleanup.",-17=>b"Original name exists. Move or rename that item first; nothing overwritten.",-2=>b"Original parent or recycled data is missing. Record preserved.",-16=>b"Unfinished recycle operation; preserve this record for recovery.",-116=>b"Drive changed. Refresh before restoring.",_=>b"Restore failed; recycled data has been preserved."};}
 fn purge(&mut self){if self.count==0{return;}let r=trash::purge(self.ids[self.selected],self.volume);self.refresh();self.message=match r {0=>b"Selected item permanently deleted.",-27=>b"Folder exceeds the 32-entry deletion limit. Operation refused.",-16=>b"Unfinished recycle records are protected; preserve for recovery.",-116=>b"Drive changed. Refresh before deleting.",_=>b"Delete failed; record preserved. A marked purge can be retried."};}
 fn paint(&self,w:u32){window::draw(w,&[Draw::rect(0,0,640,400,0xf1f5f8),Draw::label(12,12,616,0,b"Recycle Bin - restore to original location"),Draw::rect(8,42,144,28,0xd0e3f0),Draw::label(18,48,124,0,b"Restore selected"),Draw::rect(160,42,144,28,0xf0d4d0),Draw::label(170,48,124,0,b"Delete forever"),Draw::rect(312,42,88,28,0xd0e3f0),Draw::label(320,48,76,0,b"Refresh"),Draw::label(410,48,222,0,b"Prev   Next   Close"),Draw::label(12,352,616,0,self.message),Draw::label(12,376,616,0x536c80,b"Enter: restore | Delete: forever | F5: refresh | Esc: close")]);
  for i in 0..self.count {let y=84+i as u32*16;window::draw(w,&[Draw::rect(8,y,624,16,if i==self.selected {0xc4def0}else{0xffffff})]);let label=match &self.records[i] {Some(r)=>&r.path[..r.len],None=>b"Damaged record - preserved for manual recovery"};window::row(w,12,y as u32,if i==self.selected{0xc4def0}else{0xffffff},0,&label[..label.len().min(76)]);}
  if self.confirm!=0 {window::draw(w,&[Draw::rect(8,340,624,32,0xffe6bd),Draw::label(14,348,612,0,if self.confirm==1 {b"Restore selected item to its original path? Y confirms / N cancels" as &[u8]}else{b"Permanently delete selected item? Y confirms. Cannot undo. N cancels"})]);}window::present(w);
 }
}
fn main()->u32 {
 let handle=window::open(b"Recycle Bin");if handle<0{return 1;}let w=handle as u32;
 let mut bin=Bin{ids:[0;16],records:[None;16],count:0,offset:0,total:0,selected:0,volume:0,message:b"Select an item to restore or permanently delete.",confirm:0};bin.refresh();bin.paint(w);let mut buttons=0;
 loop {let mut e=window::Event::default();if window::event(w,&mut e,true)<0{return 1;}let mut action=0;
  if e.kind==window::CLOSE{return 0;}
  if e.kind==window::KEY {let k=e.code as u16;
   if bin.confirm!=0 {match k {89|121=>{if bin.confirm==1 {bin.restore();}else{bin.purge();}},78|110|27=>bin.confirm=0,_=>()}bin.paint(w);continue;}
   match k {27=>return 0,10|82|114=>action=1,xinyi::key::DELETE=>action=5,xinyi::key::F5=>action=2,xinyi::key::PAGE_UP=>action=3,xinyi::key::PAGE_DOWN=>action=4,0x100=>bin.selected=bin.selected.saturating_sub(1),0x101=>bin.selected=(bin.selected+1).min(bin.count.saturating_sub(1)),_=>()}
  }
  if e.kind==window::RESET {buttons=e.buttons;}
  if e.kind==window::POINTER {let pressed=e.buttons&!buttons;buttons=e.buttons;if pressed&1!=0 {if bin.confirm!=0 {bin.confirm=0;}else if (84..340).contains(&e.y) {let i=(e.y-84) as usize/16;if i<bin.count{bin.selected=i;}}else if (42..70).contains(&e.y) {action=match e.x {8..=152=>1,160..=304=>5,312..=400=>2,408..=464=>3,472..=528=>4,536..=639=>return 0,_=>0};}}}
  match action {1 if bin.count>0=>bin.confirm=1,5 if bin.count>0=>bin.confirm=2,2=>bin.refresh(),3=>{bin.offset=bin.offset.saturating_sub(16);bin.refresh();},4 if bin.offset as u32+16<bin.total=>{bin.offset+=16;bin.refresh();},_=>()}
  bin.paint(w);
 }
}
