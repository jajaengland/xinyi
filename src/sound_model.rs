//! Small, bounded policy shared by kernel system sounds and host tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Event {
    Startup,
    Shutdown,
    Error,
    Warning,
    Info,
    Complete,
    Connect,
    Disconnect,
    Empty,
}
impl Event {
    pub const ALL: [Self; 9] = [
        Self::Startup,
        Self::Shutdown,
        Self::Error,
        Self::Warning,
        Self::Info,
        Self::Complete,
        Self::Connect,
        Self::Disconnect,
        Self::Empty,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Shutdown => "shutdown",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Complete => "complete",
            Self::Connect => "connect",
            Self::Disconnect => "disconnect",
            Self::Empty => "empty",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        if s == "notification" {
            return Some(Self::Info);
        }
        Self::ALL.into_iter().find(|e| e.name() == s)
    }
    pub fn from_id(id: u32) -> Option<Self> {
        Self::ALL.get(id as usize).copied()
    }
}
pub struct Policy {
    pub volume: u32,
    last: Option<u32>,
}
impl Policy {
    pub const fn new() -> Self {
        Self {
            volume: 60,
            last: None,
        }
    }
    pub fn accept(&mut self, now: u32, busy: bool) -> bool {
        if busy || self.volume == 0 || self.last.is_some_and(|n| now.wrapping_sub(n) < 75) {
            return false;
        }
        self.last = Some(now);
        true
    }
}
