// Display backends: QEMU Bochs DISPI and BIOS VESA linear framebuffer.
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
struct Session {base:u32,saved:Frames,back:Frames,registers:Option<Registers>,font:Frames,native:Option<crate::vbe::Mode>,cursor:(usize,usize),visible:bool}
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
pub(crate) fn start()->Result<(),&'static str> {
    if unsafe {(*core::ptr::addr_of!(SESSION)).is_some()} {return Err("Graphics already active.");}
    if !crate::paging::status().enabled {return Err("Graphics requires protected paging.");}
    let Some(base)=device() else{return start_native();};
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
    unsafe {*core::ptr::addr_of_mut!(SESSION)=Some(Session{base,saved,back,registers:Some(registers),font,native:None,cursor:crate::vga::cursor_position(),visible:crate::vga::cursor_visible()});}
    set_dispi(4,0);set_dispi(1,WIDTH as u16);set_dispi(2,HEIGHT as u16);set_dispi(3,32);
    set_dispi(6,WIDTH as u16);set_dispi(8,0);set_dispi(9,0);set_dispi(4,0xc1);
    if dispi(1)!=WIDTH as u16||dispi(2)!=HEIGHT as u16||dispi(3)!=32||dispi(4)&1==0 {
        restore();return Err("The display refused the requested mode.");
    }
    Ok(())
}
fn start_native()->Result<(),&'static str> {
    let mode=crate::vbe::find_mode()?;
    let saved=Frames::new(1).ok_or("Not enough memory to save the text console.")?;
    let back=Frames::new((WIDTH*HEIGHT*4).div_ceil(4096)).ok_or("Not enough memory for graphics.")?;
    let font=crate::vbe::font()?;
    unsafe {for index in 0..4000 {write_volatile((saved.base() as *mut u8).add(index),read_volatile((0xb8000 as *const u8).add(index)));}}
    unsafe {*core::ptr::addr_of_mut!(SESSION)=Some(Session{base:mode.base,saved,back,registers:None,font,native:Some(mode),cursor:crate::vga::cursor_position(),visible:crate::vga::cursor_visible()});}
    if !crate::vbe::set_mode(mode) {restore();return Err("The BIOS refused the VESA graphics mode.");}
    crate::debug::write_fmt(format_args!("Graphics: BIOS VBE mode {:04x} base={:08x} pitch={} bpp={}\n",mode.number,mode.base,mode.pitch,mode.bpp));
    Ok(())
}
pub fn restore() {
    let session=unsafe {(*core::ptr::addr_of_mut!(SESSION)).take()};
    if let Some(session)=session {
        if session.native.is_some() {
            if crate::vbe::text_mode() {
                unsafe {for index in 0..4000 {write_volatile((0xb8000 as *mut u8).add(index),read_volatile((session.saved.base() as *const u8).add(index)));}}
                crate::vga::move_cursor(session.cursor.0,session.cursor.1);crate::vga::set_cursor_visible(session.visible);
            }else{crate::debug::write_str("BIOS text-mode restoration failed\n");}
        }else{
            set_dispi(4,0);
            unsafe {
                for index in 0..LEGACY_BYTES {write_volatile((session.base as *mut u8).add(index),read_volatile((session.saved.base() as *const u8).add(index)));}
                session.registers.as_ref().unwrap().restore();
            }
        }
    }
}
fn put_pixel(session:&Session,index:usize,color:u32) {
    if let Some(mode)=session.native {
        let offset=(index/WIDTH)*mode.pitch as usize+(index%WIDTH)*(mode.bpp as usize/8);
        let address=(mode.base as *mut u8).wrapping_add(offset);
        unsafe {match mode.bpp {
            32=>write_volatile(address as *mut u32,color),
            24=>{write_volatile(address,color as u8);write_volatile(address.add(1),(color>>8) as u8);write_volatile(address.add(2),(color>>16) as u8);}
            16=>write_volatile(address as *mut u16,(((color>>19)&31)<<11|((color>>10)&63)<<5|((color>>3)&31)) as u16),
            _=>(),
        }}
    }else{unsafe {write_volatile((session.base as *mut u32).add(index),color);}}
}
fn draw_scene() {
    let session=unsafe {(&mut *core::ptr::addr_of_mut!(SESSION)).as_mut().unwrap()};
    let data=unsafe {core::slice::from_raw_parts_mut(session.back.base() as *mut u32,WIDTH*HEIGHT)};
    let font=unsafe {&*(session.font.base() as *const [u8;4096])};
    let mut canvas=Canvas::new(data,WIDTH,HEIGHT,WIDTH).unwrap();
    canvas.rect(0,0,800,600,0x176c70);
    canvas.rect(0,0,800,30,0xc4c4c4);canvas.rect(10,6,80,18,0xaaaaaa);
    canvas.text(22,7,0,b"XINYI",font);
    canvas.text(110,7,0x303030,b"Graphics foundation - keyboard demo",font);
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
    canvas.text(122,495,0x202020,if session.native.is_some() {b"VESA BIOS framebuffer - hardware display path"}else{b"QEMU standard VGA framebuffer"},font);
    drop(canvas);
    for (index,&value) in data.iter().enumerate() {put_pixel(session,index,value);}
}
fn box_pixels(x:i32,y:i32,restore_background:bool) {
    let session=unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap()};
    // The RAM buffer contains the immutable scene, including triangles. Restore
    // only the old box footprint, then paint the new one directly to VRAM.
    for row in y as usize..y as usize+40 {for column in x as usize..x as usize+48 {
        let index=row*WIDTH+column;
        let value=if restore_background {unsafe {read_volatile((session.back.base() as *const u32).add(index))}}else{0x58ccd0};
        put_pixel(session,index,value);
    }}
}
pub fn demo() {
    if let Err(message)=start() {crate::println!("{}",message);return;}
    crate::debug::write_str("Graphics: 800x600 active\n");
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

// The scene buffer never contains the pointer: erase it by copying its old
// footprint, then draw the arrow into VRAM. Both backends share this path.
// Transfer framebuffer spans in machine words. VESA's packed 24/16-bit rows
// are converted in RAM first, avoiding a separate MMIO write for each channel.
unsafe fn copy_video(source:*const u8,destination:*mut u8,length:usize) {
    let words=length/4;
    asm!("push esi", "mov esi, eax", "cld", "rep movsd", "pop esi",in("eax")source,inout("edi")destination=>_,inout("ecx")words=>_);
    for i in words*4..length {write_volatile(destination.add(i),core::ptr::read(source.add(i)));}
}
fn transfer_span(session:&Session,y:usize,left:usize,right:usize,row:&mut [u8;3200]) {
    if left>=right {return;}let count=right-left;
    let source=unsafe {(session.back.base() as *const u32).add(y*WIDTH+left)};
    let Some(mode)=session.native else {unsafe {copy_video(source as *const u8,(session.base as *mut u8).add((y*WIDTH+left)*4),count*4);}return;};
    let bytes=mode.bpp as usize/8;let destination=(mode.base as *mut u8).wrapping_add(y*mode.pitch as usize+left*bytes);
    if bytes==4 {unsafe {copy_video(source as *const u8,destination,count*4);}return;}
    for x in 0..count {let color=unsafe {core::ptr::read(source.add(x))};let at=x*bytes;
        if bytes==3 {row[at]=color as u8;row[at+1]=(color>>8) as u8;row[at+2]=(color>>16) as u8;}
        else {let value=(((color>>19)&31)<<11|((color>>10)&63)<<5|((color>>3)&31)) as u16;row[at]=value as u8;row[at+1]=(value>>8) as u8;}
    }
    unsafe {copy_video(row.as_ptr(),destination,count*bytes);}
}
fn present_area(x:i32,y:i32,w:i32,h:i32,cursor:Option<(i32,i32)>) {
    if w<=0||h<=0 {return;}
    let session=unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap()};
    let left=x.clamp(0,WIDTH as i32) as usize;let right=x.saturating_add(w).clamp(0,WIDTH as i32) as usize;
    let mut buffer=[0u8;3200];
    for row in y.max(0)..y.saturating_add(h).min(HEIGHT as i32) {
        if let Some((cx,cy))=cursor {if row>=cy&&row<cy+24 {
            transfer_span(session,row as usize,left,right.min(cx.max(0) as usize),&mut buffer);
            transfer_span(session,row as usize,left.max((cx+16).max(0) as usize),right,&mut buffer);continue;
        }}
        transfer_span(session,row as usize,left,right,&mut buffer);
    }
}
pub(crate) fn present_rect(x:i32,y:i32,w:i32,h:i32) {present_area(x,y,w,h,None);}
pub(crate) fn present_guarded(x:i32,y:i32,w:i32,h:i32,cursor:(i32,i32)) {present_area(x,y,w,h,Some(cursor));}
// Place the new cursor first, then erase the old footprint without touching it.
// Subsequent window transfers exclude the new footprint, keeping it visible
// for the entire copy rather than hiding it until a large repaint finishes.
pub(crate) fn move_pointer(old:(i32,i32),new:(i32,i32)) {
    present_rect(new.0,new.1,16,24);pointer(new.0,new.1);
    present_guarded(old.0,old.1,16,24,new);
}
const POINTER:[&[u8];24]=[
    b"B...............",b"BB..............",b"BWB.............",b"BWWB............",
    b"BWWWB...........",b"BWWWWB..........",b"BWWWWWB.........",b"BWWWWWWB........",
    b"BWWWWWWWB.......",b"BWWWWWWWWB......",b"BWWWWWWWWWB.....",b"BWWWWWWWWWWB....",
    b"BWWWWWWBBBBBB...",b"BWWWBWWB........",b"BWWB.BWWB.......",b"BWB..BWWB.......",
    b"BB....BWWB......",b"B.....BWWB......",b".......BWWB.....",b".......BWWB.....",
    b"........BB......",b"................",b"................",b"................",
];
pub(crate) fn pointer(x:i32,y:i32) {
    let session=unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap()};
    for (row,pattern) in POINTER.iter().enumerate() {for (column,&pixel) in pattern.iter().enumerate() {
        let(px,py)=(x+column as i32,y+row as i32);
        if pixel!=b'.'&&px>=0&&py>=0&&px<WIDTH as i32&&py<HEIGHT as i32 {
            put_pixel(session,py as usize*WIDTH+px as usize,if pixel==b'W' {0xffffff}else{0});
        }
    }}
}
fn bevel(canvas:&mut Canvas,x:i32,y:i32,w:i32,h:i32,pressed:bool) {
    canvas.rect(x,y,w,h,0xc4c4c4);
    let(light,dark)=if pressed {(0x404040,0xffffff)}else{(0xffffff,0x404040)};
    canvas.rect(x,y,w,2,light);canvas.rect(x,y,2,h,light);
    canvas.rect(x,y+h-2,w,2,dark);canvas.rect(x+w-2,y,2,h,dark);
}
fn mouse_scene(state:&crate::mouse_ui::State) {
    use crate::mouse_ui::{Target,MENU_W,MENU_H};
    let session=unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap()};
    let data=unsafe {core::slice::from_raw_parts_mut(session.back.base() as *mut u32,WIDTH*HEIGHT)};
    let font=&crate::ui_font::FONT;
    let mut canvas=Canvas::new(data,WIDTH,HEIGHT,WIDTH).unwrap();
    canvas.rect(0,0,800,600,0x000080);
    canvas.text(24,24,0xffffff,b"Xinyi mouse test",font);
    canvas.text(24,52,0xffffff,b"Left-click the buttons. Right-click for a menu. ESC exits.",font);
    for i in 0..2 {
        let pressed=state.capture==Some(Target::Button(i))&&state.hit()==Some(Target::Button(i));
        let x=260+i as i32*160;bevel(&mut canvas,x,240,120,44,pressed);
        let offset=if pressed {1}else{0};
        canvas.text(x+28+offset,254+offset,0,if i==0 {b"Button 1"}else{b"Button 2"},font);
    }
    let status=match state.last {Some(0)=>b"Button 1 clicked" as &[u8],Some(_)=>b"Button 2 clicked",None=>b"Ready - click either button"};
    canvas.text(24,520,0xffffff,status,font);
    for i in 0..2 {
        let mut digits=[b'0';10];let mut value=state.clicks[i];
        for digit in digits.iter_mut().rev() {*digit=b'0'+(value%10) as u8;value/=10;}
        let first=digits.iter().position(|&b|b!=b'0').unwrap_or(9);
        canvas.text(24+i as i32*220,548,0xffffff,if i==0 {b"Button 1:"}else{b"Button 2:"},font);
        canvas.text(104+i as i32*220,548,0xffffff,&digits[first..],font);
    }
    if let Some((x,y))=state.menu {
        bevel(&mut canvas,x,y,MENU_W,MENU_H,false);
        canvas.rect(x+4,y+35,MENU_W-8,1,0x808080);
        for i in 0..2 {
            let top=y+4+i as i32*36;let hover=state.hit()==Some(Target::Menu(i));
            if hover {canvas.rect(x+4,top,MENU_W-8,28,0x000080);}
            canvas.text(x+12,top+6,if hover {0xffffff}else{0},if i==0 {b"Reset clicks"}else{b"Close menu"},font);
        }
    }
}
pub fn mouse_demo() {
    if !crate::mouse::init() {crate::println!("PS/2 mouse unavailable (or initialization timed out).");return;}
    if let Err(message)=start() {crate::println!("{}",message);return;}
    let mut state=crate::mouse_ui::State::new(crate::mouse::discard());
    mouse_scene(&state);present_rect(0,0,800,600);pointer(state.x,state.y);
    crate::debug::write_str("Mouse: frame ready\n");
    loop {
        let mut quit=false;
        for _ in 0..128 {
            if crate::keyboard::queued_scancodes()==0 {break;}
            if crate::keyboard::poll_key()==Some(crate::keyboard::Key::Char('\u{1b}')) {quit=true;break;}
        }
        if quit {break;}
        for _ in 0..128 {
            let Some(event)=crate::mouse::poll() else{break;};
            let(old_x,old_y,old_menu,old_buttons)=(state.x,state.y,state.menu,state.buttons);
            let mut changed=state.update(event);
            if !event.lost&&event.buttons==old_buttons {for _ in 0..63 {
                if !crate::mouse::motion_before(event.buttons,crate::keyboard::next_order()) {break;}
                changed|=state.update(crate::mouse::poll().unwrap());
            }}
            if changed {mouse_scene(&state);}
            move_pointer((old_x,old_y),(state.x,state.y));
            if changed {
                let cursor=(state.x,state.y);
                present_guarded(250,230,300,64,cursor);present_guarded(24,520,752,48,cursor);
                for menu in [old_menu,state.menu].into_iter().flatten() {present_guarded(menu.0,menu.1,crate::mouse_ui::MENU_W,crate::mouse_ui::MENU_H,cursor);}
                crate::debug::write_fmt(format_args!("Mouse: ui clicks={}/{} menu={}\n",state.clicks[0],state.clicks[1],state.menu.is_some()));
            }
        }
        crate::idle::wait_for_pointer();
    }
    crate::mouse::discard();restore();
    crate::debug::write_str("Mouse: text mode restored\n");crate::println!("Returned to text mode.");
}

