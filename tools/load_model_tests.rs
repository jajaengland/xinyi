#[path="../src/load_model.rs"]mod load_model;
use load_model::*;
#[test]fn complete_reads_publish_only_to_a_valid_destination(){
    assert_eq!(ready(false,3,3,true,true,true),Ready::Publish);
    assert_eq!(ready(true,3,3,true,true,true),Ready::Cancelled);
    assert_eq!(ready(false,3,4,true,true,true),Ready::StaleVolume);
    assert_eq!(ready(false,3,3,false,true,true),Ready::ClosedParent);
    assert_eq!(ready(false,3,3,true,false,true),Ready::MissingParent);
    assert_eq!(ready(false,3,3,true,true,false),Ready::NoRoom);
}
#[test]fn simultaneous_failures_cannot_turn_into_publication(){
    for cancelled in [false,true] {for same in [false,true] {for open in [false,true] {for alive in [false,true] {for room in [false,true] {
        assert_eq!(ready(cancelled,1,if same{1}else{2},open,alive,room)==Ready::Publish,!cancelled&&same&&open&&alive&&room);
    }}}}}
}
