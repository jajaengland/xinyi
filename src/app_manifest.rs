//! Bounded, allocation-free application metadata. Metadata can only restrict access.
pub const MAX_BYTES:usize=512;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Access {Trusted,ReadOnly,Media,Isolated}
impl Access {
    pub fn prefix(self)-> &'static [u8] {match self {Self::Trusted=>b"",Self::ReadOnly=>b"--read-only ",Self::Media=>b"--media ",Self::Isolated=>b"--isolated "}}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error {Size,Header,Field,Duplicate,Value,Executable}
#[derive(Clone,Copy)]
pub struct Manifest {
    pub executable:[u8;12],pub executable_len:usize,
    title:[u8;32],title_len:usize,description:[u8;64],description_len:usize,
    pub icon:u8,pub access:Access,types:[[u8;3];8],lengths:[u8;8],count:usize,
}
pub fn executable(name:&[u8])->bool {
    let Some(dot)=name.iter().position(|b|*b==b'.') else {return false;};
    (1..=8).contains(&dot)&&name.len()==dot+4&&name[..dot].iter().all(|b|b.is_ascii_alphanumeric()||*b==b'_')
        &&(name[dot+1..].eq_ignore_ascii_case(b"BIN")||name[dot+1..].eq_ignore_ascii_case(b"ELF"))
}
impl Manifest {
    pub fn parse(bytes:&[u8],expected:&[u8])->Result<Self,Error> {
        if bytes.is_empty()||bytes.len()>MAX_BYTES {return Err(Error::Size);}
        if !bytes.iter().all(|b|(32..=126).contains(b)||matches!(*b,b'\r'|b'\n')) {return Err(Error::Value);}
        let mut lines=bytes.split(|b|*b==b'\n');
        fn line(b:&[u8])->&[u8] {if b.ends_with(b"\r"){&b[..b.len()-1]}else{b}}
        if line(lines.next().unwrap())!=b"XINYI-APP 1" {return Err(Error::Header);}
        let mut out=Self{executable:[0;12],executable_len:0,title:[0;32],title_len:0,description:[0;64],description_len:0,icon:7,access:Access::Trusted,types:[[0;3];8],lengths:[0;8],count:0};
        let mut seen=0u8;
        for raw in lines {
            let raw=line(raw);if raw.contains(&b'\r') {return Err(Error::Value);}if raw.is_empty()||raw.starts_with(b"#") {continue;}
            let at=raw.iter().position(|b|*b==b'=').ok_or(Error::Field)?;let(key,value)=(&raw[..at],&raw[at+1..]);
            let flag=match key {b"exec"=>1,b"title"=>2,b"description"=>4,b"icon"=>8,b"access"=>16,b"handles"=>32,_=>return Err(Error::Field)};
            if seen&flag!=0 {return Err(Error::Duplicate);}seen|=flag;
            match flag {
                1=>{if !executable(value)||!value.eq_ignore_ascii_case(expected) {return Err(Error::Executable);}out.executable_len=value.len();out.executable[..value.len()].copy_from_slice(value);},
                2=>{if value.is_empty()||value.len()>32||value.iter().all(|b|*b==b' ') {return Err(Error::Value);}out.title_len=value.len();out.title[..value.len()].copy_from_slice(value);},
                4=>{if value.len()>64 {return Err(Error::Value);}out.description_len=value.len();out.description[..value.len()].copy_from_slice(value);},
                8=>out.icon=match value {b"folder"=>0,b"terminal"=>1,b"settings"=>2,b"document"=>3,b"audio"=>4,b"game"=>5,b"font"=>6,b"application"=>7,_=>return Err(Error::Value)},
                16=>out.access=match value {b"trusted"=>Access::Trusted,b"read-only"=>Access::ReadOnly,b"media"=>Access::Media,b"isolated"=>Access::Isolated,_=>return Err(Error::Value)},
                32=>if !value.is_empty() {for ext in value.split(|b|*b==b',') {
                    if out.count==8||ext.is_empty()||ext.len()>3||!(ext==b"-"||ext.iter().all(u8::is_ascii_alphanumeric))||ext.eq_ignore_ascii_case(b"BIN")||ext.eq_ignore_ascii_case(b"ELF") {return Err(Error::Value);}
                    if out.handles(ext) {return Err(Error::Duplicate);}
                    out.types[out.count][..ext.len()].copy_from_slice(ext);out.lengths[out.count]=ext.len() as u8;out.count+=1;
                }},
                _=>unreachable!(),
            }
        }
        if seen!=63 {return Err(Error::Field);}Ok(out)
    }
    pub fn title(&self)->&[u8] {&self.title[..self.title_len]}
    pub fn description(&self)->&[u8] {&self.description[..self.description_len]}
    pub fn handles(&self,extension:&[u8])->bool {
        let extension=if extension.is_empty(){b"-"}else{extension};
        (0..self.count).any(|i|self.types[i][..self.lengths[i] as usize].eq_ignore_ascii_case(extension))
    }
}
pub fn sidecar(executable:&[u8],output:&mut[u8])->Option<usize> {
    let leaf=executable.rsplit(|b|matches!(*b,b'/'|b'\\')).next()?;
    if !self::executable(leaf)||output.len()<executable.len() {return None;}
    output[..executable.len()].copy_from_slice(executable);let n=executable.len();output[n-3..n].copy_from_slice(b"APP");Some(n)
}
