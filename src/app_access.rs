//! Saved bundle restrictions. This table only removes authority at the next launch.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Restriction {ReadOnly,Media,Isolated}
impl Restriction {pub fn mask(self)->u32 {match self {Self::ReadOnly=>1,Self::Media=>33,Self::Isolated=>0}}}
const HEADER:&[u8]=b"XINYI-ACCESS 1\n";
pub const MAX_BYTES:usize=1024;
#[derive(Clone,Copy)]struct Item {id:[u8;8],len:usize,value:Restriction}
const EMPTY:Item=Item{id:[0;8],len:0,value:Restriction::Isolated};
pub struct Table {items:[Item;64],len:usize}
fn valid(id:&[u8])->bool {!id.is_empty()&&id.len()<=8&&id[0].is_ascii_alphabetic()&&id.iter().all(|c|c.is_ascii_alphanumeric()||*c==b'_')}
impl Table {
 pub const fn new()->Self {Self{items:[EMPTY;64],len:0}}
 pub fn get(&self,id:&[u8])->Option<Restriction> {self.items[..self.len].iter().find(|m|m.id[..m.len].eq_ignore_ascii_case(id)).map(|m|m.value)}
 pub fn set(&mut self,id:&[u8],value:Option<Restriction>)->Result<(),()> {
  if !valid(id){return Err(());}let found=self.items[..self.len].iter().position(|m|m.id[..m.len].eq_ignore_ascii_case(id));
  let Some(value)=value else {if let Some(i)=found{self.items.copy_within(i+1..self.len,i);self.len-=1;}return Ok(());};
  let i=found.unwrap_or(self.len);if i==64{return Err(());}let mut item=EMPTY;item.id[..id.len()].copy_from_slice(id);item.id.make_ascii_uppercase();item.len=id.len();item.value=value;self.items[i]=item;if i==self.len{self.len+=1;}Ok(())
 }
 pub fn parse(b:&[u8])->Result<Self,()> {
  if b.len()>MAX_BYTES||!b.starts_with(HEADER)||!b.ends_with(b"\n"){return Err(());}let mut t=Self::new();
  for line in b[HEADER.len()..].split(|c|*c==b'\n').filter(|l|!l.is_empty()) {
   let at=line.iter().position(|c|*c==b'=').ok_or(())?;let id=&line[..at];
   let value=match &line[at+1..]{b"R"=>Restriction::ReadOnly,b"M"=>Restriction::Media,b"I"=>Restriction::Isolated,_=>return Err(())};
   if t.get(id).is_some(){return Err(());}t.set(id,Some(value))?;
  }Ok(t)
 }
 pub fn encode(&self,b:&mut[u8;MAX_BYTES])->usize {
  b[..HEADER.len()].copy_from_slice(HEADER);let mut n=HEADER.len();
  for m in &self.items[..self.len]{b[n..n+m.len].copy_from_slice(&m.id[..m.len]);n+=m.len;b[n]=b'=';b[n+1]=match m.value{Restriction::ReadOnly=>b'R',Restriction::Media=>b'M',Restriction::Isolated=>b'I'};b[n+2]=b'\n';n+=3;}n
 }
}
