//! AC'97 descriptor ownership. Never reuse a slot the device may still read.
pub const SLOTS: usize = 32;
pub const FRAMES: usize = 2048;
pub const BYTES: usize = FRAMES * 4;
pub struct Ring {
    pub head: usize,
    pub tail: usize,
    pub count: usize,
    pub frames: u32,
    lengths: [u16; SLOTS],
}
impl Ring {
    pub const fn new() -> Self {
        Self {
            head: 0,
            tail: 0,
            count: 0,
            frames: 0,
            lengths: [0; SLOTS],
        }
    }
    pub fn available(&self) -> bool {
        self.count < SLOTS - 1
    }
    pub fn push(&mut self, frames: u16) -> Option<usize> {
        if frames == 0 || frames as usize > FRAMES || !self.available() {
            return None;
        }
        let index = self.tail;
        self.lengths[index] = frames;
        self.frames += frames as u32;
        self.tail = (self.tail + 1) % SLOTS;
        self.count += 1;
        Some(index)
    }
    fn pop(&mut self) {
        self.frames -= self.lengths[self.head] as u32;
        self.lengths[self.head] = 0;
        self.head = (self.head + 1) % SLOTS;
        self.count -= 1;
    }
    pub fn retire(&mut self, civ: usize, last_finished: bool) -> bool {
        if civ >= SLOTS {
            return false;
        }
        if self.count == 0 {
            return true;
        }
        let distance = (civ + SLOTS - self.head) % SLOTS;
        if distance >= self.count {
            return false;
        }
        for _ in 0..distance {
            self.pop();
        }
        if last_finished && self.count == 1 && self.head == civ {
            self.pop();
        }
        true
    }
}
