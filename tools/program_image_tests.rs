#![allow(dead_code)]
#[path = "../src/elf.rs"]
mod elf;
#[path = "../src/program_image.rs"]
mod program_image;
use program_image::*;
fn put(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn image() -> Vec<u8> {
    let mut b = vec![0; 200000];
    b[..7].copy_from_slice(b"\x7fELF\x01\x01\x01");
    b[16] = 2;
    b[18] = 3;
    put(&mut b, 20, 1);
    put(&mut b, 24, elf::BASE);
    put(&mut b, 28, 52);
    b[40] = 52;
    b[42] = 32;
    b[44] = 1;
    for (i, v) in [1, 4096, elf::BASE, 0, 195904, 220000, 5, 4096]
        .iter()
        .enumerate()
    {
        put(&mut b, 52 + i * 4, *v);
    }
    b
}
#[test]
fn larger_files_use_elf_validation_while_flat_limit_is_unchanged() {
    let layout = parse(&image()).unwrap();
    assert_eq!(layout.span, 220000);
    let mut boundary=image();boundary.resize(MAX_FILE,0);
    assert!(parse(&boundary).is_ok());boundary.push(0);
    assert!(matches!(parse(&boundary),Err(Error::TooLarge)));
    assert!(parse(&vec![0x90; MAX_FLAT]).is_ok());
    assert!(matches!(
        parse(&vec![0x90; MAX_FLAT + 1]),
        Err(Error::FlatTooLarge)
    ));
    assert!(matches!(parse(&[]), Err(Error::Empty)));
    assert!(matches!(
        parse(&vec![0; MAX_FILE + 1]),
        Err(Error::TooLarge)
    ));
}
#[test]
fn oversized_truncated_overlapping_and_writable_executable_images_are_rejected() {
    let b = image();
    assert!(matches!(
        parse(&b[..b.len() - 1]),
        Err(Error::InvalidElf(_))
    ));
    for (at, value) in [
        (52 + 24, 7),
        (52 + 20, 0x100001),
        (52 + 4, u32::MAX),
        (24, elf::HEAP_BASE),
        (52 + 16, 220001),
    ] {
        let mut x = b.clone();
        put(&mut x, at, value);
        assert!(
            matches!(parse(&x), Err(Error::InvalidElf(_))),
            "{at}/{value}"
        );
    }
    assert!(matches!(parse(b"\x7fELF"), Err(Error::InvalidElf(_))));
}
