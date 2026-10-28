use crate::{
    diskfs,
    fat16::{Error, MAX_FILE_SIZE},
    user_memory as user_buffer, usermode,
};
pub(crate) fn error(error: Error) -> u32 {
    (match error {
        Error::OutOfMemory => -12,
        Error::ProgramTooLarge => -27,
        Error::NotFound => -2,
        Error::ReadOnly => -13,
        Error::TooLarge => -27,
        Error::Exists => -17,
        Error::NotFile => -21,
        Error::NotDirectory => -20,
        Error::DirectoryNotEmpty => -39,
        Error::DirectoryBusy => -16,
        Error::Full => -28,
        Error::InvalidName | Error::PathTooDeep => -22,
        _ => -5i32,
    }) as u32
}
pub fn syscall(cs: u32, number: u32, first: u32, second: u32, third: u32) -> u32 {
    let Some(image_size) = usermode::active_image(cs) else {
        return u32::MAX;
    };
    if number == 59 {
        let (args, length) = usermode::arguments();
        if second < length as u32 {
            return (-22i32) as u32;
        }
        if !user_buffer::writable(first, length as u32) {
            return (-14i32) as u32;
        }
        if length != 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(args.as_ptr(), first as *mut u8, length);
            }
        }
        return length as u32;
    }
    if !diskfs::is_mounted() {
        return (-19i32) as u32;
    }
    let path_length = if number == 58 {
        second & 0xffff
    } else {
        second
    };
    let length = second >> 16;
    if path_length == 0
        || path_length > crate::fs_path::MAX_PATH as u32
        || length > MAX_FILE_SIZE as u32
    {
        return (-22i32) as u32;
    }
    if !user_buffer::readable(first, path_length, image_size) {
        return (-14i32) as u32;
    }
    let mut path = [0; crate::fs_path::MAX_PATH];
    unsafe {
        core::ptr::copy_nonoverlapping(first as *const u8, path.as_mut_ptr(), path_length as usize);
    }
    let Ok(path) = core::str::from_utf8(&path[..path_length as usize]) else {
        return (-22i32) as u32;
    };
    let mut data = [0; MAX_FILE_SIZE];
    if number == 57 {
        if !user_buffer::writable(third, MAX_FILE_SIZE as u32) {
            return (-14i32) as u32;
        }
        unsafe {
            core::arch::asm!("sti", options(nostack));
        }
        let result = diskfs::read(path, &mut data);
        unsafe {
            core::arch::asm!("cli", options(nostack));
        }
        match result {
            Ok(size) => {
                if size != 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(data.as_ptr(), third as *mut u8, size);
                    }
                }
                size as u32
            }
            Err(e) => error(e),
        }
    } else {
        if !user_buffer::readable(third, length, image_size) {
            return (-14i32) as u32;
        }
        if length != 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    third as *const u8,
                    data.as_mut_ptr(),
                    length as usize,
                );
            }
        }
        unsafe {
            core::arch::asm!("sti", options(nostack));
        }
        let result = match diskfs::write(path, &data[..length as usize]) {
            Err(Error::NotFound) => {
                diskfs::create(path).and_then(|_| diskfs::write(path, &data[..length as usize]))
            }
            result => result,
        };
        unsafe {
            core::arch::asm!("cli", options(nostack));
        }
        match result {
            Ok(()) => length,
            Err(e) => error(e),
        }
    }
}

#[repr(C)]
struct FileRequest {
    path: u32,
    path_length: u32,
    data: u32,
    length: u32,
    volume: u32,
}
pub fn large(cs: u32, number: u32, pointer: u32, size: u32, reserved: u32) -> u32 {
    let Some(image) = usermode::active_image(cs) else {
        return u32::MAX;
    };
    if size != 20 || reserved != 0 {
        return (-22i32) as u32;
    }
    if !user_buffer::readable(pointer, 20, image) {
        return (-14i32) as u32;
    }
    let r = unsafe { core::ptr::read_unaligned(pointer as *const FileRequest) };
    if r.path_length == 0
        || r.path_length > 105
        || r.length > 65536
        || (number == 90 && (r.data != 0 || r.length != 0))
    {
        return (-22i32) as u32;
    }
    if !diskfs::is_mounted() {
        return (-19i32) as u32;
    }
    if r.volume != diskfs::volume_epoch() {
        return (-116i32) as u32;
    }
    if !user_buffer::readable(r.path, r.path_length, image) {
        return (-14i32) as u32;
    }
    let mut path = [0; 105];
    unsafe {
        core::ptr::copy_nonoverlapping(
            r.path as *const u8,
            path.as_mut_ptr(),
            r.path_length as usize,
        );
    }
    if !path[..r.path_length as usize]
        .iter()
        .all(|b| (32..=126).contains(b))
    {
        return (-22i32) as u32;
    }
    let path = core::str::from_utf8(&path[..r.path_length as usize]).unwrap();
    if number == 90 {
        unsafe {
            core::arch::asm!("sti", options(nostack));
        }
        let result = diskfs::file_size(path);
        unsafe {
            core::arch::asm!("cli", options(nostack));
        }
        return match result {
            Ok(n) if n <= 65536 => n as u32,
            Ok(_) => (-27i32) as u32,
            Err(e) => error(e),
        };
    }
    if !(if number == 88 {
        user_buffer::writable(r.data, r.length)
    } else {
        user_buffer::readable_range(r.data, r.length, image)
    }) {
        return (-14i32) as u32;
    }
    let Some(frames) = crate::loader::Frames::new(16) else {
        return (-12i32) as u32;
    };
    let buffer = unsafe { core::slice::from_raw_parts_mut(frames.base() as *mut u8, 65536) };
    if number == 89 && r.length != 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(
                r.data as *const u8,
                buffer.as_mut_ptr(),
                r.length as usize,
            );
        }
    }
    unsafe {
        core::arch::asm!("sti", options(nostack));
    }
    let result = if number == 88 {
        diskfs::file_size(path).and_then(|n| {
            if n > 65536 {
                Err(Error::ProgramTooLarge)
            } else if n > r.length as usize {
                Err(Error::Full)
            } else {
                diskfs::read_program(path, buffer)
            }
        })
    } else {
        diskfs::save_large(path, &buffer[..r.length as usize]).map(|_| r.length as usize)
    };
    unsafe {
        core::arch::asm!("cli", options(nostack));
    }
    match result {
        Ok(n) => {
            if number == 88 && n != 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(buffer.as_ptr(), r.data as *mut u8, n);
                }
            }
            n as u32
        }
        Err(e) => error(e),
    }
}
