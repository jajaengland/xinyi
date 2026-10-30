use super::*;
use fat16::check::{scan, WORKSPACE_WORDS};
use stream_protocol::{Request, MAX_CHUNK, MAX_FILE};
#[test]
fn kernel_image_copy_supports_large_apps_without_changing_userspace_whole_file_limit() {
    let d=Disk::new(false);let f=Fat16::new(&d).unwrap();f.mkdir("apps").unwrap();
    let payload:Vec<u8>=(0..160_017).map(|i|(i%251) as u8).collect();
    f.save_image("apps/large.elf",&payload).unwrap();
    let mut output=vec![0;MAX_FILE];assert_eq!(f.read_image("apps/large.elf",&mut output),Ok(payload.len()));assert_eq!(&output[..payload.len()],&payload);
    assert_eq!(f.read_program("apps/large.elf",&mut output),Err(Error::ProgramTooLarge));
    let mut short=[0xa5;17];assert_eq!(f.read_image("apps/large.elf",&mut short),Err(Error::TooLarge));assert_eq!(short,[0xa5;17]);
    f.save_image("apps/large.elf",&payload[..70001]).unwrap();
    assert_eq!(f.read_image("apps/large.elf",&mut output),Ok(70001));assert_eq!(&output[..70001],&payload[..70001]);
    assert!(scan(&d,&mut vec![0;WORKSPACE_WORDS]).unwrap().clean());
    let before=d.bytes.borrow().clone();assert_eq!(f.save_image("apps/large.elf",&vec![0;MAX_FILE+1]),Err(Error::ImageTooLarge));assert_eq!(*d.bytes.borrow(),before);
}
#[test]
fn range_protocol_bounds_include_overflow_and_reserved_fields() {
    assert_eq!(std::mem::size_of::<Request>(), 24);
    let r = Request {
        path_length: 1,
        length: MAX_CHUNK as u32,
        ..Request::default()
    };
    assert!(r.valid(91));
    assert!(r.valid(92));
    assert!(!r.valid(93));
    for x in [
        Request {
            path_length: 0,
            ..r
        },
        Request {
            path_length: 106,
            ..r
        },
        Request {
            offset: u32::MAX,
            ..r
        },
        Request {
            length: MAX_CHUNK as u32 + 1,
            ..r
        },
    ] {
        assert!(!x.valid(91));
        assert!(!x.valid(92));
    }
    assert!(!Request {
        offset: MAX_FILE as u32,
        length: 1,
        ..r
    }
    .valid(92));
    assert!(Request {
        length: MAX_FILE as u32,
        ..r
    }
    .valid(94));
    assert!(!Request {
        length: MAX_FILE as u32 + 1,
        ..r
    }
    .valid(94));
    assert!(!Request {
        length: 0,
        data: 1,
        ..r
    }
    .valid(93));
}
#[test]
fn chunked_edits_cross_sectors_clusters_and_old_whole_file_limit() {
    for partitioned in [false, true] {
        let d = Disk::new(partitioned);
        let f = Fat16::new(&d).unwrap();
        f.mkdir("docs").unwrap();
        let mut expected = vec![0; 180_001];
        assert_eq!(
            f.resize_file("docs/large.bin", expected.len() as u32),
            Ok(())
        );
        for (offset, length, value) in [
            (0, 7, 0x11),
            (509, 1100, 0x22),
            (65530, 27, 0x33),
            (100001, MAX_CHUNK, 0x44),
            (179990, 11, 0x55),
        ] {
            let data = vec![value; length];
            f.write_at("docs/large.bin", offset as u32, &data).unwrap();
            expected[offset..offset + length].copy_from_slice(&data);
        }
        let f = Fat16::new(&d).unwrap();
        assert_eq!(f.file_size("docs/large.bin"), Ok(expected.len()));
        let mut actual = Vec::new();
        let mut buffer = [0; 997];
        loop {
            let n = f
                .read_at("docs/large.bin", actual.len() as u32, &mut buffer)
                .unwrap();
            if n == 0 {
                break;
            }
            actual.extend_from_slice(&buffer[..n]);
        }
        assert_eq!(actual, expected);
        assert_eq!(
            f.read_program("docs/large.bin", &mut vec![0; 65536]),
            Err(Error::ProgramTooLarge)
        );
        assert!(scan(&d, &mut vec![0; WORKSPACE_WORDS]).unwrap().clean());
        f.rename("docs/large.bin", "docs/new.bin").unwrap();
        f.delete("docs/new.bin").unwrap();
        assert!(scan(&d, &mut vec![0; WORKSPACE_WORDS]).unwrap().clean());
    }
}
#[test]
fn growth_shrink_zero_fill_empty_noop_and_maximum_file() {
    let d = Disk::new(false);
    let f = Fat16::new(&d).unwrap();
    f.write_at("sparse.bin", 1000, b"abc").unwrap();
    let mut b = [0x55; 1010];
    assert_eq!(f.read_at("sparse.bin", 0, &mut b), Ok(1003));
    assert!(b[..1000].iter().all(|&v| v == 0));
    assert_eq!(&b[1000..1003], b"abc");
    assert_eq!(b[1003], 0x55);
    f.resize_file("sparse.bin", 1001).unwrap();
    f.resize_file("sparse.bin", 1500).unwrap();
    assert_eq!(f.read_at("sparse.bin", 1000, &mut b), Ok(500));
    assert_eq!(b[0], b'a');
    assert!(b[1..500].iter().all(|&v| v == 0));
    let before = d.bytes.borrow().clone();
    f.write_at("sparse.bin", 5000, &[]).unwrap();
    assert_eq!(*d.bytes.borrow(), before);
    f.resize_file("sparse.bin", 0).unwrap();
    assert_eq!(f.file_size("sparse.bin"), Ok(0));
    f.resize_file("sparse.bin", MAX_FILE as u32).unwrap();
    f.write_at("sparse.bin", MAX_FILE as u32 - 1, b"Z").unwrap();
    assert_eq!(
        f.read_at("sparse.bin", MAX_FILE as u32 - 1, &mut b[..3]),
        Ok(1)
    );
    assert_eq!(b[0], b'Z');
    let before = d.bytes.borrow().clone();
    assert_eq!(
        f.write_at("sparse.bin", MAX_FILE as u32, b"X"),
        Err(Error::ProgramTooLarge)
    );
    assert_eq!(
        f.resize_file("sparse.bin", MAX_FILE as u32 + 1),
        Err(Error::ProgramTooLarge)
    );
    assert_eq!(*d.bytes.borrow(), before);
    f.delete("sparse.bin").unwrap();
    assert!(scan(&d, &mut vec![0; WORKSPACE_WORDS]).unwrap().clean());
}
#[test]
fn range_reads_and_mutations_validate_chains_and_readonly_before_copy_or_write() {
    let d = Disk::new(false);
    let f = Fat16::new(&d).unwrap();
    f.write_at("a.bin", 0, b"abc").unwrap();
    d.set_fat(2, 2);
    let before = d.bytes.borrow().clone();
    let mut out = [0xa5; 5];
    assert_eq!(f.read_at("a.bin", 0, &mut out), Err(Error::Corrupt));
    assert_eq!(out, [0xa5; 5]);
    assert_eq!(f.write_at("a.bin", 1, b"z"), Err(Error::Corrupt));
    assert_eq!(*d.bytes.borrow(), before);
    d.set_fat(2, 0xffff);
    d.bytes.borrow_mut()[65 * 512 + 11] |= 1;
    let before = d.bytes.borrow().clone();
    assert_eq!(f.write_at("a.bin", 0, b"z"), Err(Error::ReadOnly));
    assert_eq!(f.resize_file("a.bin", 0), Err(Error::ReadOnly));
    assert_eq!(*d.bytes.borrow(), before);
}
