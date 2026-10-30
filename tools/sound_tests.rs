#[path="../src/sound_model.rs"] mod sound_model;
#[path="../src/wav.rs"] mod wav;
use sound_model::*;
#[test]
fn events_are_bounded_and_notification_aliases_info() {
    for (id,e) in Event::ALL.into_iter().enumerate() {
        assert_eq!(Event::from_id(id as u32),Some(e));
        assert_eq!(Event::parse(e.name()),Some(e));
    }
    assert_eq!(Event::parse("notification"),Some(Event::Info));
    assert_eq!(Event::from_id(9),None);
    assert_eq!(Event::from_id(u32::MAX),None);
}
#[test]
fn voices_do_not_stack_and_muted_or_busy_requests_are_not_queued() {
    let mut p=Policy::new();
    assert!(!p.accept(0,true));
    assert!(p.accept(0,false));
    assert!(!p.accept(74,false));
    assert!(p.accept(75,false));
    p.volume=0;
    assert!(!p.accept(150,false));
    p.volume=60;
    assert!(p.accept(150,false));
    let mut p=Policy::new();
    assert!(p.accept(u32::MAX-20,false));
    assert!(!p.accept(20,false));
    assert!(p.accept(100,false));
}
#[test]
fn prepared_assets_match_the_kernel_decoder_and_are_short_and_audible() {
    let assets: [&[u8];9]=[
        include_bytes!("../assets/sounds/startup.wav"),include_bytes!("../assets/sounds/shutdown.wav"),
        include_bytes!("../assets/sounds/error.wav"),include_bytes!("../assets/sounds/warning.wav"),
        include_bytes!("../assets/sounds/info.wav"),include_bytes!("../assets/sounds/complete.wav"),
        include_bytes!("../assets/sounds/connect.wav"),include_bytes!("../assets/sounds/disconnect.wav"),
        include_bytes!("../assets/sounds/empty.wav")];
    for b in assets {
        let format=wav::parse(b.len() as u32,|p,out| {
            let Some(s)=b.get(p as usize..p as usize+out.len()) else{return false;};
            out.copy_from_slice(s);true
        }).unwrap();
        assert_eq!(format.rate,16000);
        assert_eq!(format.channels,1);
        assert_eq!(format.bits,16);
        assert_eq!(&b[36..40],b"data");
        assert!(b.len()>3244 && b.len()<32044);
        let samples:Vec<i16>=b[44..].chunks_exact(2).map(|c|i16::from_le_bytes([c[0],c[1]])).collect();
        assert!(samples.iter().any(|v|*v>5000));
        assert!(samples.iter().any(|v|*v< -5000));
        assert!(samples.iter().all(|v|v.unsigned_abs()<30000));
    }
}
