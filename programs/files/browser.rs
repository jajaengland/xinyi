use core::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View { Details, Compact, Icons }
impl View {
    pub fn layout(self, width: u32, height: u32) -> (u32, u32, u32, u16) {
        let columns = match self { Self::Details => 1, Self::Compact => ((width.saturating_sub(16))/208).max(1), Self::Icons => ((width.saturating_sub(16))/120).max(1) };
        let cell_width = width.saturating_sub(16)/columns;
        let cell_height = match self { Self::Details => 16, Self::Compact => 24, Self::Icons => 72 };
        let count = (columns * (height.saturating_sub(112)/cell_height)).min(32) as u16;
        (columns, cell_width, cell_height, count)
    }
    pub fn hit(self, width:u32, height:u32, x:i32, y:i32, count:usize)->Option<usize> {
        let (cols,cw,ch,capacity)=self.layout(width,height);
        if cw==0 || x<8 || y<80 || x>=width as i32-8 || y>=height as i32-32 {return None;}
        let col=(x as u32-8)/cw;
        let index=((y as u32-80)/ch)*cols+col;
        (col<cols && index<(capacity as u32).min(count as u32)).then_some(index as usize)
    }
    pub fn cell(self,width:u32,height:u32,index:usize)->(u32,u32,u32,u32) {
        let (cols,cw,ch,_)=self.layout(width,height);
        (8+(index as u32%cols)*cw,80+(index as u32/cols)*ch,cw,ch)
    }
}

#[derive(Clone, Copy)]
pub struct Favorites {
    pub paths: [[u8;105];8],
    pub lengths: [u8;8],
    pub count: usize,
}
impl Favorites {
    pub const fn new()->Self {Self{paths:[[0;105];8],lengths:[0;8],count:0}}
    fn valid(path:&[u8])->bool {
        !path.is_empty() && path.len()<=105 && path[0]==b'/'
            && path.iter().all(|b|(32..=126).contains(b)&&*b!=b'\\')
            && (path==b"/" || path[1..].split(|b|*b==b'/').all(|p|!p.is_empty()&&p.len()<=12&&p!=b"."&&p!=b".."))
    }
    pub fn toggle(&mut self,path:&[u8])->Result<bool,()> {
        if !Self::valid(path) {return Err(());}
        if let Some(i)=(0..self.count).find(|&i|self.paths[i][..self.lengths[i] as usize].eq_ignore_ascii_case(path)) {
            self.paths.copy_within(i+1..self.count,i);self.lengths.copy_within(i+1..self.count,i);
            self.count-=1;self.paths[self.count]=[0;105];self.lengths[self.count]=0;return Ok(false);
        }
        if self.count==8 {return Err(());}
        self.paths[self.count][..path.len()].copy_from_slice(path);self.lengths[self.count]=path.len() as u8;self.count+=1;Ok(true)
    }
    pub fn encode(&self,out:&mut[u8;1024])->usize {
        out.fill(0);out[..4].copy_from_slice(b"XFV1");out[4]=self.count as u8;let mut at=5;
        for i in 0..self.count {let n=self.lengths[i] as usize;out[at]=n as u8;out[at+1..at+1+n].copy_from_slice(&self.paths[i][..n]);at+=n+1;}at
    }
    pub fn decode(data:&[u8])->Option<Self> {
        if data.len()<5 || &data[..4]!=b"XFV1" || data[4]>8 {return None;}
        let mut result=Self::new();let mut at=5;
        for _ in 0..data[4] {
            let n=*data.get(at)? as usize;at+=1;let path=data.get(at..at+n)?;
            if !Self::valid(path) || (0..result.count).any(|i|result.paths[i][..result.lengths[i] as usize].eq_ignore_ascii_case(path)) {return None;}
            result.toggle(path).ok()?;at+=n;
        }
        (at==data.len()).then_some(result)
    }
}

#[derive(Clone, Copy, Default)]
pub struct Sort {
    pub column: u8,
    pub descending: bool,
}
impl Sort {
    pub fn choose(&mut self, column: u8) {
        self.descending = self.column == column && !self.descending;
        self.column = column;
    }
    pub fn compare(&self, a: (&[u8], u32, u32), b: (&[u8], u32, u32)) -> Ordering {
        // Keep folders together and use names to make ties deterministic.
        let folders = (b.1 & 1).cmp(&(a.1 & 1));
        if folders != Ordering::Equal { return folders; }
        let order = match self.column {
            2 => extension(a.0).cmp(extension(b.0)),
            3 => a.2.cmp(&b.2),
            _ => Ordering::Equal,
        }.then_with(|| a.0.cmp(b.0));
        if self.descending { order.reverse() } else { order }
    }
}
fn extension(name: &[u8]) -> &[u8] {
    name.iter().rposition(|&b| b == b'.').map_or(&[], |at| &name[at + 1..])
}

pub struct History {
    pub paths: [[u8; 105]; 8],
    pub lengths: [usize; 8],
    pub count: usize,
    pub cursor: usize,
}
impl History {
    pub const fn new() -> Self {
        Self { paths: [[0; 105]; 8], lengths: [0; 8], count: 0, cursor: 0 }
    }
    pub fn record(&mut self, path: &[u8]) {
        if path.is_empty() || path.len() > 105 { return; }
        if self.count != 0 && &self.paths[self.cursor][..self.lengths[self.cursor]] == path { return; }
        self.count = if self.count == 0 { 0 } else { self.cursor + 1 };
        if self.count == 8 {
            self.paths.copy_within(1..8, 0);
            self.lengths.copy_within(1..8, 0);
            self.count = 7;
        }
        self.cursor = self.count;
        self.paths[self.cursor] = [0; 105];
        self.paths[self.cursor][..path.len()].copy_from_slice(path);
        self.lengths[self.cursor] = path.len();
        self.count += 1;
    }
    pub fn target(&self, forward: bool) -> Option<usize> {
        if forward { (self.cursor + 1 < self.count).then_some(self.cursor + 1) }
        else { self.cursor.checked_sub(1) }
    }
}
