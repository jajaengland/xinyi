// Included by storage_tests.rs to share its realistic FAT16 fixture.
use super::*;
use fat16::check::{self, Report, WORKSPACE_WORDS};
fn audit(d: &dyn BlockDevice) -> Report {
    check::scan(d, &mut vec![0; WORKSPACE_WORDS]).unwrap()
}
fn repair(d: &dyn BlockDevice, r: Report) -> Result<Report, Error> {
    check::repair_orphans(d, &mut vec![0; WORKSPACE_WORDS], r)
}
fn populated(d: &Disk) {
    let f = Fat16::new(d).unwrap();
    f.mkdir("docs").unwrap();
    f.create("docs/live.txt").unwrap();
    f.write("docs/live.txt", &[0x5a; 900]).unwrap();
}
#[test]
fn scan_is_read_only_on_clean_nested_partitioned_and_raw_volumes() {
    for partitioned in [false, true] {
        let d = Disk::new(partitioned);
        populated(&d);
        let before = d.bytes.borrow().clone();
        // A device that rejects every mutation proves scan never depends on writes/flushes.
        let ro = ReadOnly { disk: &d };
        let r = audit(&ro);
        assert!(r.clean(), "{r:?}");
        assert_eq!(r.files, 1);
        assert_eq!(r.directories, 2);
        assert_eq!(*d.bytes.borrow(), before);
    }
}
struct ReadOnly<'a> {
    disk: &'a Disk,
}
#[test]
fn single_fat_and_multi_sector_clusters_are_audited() {
    for single in [false, true] {
        let d = Disk::new(false);
        {
            let mut b = d.bytes.borrow_mut();
            b.resize(16384 * 512, 0);
            b[13] = 2;
            b[19..21].copy_from_slice(&16384u16.to_le_bytes());
            if single {
                b[16] = 1;
                b[33 * 512..65 * 512].fill(0);
            }
        }
        populated(&d);
        let r = audit(&ReadOnly { disk: &d });
        assert!(r.clean(), "{r:?}");
        assert_eq!(r.cluster_bytes, 1024);
    }
}
#[test]
fn torn_fat_writes_and_metadata_read_failures_refuse_cleanup() {
    let d = Disk::new(false);
    populated(&d);
    d.set_fat(700, 0xffff);
    // Only the low byte reached the second copy: neither side is a free entry.
    d.bytes.borrow_mut()[33 * 512 + 700 * 2] = 0;
    let r = audit(&d);
    assert!(!r.repairable());
    assert_eq!(repair(&ReadOnly { disk: &d }, r), Err(Error::Corrupt));
    struct Fault<'a>(&'a Disk, u32);
    impl BlockDevice for Fault<'_> {
        fn block_size(&self) -> usize {
            512
        }
        fn block_count(&self) -> u32 {
            self.0.block_count()
        }
        fn read_block(&self, b: u32, o: &mut [u8]) -> bool {
            b != self.1 && self.0.read_block(b, o)
        }
        fn write_block(&self, _: u32, _: &[u8]) -> bool {
            panic!("unexpected write")
        }
        fn flush(&self) -> bool {
            panic!("unexpected flush")
        }
    }
    for lba in [1, 33, 65, 67] {
        assert_eq!(
            check::scan(&Fault(&d, lba), &mut vec![0; WORKSPACE_WORDS]),
            Err(Error::Io)
        );
    }
}
impl BlockDevice for ReadOnly<'_> {
    fn block_count(&self) -> u32 {
        self.disk.block_count()
    }
    fn block_size(&self) -> usize {
        512
    }
    fn read_block(&self, b: u32, o: &mut [u8]) -> bool {
        self.disk.read_block(b, o)
    }
    fn write_block(&self, _: u32, _: &[u8]) -> bool {
        panic!("read-only scan wrote")
    }
    fn flush(&self) -> bool {
        panic!("read-only scan flushed")
    }
}
#[test]
fn orphan_cleanup_preserves_live_data_bad_markers_and_all_non_fat_sectors() {
    let d = Disk::new(false);
    populated(&d);
    d.set_fat(400, 401);
    d.set_fat(401, 400); // Lost cycle.
    d.set_fat(700, 0xffff);
    d.set_fat(800, 0xfff7);
    d.set_fat(801, 0xfff2);
    // Interrupted mirror update of a lost allocation, safe regardless of which copy was first.
    d.bytes.borrow_mut()[(33 * 512 + 700 * 2)..(33 * 512 + 700 * 2 + 2)].fill(0);
    let before = d.bytes.borrow().clone();
    let r = audit(&d);
    assert_eq!(r.orphans, 3);
    assert_eq!(r.mirrors, 1);
    assert!(r.repairable(), "{r:?}");
    assert!(repair(&d, r).unwrap().clean());
    let after = d.bytes.borrow();
    assert_eq!(&after[..512], &before[..512]);
    assert_eq!(&after[65 * 512..], &before[65 * 512..]);
    assert_eq!(
        &after[512 + 800 * 2..512 + 802 * 2],
        &before[512 + 800 * 2..512 + 802 * 2]
    );
    drop(after);
    let mut out = [0; 1024];
    assert_eq!(
        Fat16::new(&d).unwrap().read("docs/live.txt", &mut out),
        Ok(900)
    );
    assert_eq!(&out[..900], &[0x5a; 900]);
}
#[test]
fn damaged_live_namespace_and_ambiguous_mirrors_never_allow_repair() {
    for kind in 0..9 {
        let d = Disk::new(false);
        populated(&d);
        d.set_fat(700, 0xffff);
        match kind {
            0 => d.set_fat(3, 3),      // Live file loop.
            1 => d.set_fat(3, 0),      // Free cluster in a live chain.
            2 => d.set_fat(3, 0xffff), // Truncation vs file size.
            3 => d.bytes.borrow_mut()[33 * 512 + 3 * 2..33 * 512 + 3 * 2 + 2]
                .copy_from_slice(&0xffffu16.to_le_bytes()),
            4 => d.bytes.borrow_mut()[33 * 512 + 700 * 2..33 * 512 + 700 * 2 + 2]
                .copy_from_slice(&701u16.to_le_bytes()),
            5 => d.bytes.borrow_mut()[67 * 512] = b'X', // Missing '.' in child directory.
            6 => d.bytes.borrow_mut()[65 * 512 + 20] = 1, // FAT32 high start word in FAT16.
            7 => d.bytes.borrow_mut()[512] = 0, // Header damage must be reportable even if mount fails.
            _ => d.set_fat(700, 9000),          // Invalid lost link.
        }
        let before = d.bytes.borrow().clone();
        let r = audit(&d);
        assert!(!r.repairable(), "kind={kind} {r:?}");
        assert_eq!(repair(&ReadOnly { disk: &d }, r), Err(Error::Corrupt));
        assert_eq!(*d.bytes.borrow(), before);
    }
}
#[test]
fn crosslinks_depth_limits_read_failures_and_stale_previews_are_detected() {
    let d = Disk::new(false);
    let f = Fat16::new(&d).unwrap();
    f.create("a.txt").unwrap();
    f.write("a.txt", b"a").unwrap();
    f.create("b.txt").unwrap();
    f.write("b.txt", b"b").unwrap();
    let first = d.bytes.borrow()[65 * 512 + 26..65 * 512 + 28].to_vec();
    d.bytes.borrow_mut()[65 * 512 + 32 + 26..65 * 512 + 32 + 28].copy_from_slice(&first);
    assert!(!audit(&d).repairable());
    assert!(audit(&d).cross_links > 0);
    let d = Disk::new(false);
    d.set_fat(500, 0xffff);
    let r = audit(&d);
    d.set_fat(501, 0xffff);
    assert_eq!(repair(&ReadOnly { disk: &d }, r), Err(Error::DiskChanged));
    let mut small = [0; 4];
    assert_eq!(check::scan(&d, &mut small), Err(Error::OutOfMemory));
    struct Broken;
    impl BlockDevice for Broken {
        fn block_size(&self) -> usize {
            512
        }
        fn block_count(&self) -> u32 {
            8192
        }
        fn read_block(&self, _: u32, _: &mut [u8]) -> bool {
            false
        }
        fn write_block(&self, _: u32, _: &[u8]) -> bool {
            panic!()
        }
    }
    assert_eq!(
        check::scan(&Broken, &mut vec![0; WORKSPACE_WORDS]),
        Err(Error::Io)
    );
    // A valid chain longer than this OS's supported directory bound is incomplete, never reclaimable.
    let d = Disk::new(false);
    Fat16::new(&d).unwrap().mkdir("dir").unwrap();
    for c in 2..35 {
        d.set_fat(c, if c == 34 { 0xffff } else { (c + 1) as u16 });
    }
    assert!(audit(&d).limits > 0);
    assert!(!audit(&d).repairable());
}
// Sector-atomic fault model: fail permanently at each write OR flush boundary.
// In write-back mode only successful flushes survive a simulated power cut.
struct CrashDisk {
    live: RefCell<Vec<u8>>,
    durable: RefCell<Vec<u8>>,
    boundary: Cell<usize>,
    fail: usize,
    writeback: bool,
}
impl CrashDisk {
    fn new(bytes: &[u8], fail: usize, writeback: bool) -> Self {
        Self {
            live: RefCell::new(bytes.to_vec()),
            durable: RefCell::new(bytes.to_vec()),
            boundary: Cell::new(0),
            fail,
            writeback,
        }
    }
    fn pass(&self) -> bool {
        let n = self.boundary.get();
        self.boundary.set(n + 1);
        n < self.fail
    }
    fn reboot(&self) -> Disk {
        Disk {
            bytes: RefCell::new(self.durable.borrow().clone()),
            fail_after: Cell::new(None),
            fail_flush: Cell::new(false),
        }
    }
}
impl BlockDevice for CrashDisk {
    fn block_size(&self) -> usize {
        512
    }
    fn block_count(&self) -> u32 {
        (self.live.borrow().len() / 512) as u32
    }
    fn read_block(&self, b: u32, o: &mut [u8]) -> bool {
        let p = b as usize * 512;
        o.copy_from_slice(&self.live.borrow()[p..p + 512]);
        true
    }
    fn write_block(&self, b: u32, v: &[u8]) -> bool {
        if !self.pass() {
            return false;
        }
        let p = b as usize * 512;
        self.live.borrow_mut()[p..p + 512].copy_from_slice(v);
        if !self.writeback {
            self.durable.borrow_mut()[p..p + 512].copy_from_slice(v);
        }
        true
    }
    fn flush(&self) -> bool {
        if !self.pass() {
            return false;
        }
        *self.durable.borrow_mut() = self.live.borrow().clone();
        true
    }
}
#[test]
fn every_interrupted_cleanup_boundary_is_resumable() {
    let d = Disk::new(false);
    populated(&d);
    for c in [400, 700, 900] {
        d.set_fat(c, 0xffff);
    }
    let bytes = d.bytes.borrow().clone();
    let preview = audit(&d);
    let full = CrashDisk::new(&bytes, usize::MAX, false);
    repair(&full, preview).unwrap();
    let boundaries = full.boundary.get();
    assert_eq!(boundaries, 7);
    for wb in [false, true] {
        for cut in 0..=boundaries {
            let disk = CrashDisk::new(&bytes, cut, wb);
            let _ = repair(&disk, preview);
            let reboot = disk.reboot();
            let r = audit(&reboot);
            assert!(r.repairable(), "wb={wb} cut={cut} {r:?}");
            assert!(repair(&reboot, r).unwrap().clean());
            let mut out = [0; 1024];
            assert_eq!(
                Fat16::new(&reboot).unwrap().read("docs/live.txt", &mut out),
                Ok(900)
            );
            assert_eq!(&out[..900], &[0x5a; 900]);
        }
    }
}
#[test]
fn interrupted_file_operations_are_detected_without_guessing_live_metadata() {
    let d = Disk::new(false);
    populated(&d);
    let bytes = d.bytes.borrow().clone();
    let mut cases = 0;
    for operation in 0..7 {
        let run = |disk: &CrashDisk| {
            let f = Fat16::new(disk).unwrap();
            match operation {
                0 => f.write("docs/live.txt", &[0x33; 1024]),
                1 => f.delete("docs/live.txt"),
                2 => f.mkdir("other"),
                3 => f.create("new.txt"),
                4 => f.copy_file("docs/live.txt", "docs/copy.txt", &mut [0; 1024]),
                5 => f.move_file("docs/live.txt", "moved.txt", &mut [0; 1024]),
                _ => f.rename("docs/live.txt", "docs/renamed.txt"),
            }
        };
        let full = CrashDisk::new(&bytes, usize::MAX, false);
        run(&full).unwrap();
        let boundaries = full.boundary.get();
        for wb in [false, true] {
            for cut in 0..=boundaries {
                let disk = CrashDisk::new(&bytes, cut, wb);
                let _ = run(&disk);
                let reboot = disk.reboot();
                let r = audit(&reboot);
                cases += 1;
                if r.repairable() {
                    repair(&reboot, r).unwrap();
                    let f = Fat16::new(&reboot).unwrap();
                    let mut out = [0; 1024];
                    let mut complete = 0;
                    for name in [
                        "docs/live.txt",
                        "docs/copy.txt",
                        "moved.txt",
                        "docs/renamed.txt",
                    ] {
                        match f.read(name, &mut out) {
                            Ok(n) => {
                                let original = n == 900 && out[..n] == [0x5a; 900];
                                let replaced =
                                    operation == 0 && n == 1024 && out[..n] == [0x33; 1024];
                                assert!(
                                    original
                                        || replaced
                                        || (n == 0 && operation >= 4 && name != "docs/live.txt")
                                );
                                if original || replaced {
                                    complete += 1;
                                }
                            }
                            Err(e) => assert_eq!(e, Error::NotFound),
                        }
                    }
                    assert!(
                        complete > 0 || operation == 1,
                        "lost live data: op={operation} cut={cut} wb={wb}"
                    );
                } else {
                    assert_eq!(repair(&ReadOnly { disk: &reboot }, r), Err(Error::Corrupt));
                }
            }
        }
    }
    assert!(cases > 50);
    println!("checked {cases} interrupted operation states");
}
