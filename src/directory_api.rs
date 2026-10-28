use crate::{
    directory_protocol::{Entry, Listing},
    diskfs,
    files_api::error,
    user_memory as user_buffer,
};
pub fn syscall(cs: u32, n: u32, a: u32, b: u32, c: u32) -> u32 {
    let Some(image) = crate::usermode::active_image(cs) else {
        return u32::MAX;
    };
    if !diskfs::is_mounted() {
        return (-19i32) as u32;
    }
    if n == 82 {
        return if a | b | c == 0 {
            diskfs::volume_epoch()
        } else {
            (-22i32) as u32
        };
    }
    if n == 81 {
        use crate::directory_protocol::Transfer;
        if b != 220 || c != 0 {
            return (-22i32) as u32;
        }
        if !user_buffer::readable(a, 220, image) {
            return (-14i32) as u32;
        }
        let request = unsafe { core::ptr::read_unaligned(a as *const Transfer) };
        if request.volume != diskfs::volume_epoch() {
            return (-116i32) as u32;
        }
        let (x, y) = (
            request.source_length as usize,
            request.destination_length as usize,
        );
        if x == 0
            || x > 105
            || y > 105
            || request.operation > 5
            || (request.operation < 3 && y == 0)
            || (request.operation >= 3 && y != 0)
        {
            return (-22i32) as u32;
        }
        if !request.source[..x]
            .iter()
            .chain(request.destination[..y].iter())
            .all(|b| (32..=126).contains(b))
        {
            return (-22i32) as u32;
        }
        let source = core::str::from_utf8(&request.source[..x]).unwrap();
        let destination = core::str::from_utf8(&request.destination[..y]).unwrap();
        unsafe {
            core::arch::asm!("sti", options(nostack));
        }
        let result = match request.operation {
            3 => diskfs::mkdir(source),
            4 => diskfs::delete(source),
            5 => diskfs::rmdir(source),
            _ => diskfs::transfer(source, destination, request.operation as u32),
        };
        unsafe {
            core::arch::asm!("cli", options(nostack));
        }
        return match result {
            Ok(()) => 0,
            Err(e) => error(e),
        };
    }
    let length = if n == 78 { b & 0xffff } else { b };
    if length == 0 || length > 105 || n == 79 && c != 0 || n == 80 && c > 2 {
        return (-22i32) as u32;
    }
    if !user_buffer::readable(a, length, image) {
        return (-14i32) as u32;
    }
    if n == 78 && !user_buffer::writable(c, 776) {
        return (-14i32) as u32;
    }
    let mut path = [0; 105];
    unsafe {
        core::ptr::copy_nonoverlapping(a as *const u8, path.as_mut_ptr(), length as usize);
    }
    let Ok(path) = core::str::from_utf8(&path[..length as usize]) else {
        return (-22i32) as u32;
    };
    if !path.bytes().all(|b| (32..=126).contains(&b)) {
        return (-22i32) as u32;
    }
    unsafe {
        core::arch::asm!("sti", options(nostack));
    }
    let mut listing = Listing::new();
    let result = match n {
        78 => diskfs::list(path, |name, size, dir| {
            if name[0] == b'.' {
                return;
            }
            let offset = b >> 16;
            let at = listing.total;
            listing.total += 1;
            if at >= offset && listing.count < 32 {
                let mut text = [0; 12];
                let len = crate::fat16::display_name(name, &mut text);
                for ch in &mut text[..len] {
                    if !(32..=126).contains(ch) {
                        *ch = b'?';
                    }
                }
                listing.entries[listing.count as usize] = Entry {
                    name: text,
                    length: len as u32,
                    size: size as u32,
                    flags: dir as u32,
                };
                listing.count += 1;
            }
        }),
        79 => diskfs::cd(path),
        80 => match c {
            0 => diskfs::mkdir(path),
            1 => diskfs::delete(path),
            _ => diskfs::rmdir(path),
        },
        _ => unreachable!(),
    };
    unsafe {
        core::arch::asm!("cli", options(nostack));
    }
    match result {
        Err(e) => error(e),
        Ok(()) => {
            if n == 78 {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &listing as *const Listing as *const u8,
                        c as *mut u8,
                        776,
                    );
                }
            }
            0
        }
    }
}
