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

#[test] fn closing_a_root_compacts_only_taskbar_slots_in_existing_order() {
    let mut slots = [Some(0), None, Some(2), Some(3), None, None, None, None];
    scheduler_model::compact_roots(&mut slots);
    assert_eq!(slots, [Some(0), None, Some(1), Some(2), None, None, None, None]);
    slots[0] = None;
    scheduler_model::compact_roots(&mut slots);
    assert_eq!(slots, [None, None, Some(0), Some(1), None, None, None, None]);
    // A newly launched root is appended; process-array holes stay untouched.
    slots[0] = Some(2);
    scheduler_model::compact_roots(&mut slots);
    assert_eq!(slots, [Some(2), None, Some(0), Some(1), None, None, None, None]);
}
