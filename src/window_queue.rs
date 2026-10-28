use crate::window_protocol as api;
pub struct Queue {
    events: [api::Event; 32],
    head: usize,
    len: usize,
    last_motion: bool,
}
impl Queue {
    pub fn new() -> Self {
        Self {
            events: [api::Event::default(); 32],
            head: 0,
            len: 0,
            last_motion: false,
        }
    }
    pub fn pending(&self) -> bool {
        self.len != 0
    }
    pub fn push(&mut self, event: api::Event, motion: bool) {
        let motion = motion && event.kind == api::POINTER;
        if motion && self.last_motion && self.len != 0 {
            let index = (self.head + self.len - 1) % 32;
            let previous = self.events[index];
            if previous.kind == api::POINTER && previous.buttons == event.buttons && previous.code==event.code {
                self.events[index] = event;
                return;
            }
        }
        if self.len == 32 {
            self.head = 0;
            self.len = 1;
            self.events[0] = api::Event {
                kind: api::RESET,
                buttons: event.buttons,
                ..api::Event::default()
            };
        }
        self.events[(self.head + self.len) % 32] = event;
        self.len += 1;
        self.last_motion = motion;
    }
    pub fn pop(&mut self) -> Option<api::Event> {
        if self.len == 0 {
            return None;
        }
        let event = self.events[self.head];
        self.head = (self.head + 1) % 32;
        self.len -= 1;
        Some(event)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pointer(x: i32, buttons: u32) -> api::Event {
        api::Event {
            kind: api::POINTER,
            x,
            buttons,
            ..api::Event::default()
        }
    }
    #[test]
    fn coalescing_preserves_press_release_and_keys() {
        let mut q = Queue::new();
        q.push(pointer(0, 1), false);
        for x in 1..100 {
            q.push(pointer(x, 1), true);
        }
        q.push(
            api::Event {
                kind: api::KEY,
                ..api::Event::default()
            },
            false,
        );
        q.push(pointer(100, 1), true);
        q.push(pointer(101, 1), true);
        q.push(pointer(101, 0), false);
        assert_eq!(q.pop().unwrap().x, 0);
        assert_eq!(q.pop().unwrap().x, 99);
        assert_eq!(q.pop().unwrap().kind, api::KEY);
        assert_eq!(q.pop().unwrap().x, 101);
        assert_eq!(q.pop().unwrap().buttons, 0);
        assert!(!q.pending());
    }
    #[test]
    fn modifier_transitions_are_not_coalesced_away() {
        let mut q = Queue::new();
        q.push(pointer(1,0),true);
        let mut shifted=pointer(2,0);shifted.code=1<<16;
        q.push(shifted,true);
        shifted.x=3;q.push(shifted,true);
        q.push(pointer(4,0),true);
        assert_eq!(q.pop().unwrap().x,1);
        let e=q.pop().unwrap();assert_eq!(e.x,3);assert_eq!(e.code,1<<16);
        assert_eq!(q.pop().unwrap().x,4);
        assert!(!q.pending());
    }
    #[test]
    fn overflow_resets_without_stuck_capture() {
        let mut q = Queue::new();
        for _ in 0..33 {
            q.push(
                api::Event {
                    kind: api::KEY,
                    ..api::Event::default()
                },
                false,
            );
        }
        assert_eq!(q.pop().unwrap().kind, api::RESET);
        assert_eq!(q.pop().unwrap().kind, api::KEY);
    }
}
