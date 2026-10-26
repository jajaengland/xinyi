// A real elapsed-time deadline, with a separate bound if timer ticks stop.
pub struct Budget {
    start: u32,
    last: u32,
    stalled: u32,
    ticks: u32,
}
impl Budget {
    pub fn new(now: u32, ticks: u32) -> Self {
        Self {
            start: now,
            last: now,
            stalled: 0,
            ticks,
        }
    }
    pub fn expired(&mut self, now: u32) -> bool {
        if now.wrapping_sub(self.start) >= self.ticks {
            return true;
        }
        if now == self.last {
            self.stalled += 1;
        } else {
            self.last = now;
            self.stalled = 0;
        }
        self.stalled >= 10_000_000
    }
}
