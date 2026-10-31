#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
use xinyi::{directory as d,files as f,trash as t};
fn check(ok:bool){if !ok{xinyi::write(b"Recycle checks FAILED!\n");xinyi::exit(1);}}
fn saved(p:&[u8],data:&[u8],v:u32){check(f::save_on(p,data,v)>=0);}
fn main()->u32 {
 let mut args=[0;128];let argc=xinyi::arguments(&mut args);if argc==4&&&args[..4]==b"clip" {let mut clip=xinyi::file_clipboard::Selection::new();check(xinyi::file_clipboard::get(&mut clip)==0);check(clip.count==2&&clip.cut==0&&clip.version==1);xinyi::write(b"Desktop recycle clipboard preserved!\n");return 0;}
 let v=d::volume();
 if argc==5&&&args[..5]==b"purge" {
  check(d::action_on(b"/PCHECK",0,v)==0);saved(b"/PCHECK/A.TXT",b"delete me",v);check(t::recycle(b"/PCHECK/A.TXT",false,v)==0);check(t::purge(0,v.wrapping_add(1))==-116);check(t::purge(0,v)==0);check(t::read(0,v).is_err());
  check(d::action_on(b"/PCHECK/DIR",0,v)==0);check(d::action_on(b"/PCHECK/DIR/SUB",0,v)==0);saved(b"/PCHECK/DIR/SUB/A.TXT",b"nested",v);check(t::recycle(b"/PCHECK/DIR",true,v)==0);check(t::purge(0,v)==0);check(t::read(0,v).is_err());
  // An unfinished recycle record cannot be purged. An interrupted purge can resume.
  check(d::action_on(b"/TRASH/T0000000",0,v)==0);let mut r=t::model::Record::new(b"/PCHECK/HELD.TXT",false).unwrap();let mut data=[0;112];let n=r.encode(&mut data);saved(b"/TRASH/T0000000/INFO.DAT",&data[..n],v);saved(b"/TRASH/T0000000/ITEM",b"held",v);check(t::purge(0,v)==-16);r.purging=true;let n=r.encode(&mut data);saved(b"/TRASH/T0000000/INFO.DAT",&data[..n],v);check(t::restore(0,v)==-16);check(t::purge(0,v)==0);
  check(d::action_on(b"/TRASH/T0000000",0,v)==0);let n=r.encode(&mut data);saved(b"/TRASH/T0000000/INFO.DAT",&data[..n],v);check(t::purge(0,v)==0);
  // Oversized plans must not change a ready record or delete any payload.
  check(d::action_on(b"/PCHECK/BIG",0,v)==0);for i in 0..32 {let path=[b'/',b'P',b'C',b'H',b'E',b'C',b'K',b'/',b'B',b'I',b'G',b'/',b'A'+(i/26) as u8,b'A'+(i%26) as u8];saved(&path,b"x",v);}
  check(t::recycle(b"/PCHECK/BIG",true,v)==0);check(t::purge(0,v)==-27);check(t::read(0,v).unwrap().ready);check(t::restore(0,v)==0);
  xinyi::write(b"Purge checks passed!\n");return 0;
 }
check(d::action_on(b"/RCHECK",0,v)>=0);saved(b"/RCHECK/A.TXT",b"original",v);
 check(t::recycle(b"/RCHECK/A.TXT",false,v)==0);let r=t::read(0,v).unwrap();check(r.ready&&&r.path[..r.len]==b"/RCHECK/A.TXT");let mut data=[0;112];check(f::read_on(b"/RCHECK/A.TXT",&mut data,v)==-2);check(f::read_on(b"/TRASH/T0000000/ITEM",&mut data,v)==8);
 saved(b"/RCHECK/A.TXT",b"collision",v);check(t::restore(0,v)==-17);check(f::read_on(b"/RCHECK/A.TXT",&mut data,v)==9&&&data[..9]==b"collision");check(d::action_on(b"/RCHECK/A.TXT",1,v)==0);check(t::restore(0,v)==0);check(f::read_on(b"/RCHECK/A.TXT",&mut data,v)==8&&&data[..8]==b"original");
 check(d::action_on(b"/RCHECK/DIR",0,v)==0);saved(b"/RCHECK/DIR/INNER.TXT",b"nested",v);check(t::recycle(b"/RCHECK/DIR",true,v)==0);check(t::read(0,v).unwrap().folder);check(t::restore(0,v)==0);check(f::read_on(b"/RCHECK/DIR/INNER.TXT",&mut data,v)==6);
 check(t::recycle(b"/RCHECK/A.TXT",false,v.wrapping_add(1))==-116);check(t::recycle(b"/TRASH/A",false,v)==-22);
 check(d::action_on(b"/TRASH/T0000000",0,v)==0);let r=t::model::Record::new(b"/RCHECK/HELD.TXT",false).unwrap();let n=r.encode(&mut data);saved(b"/TRASH/T0000000/INFO.DAT",&data[..n],v);saved(b"/TRASH/T0000000/ITEM",b"held",v);check(t::restore(0,v)==-16);check(f::read_on(b"/RCHECK/HELD.TXT",&mut data,v)==-2);saved(b"/TRASH/T0000000/INFO.DAT",b"corrupt",v);check(t::restore(0,v)==-22);check(f::read_on(b"/TRASH/T0000000/ITEM",&mut data,v)==4);
 check(d::action_on(b"/TRASH/T0000000/ITEM",1,v)==0);check(d::action_on(b"/TRASH/T0000000/INFO.DAT",1,v)==0);check(d::action_on(b"/TRASH/T0000000",2,v)==0);
 xinyi::write(b"Recycle checks passed!\n");0
}