fn desktop_scene(state:&crate::desktop_ui::State,catalog:&crate::app_catalog::Catalog,message:&[u8]) {
    use crate::desktop_ui::{Target,MENU_W,MENU_H};
    let session=unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap()};
    let data=unsafe {core::slice::from_raw_parts_mut(session.back.base() as *mut u32,WIDTH*HEIGHT)};
    let font=&crate::ui_font::FONT;
    let mut canvas=Canvas::new(data,WIDTH,HEIGHT,WIDTH).unwrap();
    canvas.rect(0,0,800,600,0x000080);
    canvas.text(24,40,0xffffff,b"Xinyi desktop test",font);
    canvas.text(24,64,0xffffff,b"Drag title bars. Click to focus. Right-click to restore windows.",font);
    canvas.rect(0,0,800,24,0xc4c4c4);canvas.rect(0,22,800,2,0xffffff);
    let apps=crate::desktop_ui::State::apps_button();
    bevel(&mut canvas,apps.x,apps.y,apps.w,apps.h,state.apps_open||(state.capture==Some(Target::AppsButton)&&state.hit()==Some(Target::AppsButton)));
    canvas.text(apps.x+16,apps.y+2,0,b"Apps",font);canvas.text(96,5,0,message,font);
    for &id in &state.order {
        let window=state.windows[id];if !window.visible {continue;}let r=window.rect;
        canvas.rect(r.x+6,r.y+6,r.w,r.h,0x000040);bevel(&mut canvas,r.x,r.y,r.w,r.h,false);
        let title=window.title();canvas.rect(title.x,title.y,title.w,title.h,if state.focus==Some(id) {0x153e8e}else{0x707070});
        canvas.text(r.x+12,r.y+9,0xffffff,if id==0 {b"Buttons"}else{b"Welcome"},font);
        let close=window.close();let pressed=state.capture==Some(Target::Close(id))&&state.hit()==state.capture;
        bevel(&mut canvas,close.x,close.y,close.w,close.h,pressed);let d=if pressed {1}else{0};
        canvas.line((close.x+5+d,close.y+5+d),(close.x+12+d,close.y+12+d),0);
        canvas.line((close.x+12+d,close.y+5+d),(close.x+5+d,close.y+12+d),0);
        if id==0 {
            canvas.text(r.x+24,r.y+48,0,b"Each button counts its own clicks.",font);
            for i in 0..2 {
                let button=window.button(i);let pressed=state.capture==Some(Target::Button(i))&&state.hit()==state.capture;
                bevel(&mut canvas,button.x,button.y,button.w,button.h,pressed);let d=if pressed {1}else{0};
                canvas.text(button.x+32+d,button.y+10+d,0,if i==0 {b"Button 1"}else{b"Button 2"},font);
                canvas.text(button.x,button.y+60,0,if i==0 {b"Clicks 1:"}else{b"Clicks 2:"},font);
                let mut digits=[b'0';10];let mut value=state.clicks[i];
                for digit in digits.iter_mut().rev() {*digit=b'0'+(value%10) as u8;value/=10;}
                let first=digits.iter().position(|&b|b!=b'0').unwrap_or(9);
                canvas.text(button.x,button.y+80,0,&digits[first..],font);
            }
        }else {
            for (i,line) in [b"A small start for Xinyi's desktop." as &[u8],b"",b"Drag this title bar to move me.",b"Click a window to bring it forward.",b"Use X to close a window.",b"Right-click to restore both windows."].iter().enumerate() {
                canvas.text(r.x+16,r.y+48+i as i32*24,0,line,font);
            }
        }
    }
    if state.apps_open {
        let r=state.apps_menu();canvas.rect(r.x+6,r.y+6,r.w,r.h,0x000040);bevel(&mut canvas,r.x,r.y,r.w,r.h,false);
        if catalog.len==0 {canvas.text(r.x+12,r.y+8,0x707070,b"No applications",font);}
        for (i,entry) in catalog.entries[..catalog.len].iter().enumerate() {
            let y=r.y+4+i as i32*24;let hover=state.hit()==Some(Target::App(i));
            if hover {canvas.rect(r.x+4,y,r.w-8,24,0x153e8e);}
            canvas.text(r.x+12,y+4,if hover {0xffffff}else{0},&entry.label[..entry.label_len],font);
        }
    }
    if let Some((x,y))=state.menu {
        canvas.rect(x+6,y+6,MENU_W,MENU_H,0x000040);bevel(&mut canvas,x,y,MENU_W,MENU_H,false);
        canvas.rect(x+4,y+35,MENU_W-8,1,0x808080);
        for i in 0..2 {
            let top=y+4+i as i32*36;let hover=state.hit()==Some(Target::Menu(i));
            if hover {canvas.rect(x+4,top,MENU_W-8,28,0x000080);}
            canvas.text(x+12,top+6,if hover {0xffffff}else{0},if i==0 {b"Restore windows"}else{b"Close menu"},font);
        }
    }
}
fn desktop_damage(old:&crate::desktop_ui::State,new:&crate::desktop_ui::State) {
    use crate::desktop_ui::{Rect,Target,MENU_W,MENU_H};
    let present=|rect:Rect| {present_guarded(rect.x,rect.y,rect.w,rect.h,(new.x,new.y));};
    for id in 0..2 {
        let(a,b)=(old.windows[id],new.windows[id]);
        if a!=b {
            match (a.visible,b.visible) {(true,true)=>present(a.bounds().union(b.bounds())),(true,false)=>present(a.bounds()),(false,true)=>present(b.bounds()),_=>()}
        }
        if old.focus!=new.focus&&b.visible {present(b.title());}
    }
    if old.order!=new.order {let top=new.windows[new.order[1]];if top.visible {present(top.bounds());}}
    if old.clicks!=new.clicks&&new.windows[0].visible {let r=new.windows[0].rect;present(Rect{x:r.x+16,y:r.y+136,w:r.w-32,h:52});}
    // Pressed controls may change on mouse-down/up or when dragging away.
    if old.capture!=new.capture||old.hit()!=new.hit() {
        for state in [old,new] {if let Some(target)=state.capture {
            match target {
                Target::Button(i) if state.windows[0].visible=>present(state.windows[0].button(i)),
                Target::Close(id) if state.windows[id].visible=>present(state.windows[id].close()),
                _=>(),
            }
        }}
    }
    if old.apps_open!=new.apps_open||old.capture!=new.capture {present(crate::desktop_ui::State::apps_button());}
    // Repair an old popup even when it was dismissed, then present the new one
    // last so a control/title repair cannot draw through it.
    for state in [old,new] {if state.apps_open {let r=state.apps_menu();present(Rect{w:r.w+6,h:r.h+6,..r});}}
    // Menus are composited last. Re-present even an unchanged menu after a
    // window/control repair so nothing can paint over this topmost overlay.
    for (x,y) in [old.menu,new.menu].into_iter().flatten() {present(Rect{x,y,w:MENU_W+6,h:MENU_H+6});}
}
pub fn desktop_demo() {
    if !crate::mouse::init() {crate::println!("PS/2 mouse unavailable (or initialization timed out).");return;}
    let mut state=crate::desktop_ui::State::new(crate::mouse::discard());let mut message=[b' ';72];
    let initial=b"Apps launches programs. ESC returns to the shell.";message[..initial.len()].copy_from_slice(initial);
    loop {
        let mut catalog=crate::app_catalog::Catalog::new();
        let result=crate::diskfs::list("/APPS",|name,_,directory|catalog.add(name,directory));catalog.sort();state.app_count=catalog.len;
        if result.is_err() {message.fill(b' ');let text=b"Apps unavailable. ESC returns to shell for disks/use.";message[..text.len()].copy_from_slice(text);}
        else if catalog.truncated {message.fill(b' ');let text=b"Showing first 16 .BIN files in /APPS.";message[..text.len()].copy_from_slice(text);}
        let Some(index)=desktop_session(&mut state,&catalog,&message) else{return;};
        let Some(entry)=catalog.entries.get(index).filter(|_|index<catalog.len) else{return;};
        let name=core::str::from_utf8(&entry.path[..entry.path_len]).unwrap();
        crate::debug::write_fmt(format_args!("Desktop: launch {}\n",name));
        let ok=crate::launch_program(name,"");message.fill(b' ');
        let prefix=if ok {b"Returned from " as &[u8]}else{b"Cannot launch "};message[..prefix.len()].copy_from_slice(prefix);
        let end=prefix.len()+entry.label_len;message[prefix.len()..end].copy_from_slice(&entry.label[..entry.label_len]);
        let suffix=if ok {b". Apps to run another." as &[u8]}else{b"; ESC shows details."};message[end..end+suffix.len()].copy_from_slice(suffix);
    }
}
fn desktop_session(state:&mut crate::desktop_ui::State,catalog:&crate::app_catalog::Catalog,message:&[u8])->Option<usize> {
    if let Err(reason)=start() {crate::println!("{}",reason);return None;}
    state.buttons=crate::mouse::discard();state.capture=None;state.launch=None;state.menu=None;state.apps_open=false;
    desktop_scene(state,catalog,message);present_rect(0,0,800,600);pointer(state.x,state.y);
    crate::debug::write_fmt(format_args!("Desktop: backbuffer={:08x}\n",unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap().back.base()}));
    crate::debug::write_fmt(format_args!("Desktop: session apps={} pos={},{};{},{} clicks={}/{}\n",catalog.len,state.windows[0].rect.x,state.windows[0].rect.y,state.windows[1].rect.x,state.windows[1].rect.y,state.clicks[0],state.clicks[1]));
    crate::debug::write_str("Desktop: frame ready\n");
    'desktop: loop {
        let mut quit=false;
        for _ in 0..128 {if crate::keyboard::queued_scancodes()==0 {break;}
            if crate::keyboard::poll_key()==Some(crate::keyboard::Key::Char('\u{1b}')) {quit=true;break;}
        }
        if quit {break;}
        for _ in 0..128 {
            let Some(event)=crate::mouse::poll() else{break;};let old=*state;
            let mut changed=state.update(event);
            if !event.lost&&event.buttons==old.buttons {for _ in 0..63 {
                if !crate::mouse::motion_before(event.buttons,crate::keyboard::next_order()) {break;}
                changed|=state.update(crate::mouse::poll().unwrap());
            }}
            if state.launch.is_some() {break 'desktop;}
            if changed {desktop_scene(state,catalog,message);}
            move_pointer((old.x,old.y),(state.x,state.y));
            if changed {
                desktop_damage(&old,state);
                crate::debug::write_fmt(format_args!("Desktop: windows={}/{} top={} clicks={}/{} pos={},{};{},{} menu={} apps={}\n",state.windows[0].visible,state.windows[1].visible,state.focus.map_or(9,|i|i),state.clicks[0],state.clicks[1],state.windows[0].rect.x,state.windows[0].rect.y,state.windows[1].rect.x,state.windows[1].rect.y,state.menu.is_some(),state.apps_open));
            }
        }
        crate::idle::wait_for_pointer();
    }
    let launch=state.launch.take();crate::mouse::discard();restore();crate::debug::write_str("Desktop: text mode restored\n");
    if launch.is_none() {crate::println!("Returned to text mode.");}launch
}

