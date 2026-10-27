// Standard three-byte PS/2 stream packets (defaults selected with F6).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Event {
    pub dx: i16,
    pub dy: i16,
    pub buttons: u8,
    pub lost: bool,
}
pub struct Decoder {
    bytes: [u8; 3],
    length: usize,
}
impl Decoder {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 3],
            length: 0,
        }
    }
    pub fn reset(&mut self) {
        self.length = 0;
    }
    pub fn feed(&mut self, byte: u8) -> Option<Event> {
        if self.length == 0 && byte & 8 == 0 {
            return None;
        }
        self.bytes[self.length] = byte;
        self.length += 1;
        if self.length != 3 {
            return None;
        }
        self.length = 0;
        let flags = self.bytes[0];
        // The sign lives in the header: a positive displacement can exceed 127.
        let x = self.bytes[1] as i16 - if flags & 0x10 != 0 { 256 } else { 0 };
        let y = self.bytes[2] as i16 - if flags & 0x20 != 0 { 256 } else { 0 };
        Some(Event {
            dx: if flags & 0x40 != 0 { 0 } else { x },
            dy: if flags & 0x80 != 0 { 0 } else { -y },
            buttons: flags & 7,
            lost: false,
        })
    }
}
