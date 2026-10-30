//! Decisions made only after a pending executable read has finished.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Ready { Publish, Cancelled, StaleVolume, ClosedParent, MissingParent, NoRoom }
pub fn ready(cancelled:bool,volume:u32,current:u32,parent_open:bool,parent_alive:bool,room:bool)->Ready {
    if cancelled {Ready::Cancelled}
    else if volume!=current {Ready::StaleVolume}
    else if !parent_open {Ready::ClosedParent}
    else if !parent_alive {Ready::MissingParent}
    else if !room {Ready::NoRoom}
    else {Ready::Publish}
}
