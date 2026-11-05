use core::sync::atomic::{AtomicU32,Ordering};
static UI_SCALE:AtomicU32=AtomicU32::new(100);
pub fn requested_scale()->u16 {UI_SCALE.load(Ordering::Relaxed) as u16}
static REQUESTED:AtomicU32=AtomicU32::new((800<<16)|600);
pub fn requested()->(u16,u16) {let v=REQUESTED.load(Ordering::Relaxed);((v>>16) as u16,v as u16)}
pub fn load() {
    let mut scale=[0;5];
    if crate::diskfs::file_size("/UISCALE.CFG")==Ok(5)&&crate::diskfs::read_at("/UISCALE.CFG",0,&mut scale)==Ok(5) {
        if let Some(percent)=crate::display_layout::decode_scale(&scale) {UI_SCALE.store(percent as u32,Ordering::Relaxed);}
    }
    let mut data=[0;8];if crate::diskfs::file_size("/DISPLAY.CFG")==Ok(8) && crate::diskfs::read_at("/DISPLAY.CFG",0,&mut data)==Ok(8) {
        if let Some((w,h))=crate::display_layout::decode(&data) {REQUESTED.store((w as u32)<<16|h as u32,Ordering::Relaxed);}
    }
}
pub fn command(argument:&str) {
    let argument=argument.trim();
    if argument.is_empty() {
        let (w,h)=requested();if crate::graphics::active() {crate::println!("Active graphics: {}x{}",crate::display_layout::physical_width(),crate::display_layout::physical_height());}crate::println!("Display preference: {}x{}",w,h);
        crate::println!("Modes: 800x600, 1024x768, 1280x720, 1280x1024");
        crate::println!("Use display WIDTHxHEIGHT; restart the desktop to apply.");return;
    }
    let size=argument.split_once('x').and_then(|(w,h)|Some((w.parse::<u16>().ok()?,h.parse::<u16>().ok()?)));
    let Some((w,h))=size.filter(|&(w,h)|crate::display_layout::supported(w,h)) else {crate::println!("Unsupported size. Type display to list supported preferences.");return;};
    match crate::diskfs::save_large("/DISPLAY.CFG",&crate::display_layout::encode(w,h)) {
        Ok(())=>{REQUESTED.store((w as u32)<<16|h as u32,Ordering::Relaxed);crate::println!("Saved {}x{}. Restart the desktop or reboot to apply.",w,h);},
        Err(e)=>crate::println!("Display preference was not saved: {}",e.message()),
    }
}

pub fn scale_command(argument:&str) {
    let argument=argument.trim().trim_end_matches('%');
    if argument.is_empty() {
        crate::println!("UI scale preference: {}%",requested_scale());
        if crate::graphics::active() {crate::println!("Active: {}%, logical desktop {}x{}",crate::display_layout::scale(),crate::display_layout::width(),crate::display_layout::height());}
        crate::println!("Use uiscale 100, 125 or 150; restart the desktop to apply.");
        crate::println!("Scaling requires at least 800x600 logical pixels.");return;
    }
    let Some(percent)=argument.parse::<u16>().ok().filter(|&n|crate::display_layout::valid_scale(n)) else {crate::println!("Unsupported UI scale. Use 100, 125 or 150.");return;};
    let (w,h)=requested();
    if crate::display_layout::logical_size(w,h,percent).is_none() {crate::println!("That scale leaves too little workspace at {}x{}. Choose a larger display mode first.",w,h);return;}
    let record=crate::display_layout::encode_scale(percent);
    match crate::diskfs::save_large("/UISCALE.CFG",&record) {
        Ok(())=>{UI_SCALE.store(percent as u32,Ordering::Relaxed);crate::println!("Saved UI scale {}%. Restart the desktop or reboot to apply.",percent);},
        Err(e)=>crate::println!("UI scale was not saved: {}",e.message()),
    }
}
