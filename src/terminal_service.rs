// Directory and application-launch services for desktop programs.
pub fn syscall(cs: u32, number: u32, first: u32, second: u32, third: u32) -> u32 {
    let Some(image) = crate::usermode::active_image(cs) else {
        return u32::MAX;
    };
    if number == 69 {
        return (-38i32) as u32;
    }
    if number == 70 {
        if third != 0 || second < 105 {
            return (-22i32) as u32;
        }
        if !crate::user_buffer::writable(first, 105) {
            return (-14i32) as u32;
        }
        let mut path = [0; 105];
        let n = crate::diskfs::pwd(&mut path);
        unsafe {
            core::ptr::copy_nonoverlapping(path.as_ptr(), first as *mut u8, n);
        }
        return n as u32;
    }
    if second > 128 || (number == 71 && third != 0) {
        return (-22i32) as u32;
    }
    if !crate::user_buffer::readable(first, second, image) {
        return (-14i32) as u32;
    }
    let mut line = [0; 128];
    if second != 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(first as *const u8, line.as_mut_ptr(), second as usize);
        }
    }
    if !line[..second as usize]
        .iter()
        .all(|b| (32..=126).contains(b))
    {
        return (-22i32) as u32;
    }
    let line = core::str::from_utf8(&line[..second as usize]).unwrap();
    if number == 71 {
        return match crate::multitask::request_launch(line) {
            Ok(()) => 0,
            Err(e) => e as u32,
        };
    }
    (-38i32) as u32
}
