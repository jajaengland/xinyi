//! Bounded per-drive file-type preferences; never executable paths or permissions.
pub const MAX_BYTES:usize=256;
const HEADER:&[u8]=b"XINYI-DEFAULTS 1\n";
#[derive(Clone,Copy)]struct Mapping {ext:[u8;3],en:usize,id:[u8;8],len:usize}
const EMPTY:Mapping=Mapping{ext:[0;3],en:0,id:[0;8],len:0};
pub struct Table {items:[Mapping;16],len:usize}
pub fn extension(e:&[u8])->bool {e==b"-"||(!e.is_empty()&&e.len()<=3&&e.iter().all(|c|c.is_ascii_alphanumeric())&&!e.eq_ignore_ascii_case(b"BIN")&&!e.eq_ignore_ascii_case(b"ELF"))}
fn identity(id:&[u8])->bool {!id.is_empty()&&id.len()<=8&&id[0].is_ascii_alphabetic()&&id.iter().all(|c|c.is_ascii_alphanumeric()||*c==b'_')}
impl Table {
 pub const fn new()->Self {Self{items:[EMPTY;16],len:0}}
 pub fn get(&self,ext:&[u8])->Option<&[u8]> {let ext=if ext.is_empty(){b"-"}else{ext};self.items[..self.len].iter().find(|m|m.ext[..m.en].eq_ignore_ascii_case(ext)).map(|m|&m.id[..m.len])}
 pub fn set(&mut self,ext:&[u8],id:Option<&[u8]>)->Result<(),()> {
  if !extension(ext)||id.is_some_and(|id|!identity(id)){return Err(());}
  let found=self.items[..self.len].iter().position(|m|m.ext[..m.en].eq_ignore_ascii_case(ext));
  let Some(id)=id else {if let Some(i)=found {self.items.copy_within(i+1..self.len,i);self.len-=1;}return Ok(());};
  let i=found.unwrap_or(self.len);if i==16{return Err(());}let mut m=EMPTY;m.en=ext.len();m.ext[..m.en].copy_from_slice(ext);m.ext.make_ascii_uppercase();m.len=id.len();m.id[..m.len].copy_from_slice(id);m.id.make_ascii_uppercase();self.items[i]=m;if i==self.len{self.len+=1;}Ok(())
 }
 pub fn parse(b:&[u8])->Result<Self,()> {
  if b.len()>MAX_BYTES||!b.starts_with(HEADER)||!b.ends_with(b"\n"){return Err(());}let mut t=Self::new();
  for line in b[HEADER.len()..].split(|c|*c==b'\n').filter(|s|!s.is_empty()) {
   let at=line.iter().position(|c|*c==b'=').ok_or(())?;let ext=&line[..at];let id=&line[at+1..];
   if t.get(ext).is_some(){return Err(());}t.set(ext,Some(id))?;
  }Ok(t)
 }
 pub fn encode(&self,b:&mut[u8;MAX_BYTES])->usize {
  b[..HEADER.len()].copy_from_slice(HEADER);let mut n=HEADER.len();
  for m in &self.items[..self.len] {b[n..n+m.en].copy_from_slice(&m.ext[..m.en]);n+=m.en;b[n]=b'=';n+=1;b[n..n+m.len].copy_from_slice(&m.id[..m.len]);n+=m.len;b[n]=b'\n';n+=1;}n
 }
}
