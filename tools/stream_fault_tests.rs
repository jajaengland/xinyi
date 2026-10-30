use super::*;
#[test]
fn interrupted_range_edit_and_resize_preserve_complete_old_or_new_file() {
    let d = Disk::new(false);
    populated(&d);
    let original = d.bytes.borrow().clone();
    let mut states = 0;
    for operation in 0..3 {
        let run = |disk: &CrashDisk| {
            let f = Fat16::new(disk).unwrap();
            match operation {
                0 => f.write_at("docs/live.txt", 510, &[0x33; 700]),
                1 => f.resize_file("docs/live.txt", 1700),
                _ => f.resize_file("docs/live.txt", 17),
            }
        };
        let full = CrashDisk::new(&original, usize::MAX, false);
        run(&full).unwrap();
        let after = full.reboot();
        let expected_size = Fat16::new(&after)
            .unwrap()
            .file_size("docs/live.txt")
            .unwrap();
        let mut expected = vec![0; expected_size];
        Fat16::new(&after)
            .unwrap()
            .read_at("docs/live.txt", 0, &mut expected)
            .unwrap();
        for wb in [false, true] {
            for cut in 0..=full.boundary.get() {
                let disk = CrashDisk::new(&original, cut, wb);
                let _ = run(&disk);
                let reboot = disk.reboot();
                let r = audit(&reboot);
                states += 1;
                if r.repairable() {
                    repair(&reboot, r).unwrap();
                    let f = Fat16::new(&reboot).unwrap();
                    let mut data = vec![0; 2048];
                    let n = f.read_at("docs/live.txt", 0, &mut data).unwrap();
                    assert!(
                        (n == 900 && data[..n] == [0x5a; 900]) || data[..n] == expected,
                        "op={operation} wb={wb} cut={cut}"
                    );
                } else {
                    assert_eq!(repair(&ReadOnly { disk: &reboot }, r), Err(Error::Corrupt));
                }
            }
        }
    }
    assert!(states > 50);
    println!("checked {states} interrupted streaming states");
}
