//! Bounded positional file ABI. No persistent kernel handles or raw disk access.
pub const MAX_FILE: usize = 1024 * 1024;
pub const MAX_CHUNK: usize = 16 * 1024;
pub const MAX_CLUSTERS: usize = MAX_FILE / 512;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Request {
    pub path: u32,
    pub path_length: u32,
    pub data: u32,
    pub length: u32,
    pub volume: u32,
    pub offset: u32,
}
impl Request {
    pub fn valid(&self, number: u32) -> bool {
        if !(1..=105).contains(&self.path_length) {
            return false;
        }
        match number {
            91 | 92 => {
                self.length <= MAX_CHUNK as u32
                    && self.offset.checked_add(self.length).is_some()
                    && (number == 91 || self.offset + self.length <= MAX_FILE as u32)
            }
            93 => self.data == 0 && self.length == 0 && self.offset == 0,
            94 => self.data == 0 && self.offset == 0 && self.length <= MAX_FILE as u32,
            _ => false,
        }
    }
}
