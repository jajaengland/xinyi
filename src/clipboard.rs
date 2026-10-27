// Shared bounded ASCII text clipboard. Kernel calls complete before switching.
pub const LIMIT: usize = 1024;
pub struct Clipboard {
    data: [u8; LIMIT],
    length: usize,
}
impl Clipboard {
    pub const fn new() -> Self {
        Self {
            data: [0; LIMIT],
            length: 0,
        }
    }
    pub fn set(&mut self, text: &[u8]) -> bool {
        if text.len() > LIMIT || !text.iter().all(|b| matches!(*b, 9 | 10 | 13 | 32..=126)) {
            return false;
        }
        self.data.fill(0);
        self.data[..text.len()].copy_from_slice(text);
        self.length = text.len();
        true
    }
    pub fn bytes(&self) -> &[u8] {
        &self.data[..self.length]
    }
}
static mut CLIPBOARD: Clipboard = Clipboard::new();
pub fn set_text(text:&[u8])->bool {unsafe {(&mut *core::ptr::addr_of_mut!(CLIPBOARD)).set(text)}}
#[cfg(not(test))]
pub fn syscall(cs: u32, number: u32, pointer: u32, length: u32, reserved: u32) -> u32 {
    let Some(image) = crate::usermode::active_image(cs) else {
        return u32::MAX;
    };
    if length > LIMIT as u32 || reserved != 0 {
        return (-22i32) as u32;
    }
    if number == 67 {
        if !crate::user_buffer::readable(pointer, length, image) {
            return (-14i32) as u32;
        }
        let mut text = [0; LIMIT];
        if length != 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    pointer as *const u8,
                    text.as_mut_ptr(),
                    length as usize,
                );
            }
        }
        if unsafe { (&mut *core::ptr::addr_of_mut!(CLIPBOARD)).set(&text[..length as usize]) } {
            length
        } else {
            (-22i32) as u32
        }
    } else {
        let data = unsafe { (&*core::ptr::addr_of!(CLIPBOARD)).bytes() };
        if length == 0 {
            return data.len() as u32;
        }
        if (length as usize) < data.len() {
            return (-28i32) as u32;
        }
        if !crate::user_buffer::writable(pointer, data.len() as u32) {
            return (-14i32) as u32;
        }
        if !data.is_empty() {
            unsafe {
                core::ptr::copy_nonoverlapping(data.as_ptr(), pointer as *mut u8, data.len());
            }
        }
        data.len() as u32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_updates_are_atomic() {
        let mut c = Clipboard::new();
        assert!(c.set(b"hello\nworld"));
        assert!(!c.set(&[0; 2]));
        assert!(!c.set(&[b'a'; LIMIT + 1]));
        assert_eq!(c.bytes(), b"hello\nworld");
    }
    #[test]
    fn full_copy_and_clear() {
        let mut c = Clipboard::new();
        assert!(c.set(&[b'a'; LIMIT]));
        assert_eq!(c.bytes().len(), LIMIT);
        assert!(c.set(b""));
        assert!(c.bytes().is_empty());
    }
}
