//! Bounded BI_RGB BMP decoder. Format reference: Microsoft BITMAPINFOHEADER.
//! https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-bitmapinfoheader
pub const MAX_FILE:usize=768*1024;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Image {pub width:u32,pub height:u32,pub stride:usize,pub offset:usize,pub bytes:usize,pub top_down:bool}
fn u32_at(b:&[u8],at:usize)->Option<u32> {Some(u32::from_le_bytes(b.get(at..at+4)?.try_into().ok()?))}
pub fn parse(b:&[u8])->Option<Image> {
    if b.len()<54 || b.len()>MAX_FILE || b.get(..2)?!=b"BM" || b[6..10]!=[0;4] {return None;}
    let declared=u32_at(b,2)? as usize;let offset=u32_at(b,10)? as usize;
    let header=u32_at(b,14)? as usize;
    if !matches!(header,40|108|124) || 14usize.checked_add(header)?>offset || offset>b.len() {return None;}
    let width=u32_at(b,18)? as i32;let h=u32_at(b,22)? as i32;
    let height=h.checked_abs()? as u32;
    if width<=0 || width>2048 || height==0 || height>2048 || b[26..28]!=[1,0] || u32_at(b,30)?!=0 {return None;}
    let bytes=match u16::from_le_bytes([b[28],b[29]]) {24=>3,32=>4,_=>return None,};
    let stride=(width as usize).checked_mul(bytes)?.checked_add(3)?&!3;
    let end=offset.checked_add(stride.checked_mul(height as usize)?)?;
    if end>b.len() || declared<end || declared>b.len() {return None;}
    Some(Image{width:width as u32,height,stride,offset,bytes,top_down:h<0})
}
impl Image {
    pub fn pixel(&self,data:&[u8],x:u32,y:u32)->u32 {
        let row=if self.top_down {y}else{self.height-1-y};
        let at=self.offset+row as usize*self.stride+x as usize*self.bytes;
        (data[at+2] as u32)<<16 | (data[at+1] as u32)<<8 | data[at] as u32
    }
    pub fn fit(&self,width:u32,height:u32)->(u32,u32) {
        if width==0||height==0 {return (0,0);}
        if self.width<=width && self.height<=height {return (self.width,self.height);}
        if self.width as u64*height as u64>self.height as u64*width as u64 {
            (width,((self.height as u64*width as u64/self.width as u64) as u32).max(1))
        }else {(((self.width as u64*height as u64/self.height as u64) as u32).max(1),height)}
    }
}
