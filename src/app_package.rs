//! XAP v1: bounded offline bundles. CRC detects corruption, not publisher identity.
use crate::app_manifest::Manifest;
pub const MAX_BYTES:usize=768*1024;
pub const MAX_ENTRIES:usize=63;
#[derive(Clone,Copy)]
pub struct Entry<'a> {pub path:&'a[u8],pub data:&'a[u8],pub directory:bool}
const EMPTY:Entry<'static>=Entry{path:b"",data:b"",directory:false};
pub struct Package<'a> {pub id:&'a[u8],pub entries:[Entry<'a>;MAX_ENTRIES],pub len:usize,pub manifest:Manifest}
pub fn crc(bytes:impl Iterator<Item=u8>)->u32 {
    let mut c=!0u32;for b in bytes {c^=b as u32;for _ in 0..8 {c=(c>>1)^(0xedb88320u32.wrapping_mul(c&1));}}!c
}
fn short(s:&[u8])->bool {
    let mut parts=s.split(|b|*b==b'.');let base=parts.next().unwrap_or(b"");let ext=parts.next();
    let chars=|p:&[u8]|p.iter().all(|b|b.is_ascii_uppercase()||b.is_ascii_digit()||matches!(*b,b'_'|b'-'));
    !base.is_empty()&&base.len()<=8&&chars(base)&&ext.map_or(true,|e|!e.is_empty()&&e.len()<=3&&chars(e))&&parts.next().is_none()
}
impl<'a> Package<'a> {
    pub fn parse(b:&'a[u8])->Result<Self,()> {
        if b.len()<24||b.len()>MAX_BYTES||&b[..8]!=b"XINYIPK1"||b[18..20]!=[0,0] {return Err(());}
        let n=b[8..16].iter().position(|x|*x==0).unwrap_or(8);let id=&b[8..8+n];
        if n==0||!id[0].is_ascii_uppercase()||!id.iter().all(|c|c.is_ascii_uppercase()||c.is_ascii_digit()||*c==b'_')||b[8+n..16].iter().any(|c|*c!=0) {return Err(());}
        let count=u16::from_le_bytes([b[16],b[17]]) as usize;
        if count<2||count>MAX_ENTRIES||crc(b[..20].iter().chain(b[24..].iter()).copied())!=u32::from_le_bytes(b[20..24].try_into().unwrap()) {return Err(());}
        let mut entries=[EMPTY;MAX_ENTRIES];let mut offset=24usize;
        for i in 0..count {
            let h=b.get(offset..offset+8).ok_or(())?;let length=h[0] as usize;let directory=h[1]==1;
            let size=u32::from_le_bytes(h[4..8].try_into().unwrap()) as usize;
            if length==0||length>77||h[1]>1||h[2..4]!=[0,0]||(directory&&size!=0) {return Err(());}
            offset+=8;let path=b.get(offset..offset+length).ok_or(())?;offset+=length;
            let depth=path.split(|c|*c==b'/').count();
            if depth>6||!path.split(|c|*c==b'/').all(short)||entries[..i].iter().any(|e|e.path==path) {return Err(());}
            if let Some(slash)=path.iter().rposition(|c|*c==b'/') {
                if !entries[..i].iter().any(|e|e.directory&&e.path==&path[..slash]) {return Err(());}
            }
            let end=offset.checked_add(size).ok_or(())?;let data=b.get(offset..end).ok_or(())?;offset=end;
            entries[i]=Entry{path,data,directory};
        }
        if offset!=b.len() {return Err(());}
        let mut name=[0;12];name[..n].copy_from_slice(id);name[n]=b'.';
        let mut executable=None;
        for ext in [b"BIN",b"ELF"] {
            name[n+1..n+4].copy_from_slice(ext);
            if let Some(e)=entries[..count].iter().find(|e|!e.directory&&e.path==&name[..n+4]) {
                if executable.is_some()||e.data.is_empty() {return Err(());}executable=Some(e.path);
            }
        }
        let executable=executable.ok_or(())?;name[n+1..n+4].copy_from_slice(b"APP");
        let metadata=entries[..count].iter().find(|e|!e.directory&&e.path==&name[..n+4]).ok_or(())?;
        let manifest=Manifest::parse(metadata.data,executable).map_err(|_|())?;
        Ok(Self{id,entries,len:count,manifest})
    }
}
