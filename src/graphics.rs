// First display backend: QEMU standard PCI VGA (1234:1111), Bochs DISPI.
// Register/BAR layout: https://www.qemu.org/docs/master/specs/standard-vga.html
// The kernel owns VRAM; user programs retain their supervisor-only VGA mapping.
use crate::{canvas::Canvas,loader::Frames};
use core::{arch::asm,ptr::{read_volatile,write_volatile}};
const WIDTH:usize=800;
const HEIGHT:usize=600;
const LEGACY_BYTES:usize=256*1024;
unsafe fn input(port:u16)->u8 {let value:u8;asm!("in al, dx",in("dx")port,out("al")value,options(nomem,nostack,preserves_flags));value}
unsafe fn output(port:u16,value:u8) {asm!("out dx, al",in("dx")port,in("al")value,options(nomem,nostack,preserves_flags));}
unsafe fn get(port:u16,index:u8)->u8 {output(port,index);input(port+1)}
unsafe fn set(port:u16,index:u8,value:u8) {output(port,index);output(port+1,value);}
fn dispi(index:u16)->u16 {unsafe {asm!("out dx, ax",in("dx")0x1ceu16,in("ax")index,options(nomem,nostack,preserves_flags));
    let value:u16;asm!("in ax, dx",in("dx")0x1cfu16,out("ax")value,options(nomem,nostack,preserves_flags));value}}
fn set_dispi(index:u16,value:u16) {unsafe {asm!("out dx, ax",in("dx")0x1ceu16,in("ax")index,options(nomem,nostack,preserves_flags));
    asm!("out dx, ax",in("dx")0x1cfu16,in("ax")value,options(nomem,nostack,preserves_flags));}}