pub(crate) fn app_scene(state:&crate::app_window_ui::State,title:&[u8],surface:&[u32]) {
    let session=unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap()};
    let data=unsafe {core::slice::from_raw_parts_mut(session.back.base() as *mut u32,WIDTH*HEIGHT)};
    let font=&crate::ui_font::FONT;let mut canvas=Canvas::new(data,WIDTH,HEIGHT,WIDTH).unwrap();
    canvas.rect(0,0,800,600,0x000080);canvas.rect(0,0,800,24,0xc4c4c4);canvas.rect(0,22,800,2,0xffffff);
    canvas.text(12,5,0,b"Xinyi | Foreground application",font);
    let r=state.window.rect;canvas.rect(r.x+6,r.y+6,r.w,r.h,0x000040);bevel(&mut canvas,r.x,r.y,r.w,r.h,false);
    let bar=state.window.title();canvas.rect(bar.x,bar.y,bar.w,bar.h,if state.focused {0x153e8e}else{0x707070});canvas.text(r.x+12,r.y+9,0xffffff,title,font);
    let close=state.window.close();bevel(&mut canvas,close.x,close.y,close.w,close.h,state.close_pressed());let d=if state.close_pressed() {1}else{0};
    canvas.line((close.x+5+d,close.y+5+d),(close.x+12+d,close.y+12+d),0);canvas.line((close.x+12+d,close.y+5+d),(close.x+5+d,close.y+12+d),0);
    drop(canvas);let client=state.client();
    copy_client(data,client,surface);
}

