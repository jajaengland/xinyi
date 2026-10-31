//! Reversible deletion through existing volume-tagged filesystem services.
use super::{directory,files};
#[path="../src/trash_model.rs"]pub mod model;
fn member(index:usize,suffix:&[u8],out:&mut[u8;32])->usize {out[..15].copy_from_slice(&model::entry(index));out[15..15+suffix.len()].copy_from_slice(suffix);15+suffix.len()}
pub fn read(index:usize,volume:u32)->Result<model::Record,i32> {
 if index>=1024{return Err(-22);}
 let mut path=[0;32];let n=member(index,b"/INFO.DAT",&mut path);let mut data=[0;112];let n=files::read_on(&path[..n],&mut data,volume);if n<0{return Err(n);}model::Record::parse(&data[..n as usize]).ok_or(-22)
}
pub fn recycle(path:&[u8],folder:bool,volume:u32)->i32 {
 let Some(mut record)=model::Record::new(path,folder) else{return -22;};
 let result=directory::action_on(b"/TRASH",0,volume);if result<0&&result != -17{return result;}
 let mut used=[false;1024];let mut list=directory::Listing::new();let mut offset=0;
 loop {if directory::volume()!=volume{return -116;}let r=directory::list(b"/TRASH",offset,&mut list);if r<0{return r;}if list.total>1024{return -28;}for e in &list.entries[..list.count as usize] {if let Some(i)=model::id(&e.name[..e.length as usize]){used[i]=true;}}offset+=list.count as u16;if offset as u32>=list.total{break;}if list.count==0{return -5;}}
 for i in 0..1024 {if used[i]{continue;}let root=model::entry(i);let r=directory::action_on(&root,0,volume);if r == -17 {continue;}if r<0{return r;}
  let mut meta=[0;32];let n=member(i,b"/INFO.DAT",&mut meta);let mut bytes=[0;112];let len=record.encode(&mut bytes);let r=files::save_on(&meta[..n],&bytes[..len],volume);if r<0{return r;}
  let mut dest=[0;32];let len=member(i,b"/ITEM",&mut dest);let r=directory::transfer_on(path,&dest[..len],2,volume);if r<0{return r;}
  record.ready=true;let len=record.encode(&mut bytes);let r=files::save_on(&meta[..n],&bytes[..len],volume);return if r<0 {r}else{0};
 } -28
}
pub fn restore(index:usize,volume:u32)->i32 {
 let record=match read(index,volume){Ok(r) if r.ready=>r,Ok(_)=>return -16,Err(e)=>return e};
 let mut source=[0;32];let n=member(index,b"/ITEM",&mut source);let r=directory::transfer_on(&source[..n],&record.path[..record.len],2,volume);if r<0{return r;}
 let n=member(index,b"/INFO.DAT",&mut source);if directory::action_on(&source[..n],1,volume)<0{return 1;}
 if directory::action_on(&model::entry(index),2,volume)<0 {1}else{0}
}

/// Permanently remove one completed record. A marked interrupted purge may be retried.
/// Preflight refuses more than 32 payload entries before changing metadata or data.
pub fn purge(index:usize,volume:u32)->i32 {
 let mut record=match read(index,volume){Ok(r) if r.ready||r.purging=>r,Ok(_)=>return -16,Err(e)=>return e};
 if directory::volume()!=volume{return -116;}
 let root=model::entry(index);let mut listing=directory::Listing::new();let r=directory::list(&root,0,&mut listing);if r<0{return r;}
 if listing.total>2{return -22;}let mut present=false;
 for e in &listing.entries[..listing.count as usize] {let name=&e.name[..e.length as usize];if name==b"ITEM" {if (e.flags&1!=0)!=record.folder{return -22;}present=true;}else if name!=b"INFO.DAT" {return -22;}}
 if !present&&!record.purging{return -2;}
 let mut payload=[0;32];let n=member(index,b"/ITEM",&mut payload);let mut plan=model::Plan::new(&payload[..n],record.folder);if !present{plan.count=0;}
 let mut parent=0;
 while parent<plan.count {
  let node=plan.nodes[parent];if node.folder {let mut offset=0u16;
   loop {if directory::volume()!=volume{return -116;}let r=directory::list(&node.path[..node.len],offset,&mut listing);if r<0{return r;}if listing.total>32{return -27;}
    for e in &listing.entries[..listing.count as usize] {if let Err(e)=plan.append(parent,&e.name[..e.length as usize],e.flags&1!=0){return e;}}
    offset+=listing.count as u16;if offset as u32>=listing.total{break;}if listing.count==0{return -5;}
   }
  }parent+=1;
 }
 // Publishing this marker before deletion blocks restoration of partially purged data.
 record.ready=false;record.purging=true;let mut bytes=[0;112];let len=record.encode(&mut bytes);let mut meta=[0;32];let n=member(index,b"/INFO.DAT",&mut meta);let r=files::save_on(&meta[..n],&bytes[..len],volume);if r<0{return r;}
 for i in (0..plan.count).rev(){let node=plan.nodes[i];let r=directory::action_on(&node.path[..node.len],if node.folder{2}else{1},volume);if r<0{return r;}}
 let r=directory::action_on(&meta[..n],1,volume);if r<0{return r;}directory::action_on(&root,2,volume)
}
