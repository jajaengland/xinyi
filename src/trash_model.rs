//! Bounded recycle records shared with userspace and host checks.
#[derive(Clone,Copy)]
pub struct Record {pub path:[u8;105],pub len:usize,pub folder:bool,pub ready:bool,pub purging:bool}
pub fn valid_path(path:&[u8])->bool {
 if path.len()<2||path.len()>105||path[0]!=b'/' {return false;}
 let mut count=0;for part in path[1..].split(|b|*b==b'/') {
  count+=1;if count>8||part.is_empty()||part==b"."||part==b".."||!part.iter().all(|b|b.is_ascii_graphic()&&!b"\\:*?\"<>|".contains(b)){return false;}
  let mut pieces=part.split(|b|*b==b'.');let base=pieces.next().unwrap();if base.is_empty()||base.len()>8{return false;}
  if let Some(ext)=pieces.next(){if ext.is_empty()||ext.len()>3||pieces.next().is_some(){return false;}}
 }
 !path[1..].split(|b|*b==b'/').next().unwrap().eq_ignore_ascii_case(b"TRASH")
}
impl Record {
 pub fn new(path:&[u8],folder:bool)->Option<Self> {if !valid_path(path){return None;}let mut r=Self{path:[0;105],len:path.len(),folder,ready:false,purging:false};r.path[..path.len()].copy_from_slice(path);Some(r)}
 pub fn encode(&self,out:&mut[u8;112])->usize {out.fill(0);out[..4].copy_from_slice(b"XTR1");out[4]=if self.purging {2}else{self.ready as u8};out[5]=self.folder as u8;out[6]=self.len as u8;out[7..7+self.len].copy_from_slice(&self.path[..self.len]);7+self.len}
 pub fn parse(data:&[u8])->Option<Self> {if data.len()<7||&data[..4]!=b"XTR1"||data[4]>2||data[5]>1||data.len()!=7+data[6] as usize{return None;}let mut r=Self::new(&data[7..],data[5]!=0)?;r.ready=data[4]==1;r.purging=data[4]==2;Some(r)}
}
pub fn id(name:&[u8])->Option<usize> {if name.len()!=8||name[0]!=b'T'{return None;}let mut v=0;for b in &name[1..] {if !b.is_ascii_digit(){return None;}v=v*10+(b-b'0') as usize;}if v<1024{Some(v)}else{None}}
pub fn entry(index:usize)->[u8;15] {let mut p=*b"/TRASH/T0000000";let mut v=index;for i in (8..15).rev(){p[i]=b'0'+(v%10) as u8;v/=10;}p}

#[derive(Clone,Copy)]
pub struct Node {pub path:[u8;105],pub len:usize,pub folder:bool}
impl Node {pub const EMPTY:Self=Self{path:[0;105],len:0,folder:false};}
pub struct Plan {pub nodes:[Node;32],pub count:usize}
impl Plan {
 pub fn new(root:&[u8],folder:bool)->Self {let mut p=Self{nodes:[Node::EMPTY;32],count:1};p.nodes[0].path[..root.len()].copy_from_slice(root);p.nodes[0].len=root.len();p.nodes[0].folder=folder;p}
 pub fn append(&mut self,parent:usize,name:&[u8],folder:bool)->Result<(),i32> {
  if parent>=self.count||!self.nodes[parent].folder||name.is_empty()||name.len()>12||name==b"."||name==b".."||!name.iter().all(|b|b.is_ascii_graphic()&&!b"/\\:".contains(b)){return Err(-22);}
  let base=self.nodes[parent];let n=base.len+1+name.len();if self.count==32||n>105||base.path[..base.len].iter().filter(|&&b|b==b'/').count()>=8{return Err(-27);}
  let mut node=Node::EMPTY;node.path[..base.len].copy_from_slice(&base.path[..base.len]);node.path[base.len]=b'/';node.path[base.len+1..n].copy_from_slice(name);node.len=n;node.folder=folder;
  if self.nodes[..self.count].iter().any(|old|old.len==n&&old.path[..n]==node.path[..n]){return Err(-22);}
  self.nodes[self.count]=node;self.count+=1;Ok(())
 }
}