#[derive(Clone,Copy)]
pub(crate) struct WindowView<'a> {pub ui:crate::app_window_ui::State,pub title:&'a[u8],pub pixels:&'a[u32]}
pub(crate) fn multi_scene(views:&[Option<WindowView<'_>>;4],order:&[usize;4],shell:Option<&crate::desktop_shell::Shell>) {
    let session=unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap()};
    let data=unsafe {core::slice::from_raw_parts_mut(session.back.base() as *mut u32,WIDTH*HEIGHT)};let font=&crate::ui_font::FONT;
    {let mut c=Canvas::new(data,WIDTH,HEIGHT,WIDTH).unwrap();c.rect(0,0,800,600,0x000080);c.text(24,40,0xffffff,if shell.is_some() {b"Xinyi desktop"}else{b"Xinyi multitasking"},font);c.text(24,64,0xffffff,shell.map_or(b"Drag windows. ESC exits demo. Ctrl+Alt+K stops focused app." as &[u8],|s|&s.message[..s.length]),font);c.rect(0,0,800,24,0xc4c4c4);c.rect(0,22,800,2,0xffffff);c.text(12,5,0,b"Independent programs | Keyboard goes to the focused window",font);}
    for &index in order {let Some(view)=views[index] else{continue;};if !view.ui.window.visible {continue;}let r=view.ui.window.rect;
        {let mut c=Canvas::new(data,WIDTH,HEIGHT,WIDTH).unwrap();c.rect(r.x+6,r.y+6,r.w,r.h,0x000040);bevel(&mut c,r.x,r.y,r.w,r.h,false);let bar=view.ui.window.title();c.rect(bar.x,bar.y,bar.w,bar.h,if view.ui.focused {0x153e8e}else{0x707070});c.text(r.x+12,r.y+9,0xffffff,view.title,font);
        if view.ui.minimizable {let m=view.ui.window.minimize();bevel(&mut c,m.x,m.y,m.w,m.h,view.ui.minimize_pressed());c.rect(m.x+4,m.y+12,10,2,0);}
        let close=view.ui.window.close();bevel(&mut c,close.x,close.y,close.w,close.h,view.ui.close_pressed());c.line((close.x+5,close.y+5),(close.x+12,close.y+12),0);c.line((close.x+12,close.y+5),(close.x+5,close.y+12),0);}
        let client=view.ui.client();copy_client(data,client,view.pixels);
    }
    if let Some(shell)=shell {
        use crate::desktop_shell::{Shell,Hit};
        let mut c=Canvas::new(data,WIDTH,HEIGHT,WIDTH).unwrap();
        c.rect(0,0,800,24,0xc4c4c4);c.rect(0,22,800,2,0xffffff);
        let r=Shell::apps();bevel(&mut c,r.x,r.y,r.w,r.h,shell.open||shell.capture==Some(Hit::Apps));c.text(r.x+16,r.y+2,0,b"Apps",font);
        for (i,task) in shell.tasks.iter().enumerate() {if let Some(task)=task {let r=Shell::task(i);bevel(&mut c,r.x,r.y,r.w,r.h,shell.focused==Some(task.pid));c.text(r.x+6,r.y+2,0,&task.name[..task.length],font);}}
        let r=Shell::exit();bevel(&mut c,r.x,r.y,r.w,r.h,shell.capture==Some(Hit::Exit));c.text(r.x+8,r.y+2,0,b"Exit",font);
        if shell.open {let r=shell.menu();c.rect(r.x+6,r.y+6,r.w,r.h,0x000040);bevel(&mut c,r.x,r.y,r.w,r.h,false);
            if shell.catalog.len==0 {c.text(r.x+12,r.y+8,0,b"No apps found",font);}
            for(i,entry) in shell.catalog.entries[..shell.catalog.len].iter().enumerate() {let y=r.y+4+i as i32*24;let hover=shell.hit()==Some(Hit::App(i));if hover {c.rect(r.x+4,y,r.w-8,24,0x153e8e);}c.text(r.x+12,y+4,if hover {0xffffff}else{0},&entry.label[..entry.label_len],font);}
        }
    }

}

