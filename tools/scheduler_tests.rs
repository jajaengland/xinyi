#[path="../src/scheduler_model.rs"] mod scheduler_model;
use scheduler_model::{State, due, runnable};
#[test] fn sleep_deadlines_survive_tick_wrap() {
    let deadline=2;
    for now in [u32::MAX-1,u32::MAX,0,1] {assert!(!due(now,deadline));}
    for now in [2,3,100] {assert!(due(now,deadline));}
    assert!(!due(0xfffffff0,0xfffffff1));
    assert!(due(0,0xfffffff1));
}
#[test] fn only_ready_due_or_signalled_jobs_run() {
    assert!(runnable(State::Ready,0,false));
    assert!(!runnable(State::Sleep(20),19,true));
    assert!(runnable(State::Sleep(20),20,false));
    assert!(!runnable(State::Waiting,100,false));
    assert!(runnable(State::Waiting,100,true));
}
