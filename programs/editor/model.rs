pub const LIMIT:usize=1024;
pub const WIDTH:usize=80;
pub struct Document {
    pub data:[u8;LIMIT],pub length:usize,pub cursor:usize,
    saved:[u8;LIMIT],saved_length:usize,
    undo:[u8;LIMIT],undo_length:usize,undo_cursor:usize,has_undo:bool,
}
impl Document {
    pub fn new()->Self {Self{data:[0;LIMIT],length:0,cursor:0,saved:[0;LIMIT],saved_length:0,undo:[0;LIMIT],undo_length:0,undo_cursor:0,has_undo:false}}
    pub fn dirty(&self)->bool {self.length!=self.saved_length||self.data[..self.length]!=self.saved[..self.saved_length]}
    pub fn mark_saved(&mut self) {self.saved=self.data;self.saved_length=self.length;}
    pub fn load(&mut self,input:&[u8])->bool {
        let mut converted=[0;LIMIT];let mut length=0;let mut previous_cr=false;
        for &byte in input {
            if byte==b'\n'&&previous_cr {previous_cr=false;continue;}
            previous_cr=byte==b'\r';
            let (value,count)=match byte {b'\r'|b'\n'=>(b'\n',1),b'\t'=>(b' ',4),32..=126=>(byte,1),_=>return false};
            if length+count>LIMIT {return false;}
            converted[length..length+count].fill(value);length+=count;
        }
        self.data=converted;self.length=length;self.cursor=0;self.has_undo=false;self.mark_saved();true
    }
    fn checkpoint(&mut self) {self.undo=self.data;self.undo_length=self.length;self.undo_cursor=self.cursor;self.has_undo=true;}
    pub fn insert(&mut self,text:&[u8])->bool {
        if self.length+text.len()>LIMIT {return false;}
        self.checkpoint();self.data.copy_within(self.cursor..self.length,self.cursor+text.len());
        self.data[self.cursor..self.cursor+text.len()].copy_from_slice(text);self.length+=text.len();self.cursor+=text.len();true
    }
    pub fn delete(&mut self,backspace:bool) {
        if backspace {if self.cursor==0{return;}}else if self.cursor==self.length{return;}
        self.checkpoint();if backspace {self.cursor-=1;}
        self.data.copy_within(self.cursor+1..self.length,self.cursor);self.length-=1;
    }
    pub fn undo(&mut self) {
        if !self.has_undo{return;}
        core::mem::swap(&mut self.data,&mut self.undo);core::mem::swap(&mut self.length,&mut self.undo_length);
        core::mem::swap(&mut self.cursor,&mut self.undo_cursor);
    }
    pub fn position(&self,index:usize)->(usize,usize) {
        let(mut row,mut column)=(0,0);
        for &byte in &self.data[..index.min(self.length)] {
            if byte==b'\n' {row+=1;column=0;}else {column+=1;if column==WIDTH {row+=1;column=0;}}
        }(row,column)
    }
    pub fn offset(&self,row:usize,column:usize)->usize {
        let mut best=0;
        for index in 0..=self.length {let(r,c)=self.position(index);
            if r>row {break;}if r<row||c<=column {best=index;}
        }best
    }
    pub fn vertical(&mut self,delta:i32) {
        let(row,column)=self.position(self.cursor);
        let target=(row as i32+delta).max(0) as usize;self.cursor=self.offset(target,column);
    }
    pub fn screen_row(&self,row:usize)->[u8;WIDTH] {
        let mut output=[b' ';WIDTH];
        let(mut r,mut c)=(0,0);
        for &byte in &self.data[..self.length] {
            if r>row {break;}
            if byte==b'\n' {r+=1;c=0;}else {
                if r==row {output[c]=byte;}
                c+=1;if c==WIDTH {r+=1;c=0;}
            }
        }output
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn insertion_deletion_and_undo(){let mut d=Document::new();d.insert(b"ac");d.cursor=1;d.insert(b"b");assert_eq!(&d.data[..d.length],b"abc");d.undo();assert_eq!(&d.data[..d.length],b"ac");d.undo();assert_eq!(&d.data[..d.length],b"abc");d.delete(true);assert_eq!(&d.data[..d.length],b"ac");d.delete(false);assert_eq!(&d.data[..d.length],b"a");}
    #[test]fn save_baseline_and_normalization(){let mut d=Document::new();assert!(d.load(b"a\r\nb\tc\rd"));assert_eq!(&d.data[..d.length],b"a\nb    c\nd");assert!(!d.dirty());d.insert(b"x");assert!(d.dirty());d.undo();assert!(!d.dirty());}
    #[test]fn capacity_and_bad_load_preserve_document(){let mut d=Document::new();d.insert(&[b'a';LIMIT]);assert!(!d.insert(b"x"));assert_eq!(d.length,LIMIT);assert!(!d.load(&[0xff]));assert_eq!(d.length,LIMIT);}
    #[test]fn navigation_and_wrapping(){let mut d=Document::new();d.load(b"abc\nx\nxyz");d.cursor=2;d.vertical(1);assert_eq!(d.cursor,5);d.vertical(1);assert_eq!(d.cursor,7);d.load(&[b'x';161]);assert_eq!(d.position(160),(2,0));assert_eq!(d.offset(1,5),85);}
    #[test]fn empty_lines_and_end_boundaries(){let mut d=Document::new();d.load(b"\n\n");assert_eq!(d.position(2),(2,0));d.cursor=2;d.delete(false);assert_eq!(d.length,2);d.delete(true);assert_eq!(d.length,1);}
}
