use crate::{directory_protocol::FileClipboard, fs_path::Path, user_memory as user_buffer};
static mut CLIP: FileClipboard = FileClipboard::new();
static mut NEXT: u32 = 1;
pub fn syscall(cs: u32, op: u32, pointer: u32, size: u32) -> u32 {
    let Some(image) = crate::usermode::active_image(cs) else {
        return u32::MAX;
    };
    if size != 3468 || op > 2 {
        return (-22i32) as u32;
    }
    if op == 1 {
        if !user_buffer::writable(pointer, 3468) {
            return (-14i32) as u32;
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                core::ptr::addr_of!(CLIP) as *const u8,
                pointer as *mut u8,
                3468,
            );
        }
        return 0;
    }
    // This fixed-size request is larger than the text API's 1 KiB limit.
    if !user_buffer::readable_range(pointer, 3468, image) {
        return (-14i32) as u32;
    }
    let request = unsafe { core::ptr::read_unaligned(pointer as *const FileClipboard) };
    if op == 2 && request.version != unsafe { CLIP.version } {return (-116i32) as u32;}
    set(request)
}
pub(crate) fn snapshot()->FileClipboard {unsafe {CLIP}}
pub(crate) fn set(mut request:FileClipboard)->u32 {
    if request.count>32||request.cut>1{return (-22i32) as u32;}
    if request.count>0&&(!crate::diskfs::is_mounted()||request.volume!=crate::diskfs::volume_epoch()){return (-116i32) as u32;}
    if let Err(e)=validate(&mut request){return e;}
    unsafe {if NEXT>i32::MAX as u32{return (-24i32) as u32;}request.version=NEXT;NEXT+=1;CLIP=request;request.version}
}

pub(crate) fn validate(request: &mut FileClipboard) -> Result<(), u32> {
    for i in 0..request.count as usize {
        if request.flags[i] > 1 {
            return Err((-22i32) as u32);
        }
        let n = request.lengths[i] as usize;
        if n == 0 || n > 105 {
            return Err((-22i32) as u32);
        }
        let text = &request.paths[i][..n];
        if text[0] != b'/' || !text.iter().all(|b| (32..=126).contains(b)) {
            return Err((-22i32) as u32);
        }
        let Ok(path) = Path::parse(core::str::from_utf8(text).unwrap(), &Path::ROOT) else {
            return Err((-22i32) as u32);
        };
        if path.depth == 0 {
            return Err((-22i32) as u32);
        }
        let mut encoded = [0; 105];
        let len = path.encode(&mut encoded);
        request.paths[i] = encoded;
        request.lengths[i] = len as u16;
        for j in 0..i {
            let other = Path::parse(
                core::str::from_utf8(&request.paths[j][..request.lengths[j] as usize]).unwrap(),
                &Path::ROOT,
            )
            .unwrap();
            if path.contains(&other) || other.contains(&path) {
                return Err((-22i32) as u32);
            }
        }
    }
    for i in request.count as usize..32 {
        request.paths[i] = [0; 105];
        request.lengths[i] = 0;
        request.flags[i] = 0;
    }
    Ok(())
}
