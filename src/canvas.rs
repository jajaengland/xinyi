// Drawing into owned RAM, independent of the display hardware.
#[path="text_layout.rs"]
pub mod text;
pub struct Canvas<'a> {pixels:&'a mut [u32],width:usize,height:usize,stride:usize}
impl<'a> Canvas<'a> {
    pub fn new(pixels:&'a mut [u32],width:usize,height:usize,stride:usize)->Option<Self> {
        if width==0||height==0||stride<width||stride.checked_mul(height)?>pixels.len() {return None;}
        Some(Self{pixels,width,height,stride})
    }
    pub fn pixel(&mut self,x:i32,y:i32,color:u32) {
        if x>=0&&y>=0&&(x as usize)<self.width&&(y as usize)<self.height {self.pixels[y as usize*self.stride+x as usize]=color;}
    }
    pub fn rect(&mut self,x:i32,y:i32,width:i32,height:i32,color:u32) {
        if width<=0||height<=0 {return;}
        let left=(x as i64).max(0).min(self.width as i64) as usize;
        let top=(y as i64).max(0).min(self.height as i64) as usize;
        let right=(x as i64+width as i64).max(0).min(self.width as i64) as usize;
        let bottom=(y as i64+height as i64).max(0).min(self.height as i64) as usize;
        for row in top..bottom {self.pixels[row*self.stride+left..row*self.stride+right].fill(color);}
    }
    pub fn line(&mut self,a:(i32,i32),b:(i32,i32),color:u32) {
        let(mut x0,mut y0,mut x1,mut y1)=(a.0 as i64,a.1 as i64,b.0 as i64,b.1 as i64);
        let(max_x,max_y)=((self.width-1) as i64,(self.height-1) as i64);
        // Clip before rasterizing, so even extreme off-screen endpoints do
        // bounded work. Wider intermediates prevent intersection overflow.
        let mut clipped=false;
        for _ in 0..16 {
            let c0=outcode(x0,y0,max_x,max_y);let c1=outcode(x1,y1,max_x,max_y);
            if c0|c1==0 {clipped=true;break;}if c0&c1!=0 {return;}
            let code=if c0!=0 {c0}else{c1};
            let(x,y)=if code&8!=0 {
                (x0+(((x1-x0) as i128*(max_y-y0) as i128)/(y1-y0) as i128) as i64,max_y)
            }else if code&4!=0 {
                (x0+(((x1-x0) as i128*(-y0) as i128)/(y1-y0) as i128) as i64,0)
            }else if code&2!=0 {
                (max_x,y0+(((y1-y0) as i128*(max_x-x0) as i128)/(x1-x0) as i128) as i64)
            }else {
                (0,y0+(((y1-y0) as i128*(-x0) as i128)/(x1-x0) as i128) as i64)
            };
            if c0!=0 {x0=x;y0=y;}else{x1=x;y1=y;}
        }
        if !clipped {return;}
        let dx=(x1-x0).abs();let dy=-(y1-y0).abs();
        let sx=if x0<x1 {1}else{-1};let sy=if y0<y1 {1}else{-1};let mut error=dx+dy;
        loop {
            self.pixel(x0 as i32,y0 as i32,color);if x0==x1&&y0==y1 {break;}
            let doubled=error*2;
            if doubled>=dy {error+=dy;x0+=sx;}if doubled<=dx {error+=dx;y0+=sy;}
        }
    }
    pub fn triangle(&mut self,a:(i32,i32),b:(i32,i32),c:(i32,i32),color:u32,filled:bool) {
        let area=edge(a,b,c);
        if !filled||area==0 {
            self.line(a,b,color);self.line(b,c,color);self.line(c,a,color);return;
        }
        let left=(a.0.min(b.0).min(c.0) as i64).max(0);
        let right=(a.0.max(b.0).max(c.0) as i64).min(self.width as i64-1);
        let top=(a.1.min(b.1).min(c.1) as i64).max(0);
        let bottom=(a.1.max(b.1).max(c.1) as i64).min(self.height as i64-1);
        if left>right||top>bottom {return;}
        for y in top..=bottom {for x in left..=right {
            let p=(x as i32,y as i32);let signs=[edge(a,b,p),edge(b,c,p),edge(c,a,p)];
            if (area>0&&signs.iter().all(|&s|s>=0))||(area<0&&signs.iter().all(|&s|s<=0)) {
                self.pixel(p.0,p.1,color);
            }
        }}
    }
    /// Draw one raw bitmap glyph, including CP437 console cells.
    pub fn glyph(&mut self,x:i32,y:i32,color:u32,byte:u8,font:&[u8;4096]) {
        if x>=0 && y>=0 && x as i64+8<=self.width as i64 && y as i64+16<=self.height as i64 {
            for row in 0..16 {
                let bits=font[byte as usize*16+row];if bits==0 {continue;}
                let start=(y as usize+row)*self.stride+x as usize;
                let output=&mut self.pixels[start..start+8];
                for column in 0..8 {if bits&(0x80>>column)!=0 {output[column]=color;}}
            }
            return;
        }
        self.glyph_clip(x,y,color,byte,font,0,8,(0,0,self.width as i64,self.height as i64));
    }
    fn glyph_clip(&mut self,x:i32,y:i32,color:u32,byte:u8,font:&[u8;4096],source:u8,width:u8,clip:(i64,i64,i64,i64)) {
        let (x,y)=(x as i64,y as i64);
        let left=x.max(clip.0).max(0);let right=(x+width as i64).min(clip.2).min(self.width as i64);
        let top=y.max(clip.1).max(0);let bottom=(y+16).min(clip.3).min(self.height as i64);
        if left>=right || top>=bottom {return;}
        for row in top..bottom {
            let bits=font[byte as usize*16+(row-y) as usize];
            if bits==0 {continue;}
            let output=&mut self.pixels[row as usize*self.stride+left as usize..row as usize*self.stride+right as usize];
            let mut mask=0x80u8>>(source as i64+left-x);
            for pixel in output {if bits&mask!=0 {*pixel=color;}mask>>=1;}
        }
    }
    fn ui_run(&mut self,mut x:i32,y:i32,color:u32,bytes:&[u8],clip:(i64,i64,i64,i64)) {
        for &byte in bytes {
            let g=text::glyph(byte,text::Font::Ui);
            self.glyph_clip(x,y,color,g.byte,&text::FONT,g.left,g.width,clip);
            x=x.saturating_add(g.advance as i32);
        }
    }
    /// Proportional label, ellipsized and clipped to the specified pixel box.
    pub fn label(&mut self,x:i32,y:i32,width:u32,color:u32,bytes:&[u8]) {
        let clip=(x as i64,y as i64,x as i64+width as i64,y as i64+16);
        let (n,dots)=text::ellipsis(bytes,width,text::Font::Ui);
        self.ui_run(x,y,color,&bytes[..n],clip);
        self.ui_run(x.saturating_add(text::width(&bytes[..n],text::Font::Ui) as i32),y,color,&b"..."[..dots],clip);
    }
    /// Word-wrapped proportional text with hard breaks for long words and strict clipping.
    pub fn paragraph(&mut self,x:i32,y:i32,width:u32,height:u32,color:u32,bytes:&[u8]) {
        if width==0 || height==0 {return;}
        let clip=(x as i64,y as i64,x as i64+width as i64,y as i64+height as i64);
        for (i,line) in text::Lines::new(bytes,width,text::Font::Ui).enumerate() {
            let dy=i as i64*16;if dy>=height as i64 {break;}
            self.ui_run(x,y.saturating_add(dy as i32),color,&bytes[line.start..line.end],clip);
        }
    }
    pub fn text(&mut self,mut x:i32,mut y:i32,color:u32,text:&[u8],font:&[u8;4096]) {
        let origin=x;
        for &byte in text {
            if byte==b'\n' {x=origin;y=y.saturating_add(16);continue;}
            let byte=if byte.is_ascii_graphic()||byte==b' ' {byte}else{b'?'};
            self.glyph(x,y,color,byte,font);x=x.saturating_add(8);
        }
    }
}
fn outcode(x:i64,y:i64,max_x:i64,max_y:i64)->u8 {
    (if x<0 {1}else if x>max_x {2}else{0})|(if y<0 {4}else if y>max_y {8}else{0})
}
fn edge(a:(i32,i32),b:(i32,i32),p:(i32,i32))->i128 {
    (b.0 as i128-a.0 as i128)*(p.1 as i128-a.1 as i128)
        -(b.1 as i128-a.1 as i128)*(p.0 as i128-a.0 as i128)
}
