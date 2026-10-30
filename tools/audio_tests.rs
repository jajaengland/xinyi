#[path = "../src/audio_model.rs"]
mod audio_model;
#[path = "../src/wav.rs"]
mod wav;
use audio_model::*;
fn wave() -> Vec<u8> {
    let mut b = vec![0; 52];
    b[..4].copy_from_slice(b"RIFF");
    b[4..8].copy_from_slice(&44u32.to_le_bytes());
    b[8..16].copy_from_slice(b"WAVEfmt ");
    b[16..20].copy_from_slice(&16u32.to_le_bytes());
    b[20..22].copy_from_slice(&1u16.to_le_bytes());
    b[22..24].copy_from_slice(&2u16.to_le_bytes());
    b[24..28].copy_from_slice(&48000u32.to_le_bytes());
    b[28..32].copy_from_slice(&192000u32.to_le_bytes());
    b[32..34].copy_from_slice(&4u16.to_le_bytes());
    b[34..36].copy_from_slice(&16u16.to_le_bytes());
    b[36..40].copy_from_slice(b"data");
    b[40..44].copy_from_slice(&8u32.to_le_bytes());
    b[44..].copy_from_slice(&[0, 0x80, 0xff, 0x7f, 0, 0, 0, 0]);
    b
}
fn parse(b: &[u8]) -> Result<wav::Format, wav::Error> {
    wav::parse(b.len() as u32, |p, out| {
        let p = p as usize;
        match b.get(p..p + out.len()) {
            Some(s) => {
                out.copy_from_slice(s);
                true
            }
            None => false,
        }
    })
}
#[test]
fn ring_never_reuses_live_dma_and_handles_wrap_and_final_halt() {
    let mut r = Ring::new();
    assert!(r.push(0).is_none());
    assert!(r.push((FRAMES + 1) as u16).is_none());
    for i in 0..31 {
        assert_eq!(r.push(2048), Some(i));
    }
    assert!(!r.available());
    assert!(r.push(10).is_none());
    assert!(r.retire(17, false));
    assert_eq!(r.count, 14);
    for _ in 0..17 {
        r.push(100).unwrap();
    }
    assert_eq!(r.count, 31);
    assert!(!r.retire(16, false));
    assert!(r.retire(15, true));
    assert_eq!(r.count, 0);
    assert_eq!(r.frames, 0);
    assert_eq!(r.push(3), Some(16));
    assert!(r.retire(16, false));
    assert_eq!(r.count, 1);
    assert!(r.retire(16, true));
    assert_eq!(r.frames, 0);
}
#[test]
fn wav_format_and_samples_are_bounded_and_correct() {
    let b = wave();
    let f = parse(&b).unwrap();
    assert_eq!(
        (f.rate, f.channels, f.bits, f.align, f.data, f.bytes),
        (48000, 2, 16, 4, 44, 8)
    );
    assert_eq!(wav::sample(f, &b[44..48], 0), i16::MIN);
    assert_eq!(wav::sample(f, &b[44..48], 1), i16::MAX);
    let mut m = f;
    m.channels = 1;
    m.bits = 8;
    m.align = 1;
    assert_eq!(wav::sample(m, &[0], 0), i16::MIN);
    assert_eq!(wav::sample(m, &[128], 1), 0);
    assert_eq!(wav::sample(m, &[255], 1), 32512);
    for n in 0..b.len() {
        assert!(parse(&b[..n]).is_err());
    }
    for (at, value) in [
        (4, u32::MAX),
        (16, u32::MAX),
        (24, 0),
        (28, 1),
        (40, u32::MAX),
        (40, 7),
    ] {
        let mut x = b.clone();
        x[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(parse(&x).is_err(), "{at}/{value}");
    }
    let mut x = b.clone();
    x[20] = 3;
    assert_eq!(parse(&x), Err(wav::Error::Unsupported));
    assert_eq!(wav::parse(100, |_, _| false), Err(wav::Error::Read));
}
#[test]
fn arbitrary_riff_mutations_never_panic_or_escape_declared_file() {
    let mut seed = 42u32;
    for _ in 0..3000 {
        let mut b = wave();
        for _ in 0..4 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let at = (seed as usize) % b.len();
            b[at] = (seed >> 16) as u8;
        }
        let _ = parse(&b);
    }
    let mut b = wave();
    b.splice(36..36, b"JUNK\x01\x00\x00\x00x\x00".iter().copied());
    let n = b.len() as u32 - 8;
    b[4..8].copy_from_slice(&n.to_le_bytes());
    assert_eq!(parse(&b).unwrap().data, 54);
}
