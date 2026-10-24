// Simple in-memory filesystem for basic file operations.

use crate::{print, println};

#[derive(Clone, Copy)]
pub struct File {
    pub name: [u8; 32],
    pub name_len: usize,
    pub data: [u8; 1024],
    pub size: usize,
    pub used: bool,
}

const MAX_FILES: usize = 32;

pub static mut FILES: [File; MAX_FILES] = [File {
    name: [0; 32],
    name_len: 0,
    data: [0; 1024],
    size: 0,
    used: false,
}; MAX_FILES];

pub fn init() {
    unsafe {
        for file in FILES.iter_mut().take(MAX_FILES) {
            file.used = false;
            file.size = 0;
        }
    }
}

pub fn create(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 31 {
        return false;
    }

    unsafe {
        for file in FILES.iter_mut().take(MAX_FILES) {
            if !file.used {
                for (i, &b) in bytes.iter().enumerate() {
                    file.name[i] = b;
                }
                file.name_len = bytes.len();
                file.used = true;
                file.size = 0;
                return true;
            }
        }
    }
    false
}

pub fn write(name: &str, data: &[u8]) -> bool {
    let bytes = name.as_bytes();
    unsafe {
        for file in FILES.iter_mut().take(MAX_FILES) {
            if file.used && file.name_len == bytes.len() {
                let mut match_name = true;
                for i in 0..bytes.len() {
                    if file.name[i] != bytes[i] {
                        match_name = false;
                        break;
                    }
                }
                if match_name {
                    let len = data.len().min(1024);
                    for i in 0..len {
                        file.data[i] = data[i];
                    }
                    file.size = len;
                    return true;
                }
            }
        }
    }
    false
}

pub fn read(name: &str) -> Option<([u8; 1024], usize)> {
    let bytes = name.as_bytes();
    unsafe {
        for file in FILES.iter().take(MAX_FILES) {
            if file.used && file.name_len == bytes.len() {
                let mut match_name = true;
                for i in 0..bytes.len() {
                    if file.name[i] != bytes[i] {
                        match_name = false;
                        break;
                    }
                }
                if match_name {
                    return Some((file.data, file.size));
                }
            }
        }
    }
    None
}

pub fn list() -> usize {
    let mut count = 0;
    unsafe {
        for file in FILES.iter().take(MAX_FILES) {
            if file.used {
                count += 1;
            }
        }
    }
    count
}