struct Registers {misc:u8,seq:[u8;5],crtc:[u8;25],gc:[u8;9],attr:[u8;21],palette:[u8;768],mask:u8,vbe:[u16;10]}
impl Registers {
    unsafe fn save()->Self {
        let mut r=Self{misc:input(0x3cc),seq:[0;5],crtc:[0;25],gc:[0;9],attr:[0;21],palette:[0;768],mask:input(0x3c6),vbe:[0;10]};
        for i in 0..5 {r.seq[i]=get(0x3c4,i as u8);}
        for i in 0..25 {r.crtc[i]=get(0x3d4,i as u8);}
        for i in 0..9 {r.gc[i]=get(0x3ce,i as u8);}
        for i in 0..21 {input(0x3da);output(0x3c0,i as u8);r.attr[i]=input(0x3c1);}
        input(0x3da);output(0x3c0,0x20);
        output(0x3c7,0);for value in &mut r.palette {*value=input(0x3c9);}
        for i in 1..10 {r.vbe[i]=dispi(i as u16);}
        r
    }
    unsafe fn restore(&self) {
        set_dispi(4,0);
        for i in [1,2,3,5,6,8,9] {set_dispi(i,self.vbe[i as usize]);}
        output(0x3c2,self.misc);
        set(0x3c4,0,1);for i in 1..5 {set(0x3c4,i as u8,self.seq[i]);}set(0x3c4,0,self.seq[0]);
        set(0x3d4,0x11,self.crtc[0x11]&0x7f);
        for i in 0..25 {set(0x3d4,i as u8,self.crtc[i]);}
        for i in 0..9 {set(0x3ce,i as u8,self.gc[i]);}
        output(0x3c6,self.mask);output(0x3c8,0);for &value in &self.palette {output(0x3c9,value);}
        for i in 0..21 {input(0x3da);output(0x3c0,i as u8);output(0x3c0,self.attr[i]);}
        input(0x3da);output(0x3c0,0x20);
    }
}
struct Session {base:u32,saved:Frames,back:Frames,registers:Registers,font:Frames}
static mut SESSION:Option<Session>=None;
fn device()->Option<u32> {
    for bus in 0u16..256 {for slot in 0..32 {
        if crate::pci::read(bus as u8,slot,0,0)!=0x11111234 {continue;}
        let command=crate::pci::read(bus as u8,slot,0,4);
        let bar=crate::pci::read(bus as u8,slot,0,0x10);
        let base=bar&!15;
        if command&3!=3||bar&7!=0||base<0x80000000||base.checked_add((WIDTH*HEIGHT*4) as u32).is_none() {return None;}
        // No native framebuffer pages may overlap the allocator's user window.
        return Some(base);
    }}None
}
fn start()->Result<(),&'static str> {
    if unsafe {(*core::ptr::addr_of!(SESSION)).is_some()} {return Err("Graphics already active.");}
    if !crate::paging::status().enabled {return Err("Graphics requires protected paging.");}
    let base=device().ok_or("Graphics backend unavailable: this test requires QEMU standard VGA (-vga std).")?;
    if !(0xb0c2..=0xb0c5).contains(&dispi(0))||dispi(4)&1!=0 {return Err("Display is not in a supported text state.");}
    if dispi(10) as usize*65536<WIDTH*HEIGHT*4 {return Err("Not enough video memory.");}
    let saved=Frames::new(LEGACY_BYTES/4096).ok_or("Not enough memory for text recovery.")?;
    let back=Frames::new((WIDTH*HEIGHT*4).div_ceil(4096)).ok_or("Not enough memory for a back buffer.")?;
    let font=Frames::new(1).ok_or("Not enough memory for the bitmap font.")?;
    if unsafe {input(0x3cc)}&1==0 {return Err("Monochrome text mode is unsupported.");}
    let registers=unsafe {Registers::save()};
    if registers.seq[3]&0x3f!=0 {return Err("Non-default VGA font banks are unsupported.");}
    unsafe {for index in 0..LEGACY_BYTES {write_volatile((saved.base() as *mut u8).add(index),read_volatile((base as *const u8).add(index)));}}
    for glyph in 0..256 {for row in 0..16 {
        unsafe {write_volatile((font.base() as *mut u8).add(glyph*16+row),read_volatile((saved.base() as *const u8).add((glyph*32+row)*4+2)));}
    }}
    unsafe {*core::ptr::addr_of_mut!(SESSION)=Some(Session{base,saved,back,registers,font});}
    set_dispi(4,0);set_dispi(1,WIDTH as u16);set_dispi(2,HEIGHT as u16);set_dispi(3,32);
    set_dispi(6,WIDTH as u16);set_dispi(8,0);set_dispi(9,0);set_dispi(4,0xc1);
    if dispi(1)!=WIDTH as u16||dispi(2)!=HEIGHT as u16||dispi(3)!=32||dispi(4)&1==0 {
        restore();return Err("The display refused the requested mode.");
    }
    Ok(())
}
pub fn restore() {
    let session=unsafe {(*core::ptr::addr_of_mut!(SESSION)).take()};
    if let Some(session)=session {
        set_dispi(4,0);
        unsafe {
            for index in 0..LEGACY_BYTES {write_volatile((session.base as *mut u8).add(index),read_volatile((session.saved.base() as *const u8).add(index)));}
            session.registers.restore();
        }
    }
}
fn draw_scene() {
    let session=unsafe {(&mut *core::ptr::addr_of_mut!(SESSION)).as_mut().unwrap()};
    let data=unsafe {core::slice::from_raw_parts_mut(session.back.base() as *mut u32,WIDTH*HEIGHT)};
    let font=unsafe {&*(session.font.base() as *const [u8;4096])};
    let mut canvas=Canvas::new(data,WIDTH,HEIGHT,WIDTH).unwrap();
    canvas.rect(0,0,800,600,0x176c70);
    canvas.rect(0,570,800,30,0xc4c4c4);canvas.rect(10,576,80,18,0xaaaaaa);
    canvas.text(22,577,0,b"XINYI",font);
    canvas.text(110,577,0x303030,b"Graphics foundation - keyboard demo",font);
    canvas.rect(108,88,600,450,0x10383a);canvas.rect(100,80,600,450,0xc4c4c4);
    canvas.rect(104,84,592,25,0x153e8e);canvas.text(115,89,0xffffff,b"Xinyi graphics test",font);
    canvas.rect(674,88,18,17,0xc4c4c4);canvas.text(679,89,0,b"X",font);
    canvas.text(122,125,0x202020,b"800 x 600 | 32-bit colour | RAM back buffer",font);
    canvas.text(122,150,0x202020,b"Pixels, triangles, lines and bitmap text",font);
    for (index,color) in [0xd95755,0x4bb76a,0x4a7fe0,0xecc653,0x8a63c9,0xeeeeee].iter().enumerate() {
        canvas.rect(125+index as i32*90,190,74,70,*color);
    }
    canvas.rect(122,282,554,170,0x202833);
    canvas.triangle((160,425),(235,300),(310,425),0xe5a34f,true);
    canvas.triangle((160,425),(235,300),(310,425),0xffdfb1,false);
    canvas.triangle((525,410),(570,330),(615,410),0x5865ac,true);
    canvas.triangle((475,430),(570,295),(650,430),0x8fe186,false);
    canvas.text(122,470,0x202020,b"ARROWS move the box. ESC returns to the shell.",font);
    canvas.text(122,495,0x202020,b"QEMU test backend - hardware VBE comes later.",font);
    drop(canvas);
    for (index,&value) in data.iter().enumerate() {unsafe {write_volatile((session.base as *mut u32).add(index),value);}}
}
fn box_pixels(x:i32,y:i32,restore_background:bool) {
    let session=unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap()};
    // The RAM buffer contains the immutable scene, including triangles. Restore
    // only the old box footprint, then paint the new one directly to VRAM.
    for row in y as usize..y as usize+40 {for column in x as usize..x as usize+48 {
        let index=row*WIDTH+column;
        let value=if restore_background {unsafe {read_volatile((session.back.base() as *const u32).add(index))}}else{0x58ccd0};
        unsafe {write_volatile((session.base as *mut u32).add(index),value);}
    }}
}
pub fn demo() {
    if let Err(message)=start() {crate::println!("{}",message);return;}
    crate::debug::write_str("Graphics: 800x600x32 active\n");
    crate::keyboard::clear_arrows();
    let(mut x,mut y)=(350,340);draw_scene();box_pixels(x,y,false);
    crate::debug::write_str("Graphics: frame ready\n");
    let mut last_move=crate::interrupts::timer_ticks();
    loop {
        let mut pressed=[false;4];let mut quit=false;
        for _ in 0..128 {
            if crate::keyboard::queued_scancodes()==0 {break;}
            let before=crate::keyboard::arrows_down();
            let key=crate::keyboard::poll_key();
            if key==Some(crate::keyboard::Key::Char('\u{1b}')) {quit=true;break;}
            let after=crate::keyboard::arrows_down();
            for i in 0..4 {pressed[i]|=after[i]&&!before[i];}
        }
        if quit {break;}
        let held=crate::keyboard::arrows_down();let now=crate::interrupts::timer_ticks();
        if pressed.iter().any(|&p|p)||now.wrapping_sub(last_move)>=2 {
            let directions=core::array::from_fn::<_,4,_>(|i|held[i]||pressed[i]);
            let nx=(x+(directions[3] as i32-directions[2] as i32)*8).clamp(124,626);
            let ny=(y+(directions[1] as i32-directions[0] as i32)*8).clamp(284,410);
            if nx!=x||ny!=y {box_pixels(x,y,true);box_pixels(nx,ny,false);x=nx;y=ny;}
            last_move=now;
        }
        crate::idle::wait_for_input();
    }
    crate::keyboard::clear_arrows();
    restore();crate::debug::write_str("Graphics: text mode restored\n");
    crate::println!("Returned to text mode.");
}