pub(crate) fn terminal_cells(pixels:&mut[u32],cells:&[crate::pty_model::Cell;2000],rows:u32,cursor:Option<(usize,usize)>) {
 let colors=[0x000000,0x0000aa,0x00aa00,0x00aaaa,0xaa0000,0xaa00aa,0xaa5500,0xaaaaaa,0x555555,0x5555ff,0x55ff55,0x55ffff,0xff5555,0xff55ff,0xffff55,0xffffff];
 let session=unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().unwrap()};let font=unsafe {&*(session.font.base() as *const [u8;4096])};let mut canvas=Canvas::new(pixels,640,400,640).unwrap();
 for y in 0..25 {if rows&(1<<y)==0 {continue;}for x in 0..80 {let cell=cells[y*80+x];canvas.rect(x as i32*8,y as i32*16,8,16,colors[(cell.color>>4) as usize]);canvas.glyph(x as i32*8,y as i32*16,colors[(cell.color&15) as usize],cell.ch,font);}}
 if let Some((x,y))=cursor {if rows&(1<<y)!=0 {canvas.rect(x as i32*8,y as i32*16+14,8,2,colors[(cells[y*80+x].color&15) as usize]);}}
}

pub(crate) fn font_address()->u32 {unsafe {(&*core::ptr::addr_of!(SESSION)).as_ref().map_or(0,|s|s.font.base())}}

fn copy_client(data:&mut[u32],r:crate::desktop_ui::Rect,pixels:&[u32]) {
 let left=r.x.max(0);let right=(r.x+r.w).min(800);let top=r.y.max(24);let bottom=(r.y+r.h).min(600);
 if left>=right||top>=bottom{return;}let n=(right-left) as usize;
 for y in top..bottom {let dst=y as usize*800+left as usize;let src=((y-r.y)*r.w+left-r.x) as usize;data[dst..dst+n].copy_from_slice(&pixels[src..src+n]);}
}
