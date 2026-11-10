//! Bounded wallpaper preference and integer image placement; shared with Settings.
pub const MAX_PATH:usize=105;
pub const HEADER:&[u8]=b"XINYI-WALL 1\n";
#[repr(C)]#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Request {pub path:[u8;105],pub mode:u8,pub length:u16,pub volume:u32}
impl Request {
 pub const fn new()->Self{Self{path:[0;105],mode:0,length:0,volume:0}}
 pub fn valid(&self)->bool {self.mode<=2&&(self.length as usize)<=MAX_PATH&&valid_path(&self.path[..self.length as usize])}
 pub fn encode(&self,out:&mut[u8;128])->usize{out[..HEADER.len()].copy_from_slice(HEADER);let mut n=HEADER.len();out[n]=b'0'+self.mode;out[n+1]=b'\n';n+=2;out[n..n+self.length as usize].copy_from_slice(&self.path[..self.length as usize]);n+=self.length as usize;out[n]=b'\n';n+1}
 pub fn parse(b:&[u8])->Option<Self>{let mut lines=b.strip_prefix(HEADER)?.split(|b|*b==b'\n');let mode=lines.next()?;if mode.len()!=1||!(b'0'..=b'2').contains(&mode[0]){return None;}let path=lines.next()?;if path.len()>MAX_PATH||lines.next()?!=b""||lines.next().is_some(){return None;}let mut r=Self::new();r.mode=mode[0]-b'0';r.length=path.len() as u16;r.path[..path.len()].copy_from_slice(path);r.valid().then_some(r)}
}
pub fn valid_path(path:&[u8])->bool {
 if path.is_empty(){return true;}if path[0]!=b'/'||path.len()>105{return false;}let mut depth=0;
 for name in path[1..].split(|b|*b==b'/'){depth+=1;if depth>8{return false;}let mut parts=name.split(|b|*b==b'.');let base=parts.next().unwrap();let ext=parts.next();let chars=|p:&[u8]|p.iter().all(|b|b.is_ascii_alphanumeric()||matches!(*b,b'_'|b'-'));if base.is_empty()||base.len()>8||!chars(base)||ext.is_some_and(|s|s.is_empty()||s.len()>3||!chars(s))||parts.next().is_some(){return false;}}
 true
}
#[derive(Clone,Copy,Debug)]pub struct Placement{pub x:u32,pub y:u32,pub width:u32,pub height:u32,sx:u32,sy:u32,sw:u32,sh:u32}
impl Placement{
 pub fn new(sw:u32,sh:u32,dw:u32,dh:u32,mode:u8)->Option<Self>{if sw==0||sh==0||dw==0||dh==0||sw>2048||sh>2048||dw>1280||dh>1024||mode>2{return None;}let(mut w,mut h,mut cropw,mut croph)=(dw,dh,sw,sh);
  if mode==0{if sw as u64*dh as u64>sh as u64*dw as u64{h=((sh as u64*dw as u64/sw as u64) as u32).max(1);}else{w=((sw as u64*dh as u64/sh as u64) as u32).max(1);}}
  else if mode==1{if sw as u64*dh as u64>sh as u64*dw as u64{cropw=((sh as u64*dw as u64/dh as u64) as u32).max(1);}else{croph=((sw as u64*dh as u64/dw as u64) as u32).max(1);}}
  else{w=sw.min(dw);h=sh.min(dh);cropw=w;croph=h;}
  Some(Self{x:(dw-w)/2,y:(dh-h)/2,width:w,height:h,sx:(sw-cropw)/2,sy:(sh-croph)/2,sw:cropw,sh:croph})
 }
 pub fn source(&self,x:u32,y:u32)->(u32,u32){(self.sx+((x as u64*2+1)*self.sw as u64/(self.width as u64*2)) as u32,self.sy+((y as u64*2+1)*self.sh as u64/(self.height as u64*2)) as u32)}
}
