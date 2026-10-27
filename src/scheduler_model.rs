#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Ready,
    Sleep(u32),
    Waiting,
}
pub fn due(now: u32, deadline: u32) -> bool {
    (now.wrapping_sub(deadline) as i32) >= 0
}
pub fn runnable(state: State, now: u32, event: bool) -> bool {
    match state {
        State::Ready => true,
        State::Sleep(t) => due(now, t),
        State::Waiting => event,
    }
}
