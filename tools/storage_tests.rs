// Run with rustc --edition=2021 --test tools/storage_tests.rs -o target/storage-tests.exe
#[path = "../src/ata_capacity.rs"]
mod ata_capacity;
#[path = "../src/ata_poll.rs"]
mod ata_poll;
#[path = "../src/blockdev.rs"]
mod blockdev;
#[path = "../src/bootdisk.rs"]
mod bootdisk;
#[path = "../src/disk_target.rs"]
mod disk_target;
#[path = "../src/fat16.rs"]
mod fat16;
#[path = "../src/fs_path.rs"]
mod fs_path;
#[path = "folder_tests.rs"]
mod folder_tests;
#[path = "../src/pci.rs"]
mod pci;
#[path = "../src/vfs.rs"]
mod vfs;
use blockdev::{BlockDevice, RamDisk};
use fat16::{format_disk, Error, Fat16, FormatPlan};
use std::cell::{Cell, RefCell};

#[test]
fn installer_program_writes_replace_large_images_without_leaking_clusters() {
    let disk=Disk::new(false);let fs=Fat16::new(&disk).unwrap();
    fs.mkdir("apps").unwrap();fs.create("apps/edit.bin").unwrap();
    let input=vec![0x5a;65536];fs.write_program("apps/edit.bin",&input).unwrap();
    let mut output=vec![0;65536];assert_eq!(fs.read_program("apps/edit.bin",&mut output),Ok(65536));assert_eq!(input,output);
    fs.write_program("apps/edit.bin",&[0x33;5000]).unwrap();
    assert_eq!(fs.read_program("apps/edit.bin",&mut output),Ok(5000));assert!(output[..5000].iter().all(|&b|b==0x33));
    fs.delete("apps/edit.bin").unwrap();fs.rmdir("apps").unwrap();
    assert!(disk.bytes.borrow()[512+4..33*512].iter().all(|&b|b==0));
}

#[test]
fn program_reader_accepts_64k_fragmented_chains_and_rejects_corruption_before_copy() {
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    fs.create("large.bin").unwrap();
    let root = 65 * 512;
    {
        let mut bytes=disk.bytes.borrow_mut();
        bytes[root+26..root+28].copy_from_slice(&2u16.to_le_bytes());
        bytes[root+28..root+32].copy_from_slice(&65536u32.to_le_bytes());
    }
    for i in 0..128 {
        let cluster=2+i*2;
        disk.set_fat(cluster, if i==127 {0xffff} else {(cluster+2) as u16});
        disk.bytes.borrow_mut()[(67+cluster-2)*512..(68+cluster-2)*512].fill(i as u8);
    }
    let mut output=vec![0;65536];
    assert_eq!(fs.read_program("large.bin",&mut output),Ok(65536));
    for i in 0..128 {assert!(output[i*512..(i+1)*512].iter().all(|&v|v==i as u8));}
    assert_eq!(fs.read("large.bin",&mut [0;1024]),Err(Error::TooLarge));
    let original=disk.bytes.borrow().clone();
    output.fill(0xaa);
    disk.set_fat(256,2); // A cycle after the final required cluster is invalid.
    assert_eq!(fs.read_program("large.bin",&mut output),Err(Error::Corrupt));
    assert!(output.iter().all(|&v|v==0xaa));
    *disk.bytes.borrow_mut()=original.clone();
    disk.bytes.borrow_mut()[root+28..root+32].copy_from_slice(&65537u32.to_le_bytes());
    assert_eq!(fs.read_program("large.bin",&mut output),Err(Error::ProgramTooLarge));
    assert!(output.iter().all(|&v|v==0xaa));
    *disk.bytes.borrow_mut()=original;
    fs.delete("large.bin").unwrap();
    for i in 0..128 {
        let at=512+(2+i*2)*2;
        assert_eq!(&disk.bytes.borrow()[at..at+2], &[0,0]);
        assert_eq!(&disk.bytes.borrow()[at+32*512..at+32*512+2], &[0,0]);
    }
}

struct Disk {
    bytes: RefCell<Vec<u8>>,
    fail_after: Cell<Option<usize>>,
    fail_flush: Cell<bool>,
}
impl Disk {
    fn new(partitioned: bool) -> Self {
        let start = if partitioned { 63 } else { 0 };
        let mut bytes = vec![0; (8192 + start) * 512];
        if partitioned {
            bytes[450] = 6;
            bytes[454..458].copy_from_slice(&(start as u32).to_le_bytes());
            bytes[458..462].copy_from_slice(&8192u32.to_le_bytes());
            bytes[510..512].copy_from_slice(&[0x55, 0xaa]);
        }
        let p = start * 512;
        bytes[p + 11..p + 13].copy_from_slice(&512u16.to_le_bytes());
        bytes[p + 13] = 1;
        bytes[p + 14..p + 16].copy_from_slice(&1u16.to_le_bytes());
        bytes[p + 16] = 2;
        bytes[p + 17..p + 19].copy_from_slice(&32u16.to_le_bytes());
        bytes[p + 19..p + 21].copy_from_slice(&8192u16.to_le_bytes());
        bytes[p + 21] = 0xf8;
        bytes[p + 22..p + 24].copy_from_slice(&32u16.to_le_bytes());
        bytes[p + 510..p + 512].copy_from_slice(&[0x55, 0xaa]);
        for copy in 0..2 {
            let at = p + (1 + copy * 32) * 512;
            bytes[at..at + 4].copy_from_slice(&[0xf8, 0xff, 0xff, 0xff]);
        }
        Self {
            bytes: RefCell::new(bytes),
            fail_after: Cell::new(None),
            fail_flush: Cell::new(false),
        }
    }
    fn set_fat(&self, cluster: usize, value: u16) {
        for copy in 0..2 {
            let at = (1 + copy * 32) * 512 + cluster * 2;
            self.bytes.borrow_mut()[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
}
impl BlockDevice for Disk {
    fn read_block(&self, block: u32, out: &mut [u8]) -> bool {
        if block >= self.block_count() || out.len() < 512 {
            return false;
        }
        let p = block as usize * 512;
        out[..512].copy_from_slice(&self.bytes.borrow()[p..p + 512]);
        true
    }
    fn write_block(&self, block: u32, data: &[u8]) -> bool {
        if block >= self.block_count() || data.len() < 512 {
            return false;
        }
        if let Some(left) = self.fail_after.get() {
            if left == 0 {
                return false;
            }
            self.fail_after.set(Some(left - 1));
        }
        let p = block as usize * 512;
        self.bytes.borrow_mut()[p..p + 512].copy_from_slice(&data[..512]);
        true
    }
    fn block_size(&self) -> usize {
        512
    }
    fn block_count(&self) -> u32 {
        (self.bytes.borrow().len() / 512) as u32
    }
    fn flush(&self) -> bool {
        !self.fail_flush.get()
    }
}

#[test]
fn create_replace_shrink_and_remount_both_layouts() {
    for partitioned in [false, true] {
        let disk = Disk::new(partitioned);
        let fs = Fat16::new(&disk).unwrap();
        fs.create("note.txt").unwrap();
        assert_eq!(fs.create("NOTE.TXT"), Err(Error::Exists));
        let mut out = [0; 1024];
        assert_eq!(fs.read("note.txt", &mut out), Ok(0));
        for length in [1, 511, 512, 513, 1024, 17, 0, 700] {
            let data: Vec<u8> = (0..length).map(|i| (i % 251) as u8).collect();
            fs.write("NOTE.txt", &data).unwrap();
            let remounted = Fat16::new(&disk).unwrap();
            assert_eq!(remounted.read("note.txt", &mut out), Ok(length));
            assert_eq!(&out[..length], &data);
            let mut names = Vec::new();
            remounted.list_entries("/", |n, s, _| names.push((*n, s))).unwrap();
            assert_eq!(names, vec![(*b"NOTE    TXT", length)]);
        }
        let bytes = disk.bytes.borrow();
        let start = if partitioned { 63 * 512 } else { 0 };
        assert_eq!(
            &bytes[start + 512..start + 33 * 512],
            &bytes[start + 33 * 512..start + 65 * 512]
        );
        // Only the current two clusters remain allocated after all replacements.
        let used = bytes[start + 512 + 4..start + 33 * 512]
            .chunks_exact(2)
            .filter(|c| *c != [0, 0])
            .count();
        assert_eq!(used, 2);
    }
}
#[test]
fn invalid_names_sizes_and_root_full_are_clean_errors() {
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    for name in [
        "",
        "a?b",
        "a b",
        "toolonggg.txt",
        "a.long",
        "a.",
        ".a",
        "a.b.c",
        "é",
    ] {
        assert_eq!(fs.create(name), Err(Error::InvalidName));
    }
    for i in 0..32 {
        fs.create(&format!("f{i}")).unwrap();
    }
    let before = disk.bytes.borrow().clone();
    assert_eq!(fs.create("extra"), Err(Error::Full));
    assert_eq!(fs.write("f0", &[0; 1025]), Err(Error::TooLarge));
    assert_eq!(fs.write("missing", b"x"), Err(Error::NotFound));
    assert_eq!(*disk.bytes.borrow(), before);
}
#[test]
fn disk_full_does_not_damage_previous_contents() {
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    fs.create("a").unwrap();
    fs.write("a", b"original").unwrap();
    for c in 3..=8126 {
        disk.set_fat(c, 0xffff);
    }
    let before = disk.bytes.borrow().clone();
    assert_eq!(fs.write("a", b"replacement"), Err(Error::Full));
    assert_eq!(*disk.bytes.borrow(), before);
    let mut out = [0; 1024];
    assert_eq!(fs.read("a", &mut out), Ok(8));
    assert_eq!(&out[..8], b"original");
}
#[test]
fn corrupt_chains_and_mismatched_fats_are_rejected() {
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    fs.create("a").unwrap();
    fs.write("a", &[7; 700]).unwrap();
    disk.set_fat(2, 2); // Cycle, bounded rather than followed forever.
    assert_eq!(fs.read("a", &mut [0; 1024]), Err(Error::Corrupt));
    disk.set_fat(2, 3);
    disk.bytes.borrow_mut()[33 * 512 + 4] = 0xff;
    assert_eq!(fs.read("a", &mut [0; 1024]), Err(Error::Corrupt));
}
#[test]
fn fragmented_host_file_and_readonly_directory_attributes() {
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    fs.create("a").unwrap();
    {
        let mut b = disk.bytes.borrow_mut();
        let root = 65 * 512;
        b[root + 26..root + 28].copy_from_slice(&3u16.to_le_bytes());
        b[root + 28..root + 32].copy_from_slice(&700u32.to_le_bytes());
        b[68 * 512..69 * 512].fill(b'x');
        b[73 * 512..74 * 512].fill(b'y');
    }
    disk.set_fat(3, 8);
    disk.set_fat(8, 0xffff);
    let mut out = [0; 1024];
    assert_eq!(fs.read("a", &mut out), Ok(700));
    assert_eq!(&out[..512], &[b'x'; 512]);
    assert_eq!(&out[512..700], &[b'y'; 188]);
    disk.bytes.borrow_mut()[65 * 512 + 11] = 1;
    assert_eq!(fs.write("a", b"no"), Err(Error::ReadOnly));
    disk.bytes.borrow_mut()[65 * 512 + 11] = 0x10;
    assert_eq!(fs.write("a", b"no"), Err(Error::NotFile));
    assert_eq!(fs.read("a", &mut out), Err(Error::NotFile));
    assert_eq!(fs.create("a"), Err(Error::Exists));
}
#[test]
fn invalid_geometry_is_rejected_without_writes() {
    for (offset, value) in [
        (13, 0),
        (13, 3),
        (16, 0),
        (16, 3),
        (12, 1),
        (510, 0),
        (22, 1),
        (20, 255),
    ] {
        let disk = Disk::new(false);
        disk.bytes.borrow_mut()[offset] = value;
        let before = disk.bytes.borrow().clone();
        assert!(Fat16::new(&disk).is_err(), "offset {offset}");
        assert_eq!(*disk.bytes.borrow(), before);
    }
    let disk = Disk::new(true);
    disk.bytes.borrow_mut()[454..458].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Fat16::new(&disk).is_err());
}
#[test]
fn failed_data_write_preserves_old_file_and_flush_failure_is_reported() {
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    fs.create("a").unwrap();
    fs.write("a", b"original").unwrap();
    disk.fail_after.set(Some(0));
    assert_eq!(fs.write("a", b"new"), Err(Error::Io));
    let mut out = [0; 1024];
    assert_eq!(fs.read("a", &mut out), Ok(8));
    assert_eq!(&out[..8], b"original");
    disk.fail_after.set(None);
    disk.fail_flush.set(true);
    assert_eq!(fs.write("a", b"new"), Err(Error::Io));
}
#[test]
fn ramdisk_really_writes_and_checks_bounds() {
    let disk = RamDisk::new(Box::leak(vec![0; 1024].into_boxed_slice()), 512);
    assert!(disk.write_block(1, &[42; 512]));
    let mut out = [0; 512];
    assert!(disk.read_block(1, &mut out));
    assert_eq!(out, [42; 512]);
    assert!(!disk.write_block(2, &[0; 512]));
    assert!(!disk.read_block(u32::MAX, &mut out));
    assert!(!disk.write_block(0, &[0; 3]));
}
#[test]
fn multi_sector_clusters_and_single_fat_are_supported() {
    let disk = Disk::new(false);
    {
        let mut bytes = disk.bytes.borrow_mut();
        bytes.resize(16384 * 512, 0);
        bytes[13] = 2;
        bytes[19..21].copy_from_slice(&16384u16.to_le_bytes());
    }
    let fs = Fat16::new(&disk).unwrap();
    fs.create("two.bin").unwrap();
    fs.write("two.bin", &[93; 1024]).unwrap();
    let mut out = [0; 1024];
    assert_eq!(fs.read("two.bin", &mut out), Ok(1024));
    assert_eq!(out, [93; 1024]);
    fs.write("two.bin", b"short").unwrap();
    assert_eq!(fs.read("two.bin", &mut out), Ok(5));
    assert_eq!(&out[..5], b"short");
    // Also exercise the one-FAT layout (different root/data offsets).
    let disk = Disk::new(false);
    disk.bytes.borrow_mut()[16] = 1;
    disk.bytes.borrow_mut()[33 * 512..65 * 512].fill(0);
    let fs = Fat16::new(&disk).unwrap();
    fs.create("a").unwrap();
    fs.write("a", &[4; 700]).unwrap();
    assert_eq!(fs.read("a", &mut out), Ok(700));
    assert_eq!(&out[..700], &[4; 700]);
}
#[test]
fn volatile_filesystem_matches_file_command_semantics() {
    vfs::init();
    vfs::create("note.txt").unwrap();
    assert_eq!(vfs::create("NOTE.TXT"), Err(Error::Exists));
    vfs::write("note.txt", b"hello").unwrap();
    let mut out = [0; 1024];
    assert_eq!(vfs::read("NOTE.TXT", &mut out), Ok(5));
    assert_eq!(&out[..5], b"hello");
    assert_eq!(vfs::write("note.txt", &[0; 1025]), Err(Error::TooLarge));
    let mut files = Vec::new();
    vfs::list_entries("/", |n, s, _| files.push((*n, s))).unwrap();
    assert_eq!(files, vec![(*b"NOTE    TXT", 5)]);
    assert_eq!(vfs::delete("missing"), Err(Error::NotFound));
    assert_eq!(vfs::delete("bad?name"), Err(Error::InvalidName));
    vfs::delete("NOTE.TXT").unwrap();
    assert_eq!(vfs::read("note.txt", &mut out), Err(Error::NotFound));
    assert_eq!(vfs::delete("note.txt"), Err(Error::NotFound));
    files.clear();
    vfs::list_entries("/", |n, s, _| files.push((*n, s))).unwrap();
    assert!(files.is_empty());
    vfs::create("note.txt").unwrap();
    assert_eq!(vfs::read("note.txt", &mut out), Ok(0));
    vfs::mkdir("docs").unwrap();
    vfs::mkdir("docs/sub").unwrap();
    vfs::create("docs/note.txt").unwrap();
    vfs::write("docs/note.txt", b"folder").unwrap();
    assert_eq!(vfs::read("/DOCS/SUB/../NOTE.TXT", &mut out), Ok(6));
    assert_eq!(&out[..6], b"folder");
    assert_eq!(vfs::read("note.txt", &mut out), Ok(0));
    assert_eq!(vfs::rmdir("docs"), Err(Error::DirectoryNotEmpty));
    assert_eq!(vfs::delete("docs"), Err(Error::NotFile));
    assert_eq!(vfs::is_directory("docs/note.txt"), Err(Error::NotDirectory));
    vfs::rmdir("docs/sub").unwrap();
    vfs::delete("docs/note.txt").unwrap();
    vfs::rmdir("docs").unwrap();
    vfs::init();
    assert_eq!(vfs::read("note.txt", &mut out), Err(Error::NotFound));
}

#[test]
fn delete_reclaims_clusters_and_directory_slots_in_both_layouts() {
    for partitioned in [false, true] {
        let disk = Disk::new(partitioned);
        let fs = Fat16::new(&disk).unwrap();
        let start = if partitioned { 63 } else { 0 };
        fs.create("gone.txt").unwrap();
        fs.write("gone.txt", &[42; 1024]).unwrap();
        fs.create("keep.txt").unwrap();
        fs.write("keep.txt", b"keep").unwrap();
        fs.create("empty.txt").unwrap();
        fs.delete("GONE.TXT").unwrap();
        fs.delete("empty.txt").unwrap();
        let mut out = [0; 1024];
        let fs = Fat16::new(&disk).unwrap();
        assert_eq!(fs.read("gone.txt", &mut out), Err(Error::NotFound));
        assert_eq!(fs.read("keep.txt", &mut out), Ok(4));
        assert_eq!(&out[..4], b"keep");
        let mut names = Vec::new();
        fs.list_entries("/", |name, _, _| names.push(*name)).unwrap();
        assert_eq!(names, vec![*b"KEEP    TXT"]);
        for copy in 0..2 {
            let at = (start + 1 + copy * 32) * 512 + 4;
            assert_eq!(&disk.bytes.borrow()[at..at + 4], &[0; 4]);
        }
        assert_eq!(disk.bytes.borrow()[(start + 65) * 512], 0xe5);
        fs.create("reuse.txt").unwrap();
        fs.write("reuse.txt", &[99; 1024]).unwrap();
        assert_eq!(fs.read("reuse.txt", &mut out), Ok(1024));
        assert_eq!(out, [99; 1024]);
        let bytes = disk.bytes.borrow();
        assert_eq!(&bytes[(start + 65) * 512..(start + 65) * 512 + 11], b"REUSE   TXT");
        assert_eq!(&bytes[(start + 1) * 512..(start + 33) * 512], &bytes[(start + 33) * 512..(start + 65) * 512]);
    }
}

#[test]
fn delete_refuses_readonly_directories_and_bad_chains_without_writes() {
    for (attribute, corrupt, expected) in [(1, false, Error::ReadOnly), (0x10, false, Error::NotFile), (0x20, true, Error::Corrupt)] {
        let disk = Disk::new(false);
        let fs = Fat16::new(&disk).unwrap();
        fs.create("test.txt").unwrap();
        fs.write("test.txt", b"data").unwrap();
        disk.bytes.borrow_mut()[65 * 512 + 11] = attribute;
        if corrupt { disk.set_fat(2, 2); }
        let before = disk.bytes.borrow().clone();
        assert_eq!(fs.delete("test.txt"), Err(expected));
        assert_eq!(fs.delete("missing"), Err(Error::NotFound));
        assert_eq!(fs.delete("bad?name"), Err(Error::InvalidName));
        assert_eq!(*disk.bytes.borrow(), before);
    }
}

#[test]
fn delete_failures_never_release_clusters_before_removing_the_entry() {
    for after in 0..5 {
        let disk = Disk::new(false);
        let fs = Fat16::new(&disk).unwrap();
        fs.create("gone.txt").unwrap();
        fs.write("gone.txt", &[42; 1024]).unwrap();
        fs.create("keep.txt").unwrap();
        fs.write("keep.txt", b"safe").unwrap();
        let before = disk.bytes.borrow().clone();
        disk.fail_after.set(Some(after));
        assert_eq!(fs.delete("gone.txt"), Err(Error::Io));
        if after == 0 { assert_eq!(*disk.bytes.borrow(), before); }
        else { assert_eq!(disk.bytes.borrow()[65 * 512], 0xe5); }
        let mut out = [0; 1024];
        assert_eq!(fs.read("keep.txt", &mut out), Ok(4));
        assert_eq!(&out[..4], b"safe");
    }
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    fs.create("flush.txt").unwrap();
    fs.write("flush.txt", b"data").unwrap();
    let fats = disk.bytes.borrow()[512..65 * 512].to_vec();
    disk.fail_flush.set(true);
    assert_eq!(fs.delete("flush.txt"), Err(Error::Io));
    assert_eq!(&disk.bytes.borrow()[512..65 * 512], &fats);
}

#[test]
fn format_replaces_old_metadata_and_creates_a_usable_partition() {
    let disk = Disk::new(false);
    disk.bytes.borrow_mut().fill(0xa5); // Old files, partition tables, GPT signatures.
    let plan = FormatPlan::new(disk.block_count()).unwrap();
    assert_eq!(plan.disk_sectors, 8192);
    format_disk(&disk, plan).unwrap();
    {
        let bytes = disk.bytes.borrow();
        assert_eq!(bytes[450], 0x0e);
        assert_eq!(&bytes[454..458], &2048u32.to_le_bytes());
        assert!(bytes[512..34 * 512].iter().all(|b| *b == 0));
        assert!(bytes[bytes.len() - 33 * 512..].iter().all(|b| *b == 0));
        let start = FormatPlan::START as usize;
        let spf = plan.sectors_per_fat as usize;
        assert_eq!(
            &bytes[(start + 1) * 512..(start + 1 + spf) * 512],
            &bytes[(start + 1 + spf) * 512..(start + 1 + spf * 2) * 512]
        );
        assert!(
            bytes[(start + 1 + spf * 2) * 512..(start + 33 + spf * 2) * 512]
                .iter()
                .all(|b| *b == 0)
        );
        // Quick format leaves free data untouched except the old backup GPT area.
        assert_eq!(bytes[(start + 33 + spf * 2) * 512], 0xa5);
    }
    let fs = Fat16::new(&disk).unwrap();
    assert_eq!(fs.start_sector(), 2048);
    let mut count = 0;
    fs.list_entries("/", |_, _, _| count += 1).unwrap();
    assert_eq!(count, 0);
    fs.create("formatted.txt").unwrap_err(); // 8.3 length still enforced.
    fs.create("new.txt").unwrap();
    fs.write("new.txt", b"readable after format").unwrap();
    let mut out = [0; 1024];
    assert_eq!(Fat16::new(&disk).unwrap().read("new.txt", &mut out), Ok(21));
    assert_eq!(&out[..21], b"readable after format");
}

#[test]
fn format_geometry_stays_fat16_across_disk_sizes_and_caps_large_disks() {
    for sectors in [0, 2048, 4096, 6144] {
        assert_eq!(FormatPlan::new(sectors), Err(Error::DiskTooSmall));
    }
    for sectors in [
        8192,
        20480,
        32768,
        65536,
        131072,
        262144,
        1_048_576,
        4_194_304,
        8_388_608,
        1 << 28,
    ] {
        let plan = FormatPlan::new(sectors).unwrap();
        let overhead = 33 + 2 * plan.sectors_per_fat as u32;
        let clusters = (plan.volume_sectors - overhead) / plan.sectors_per_cluster as u32;
        assert!((4101..=65508).contains(&clusters));
        assert!((clusters + 2) * 2 <= plan.sectors_per_fat as u32 * 512);
        assert!(2048 + plan.volume_sectors <= sectors);
        assert!(plan.volume_sectors <= FormatPlan::MAX_VOLUME_SECTORS);
    }
    let disk = Disk::new(false);
    let mut wrong = FormatPlan::new(disk.block_count()).unwrap();
    wrong.volume_sectors -= 1;
    let before = disk.bytes.borrow().clone();
    assert_eq!(format_disk(&disk, wrong), Err(Error::InvalidVolume));
    assert_eq!(*disk.bytes.borrow(), before);
}

#[test]
fn formatting_reports_io_failures_without_publishing_a_valid_partition() {
    for writes in [0, 1, 2, 68, 100] {
        let disk = Disk::new(false);
        let plan = FormatPlan::new(disk.block_count()).unwrap();
        disk.fail_after.set(Some(writes));
        assert_eq!(format_disk(&disk, plan), Err(Error::Io));
        if writes > 0 {
            assert_eq!(&disk.bytes.borrow()[510..512], &[0, 0]);
        }
    }
    let disk = Disk::new(false);
    let plan = FormatPlan::new(disk.block_count()).unwrap();
    disk.fail_flush.set(true);
    assert_eq!(format_disk(&disk, plan), Err(Error::Io));
    assert_eq!(&disk.bytes.borrow()[510..512], &[0, 0]);
}

fn install_fixture() -> (Vec<u8>, bootdisk::Kernel, Vec<u8>, Vec<u8>) {
    let image: Vec<u8> = (0..1500).map(|i| (i % 251) as u8).collect();
    let kernel = bootdisk::Kernel {
        entry: bootdisk::LOAD + 42,
        bytes: image.len() as u32,
        memory: 16384,
        checksum: bootdisk::checksum(&image),
    };
    let mut mbr = vec![0x90; 512];
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
    (image, kernel, mbr, vec![0x5a; 32 * 512])
}
#[test]
fn installer_preserves_compatible_fat_data_and_commits_loader_last() {
    let disk = Disk::new(false);
    format_disk(&disk, FormatPlan::new(disk.block_count()).unwrap()).unwrap();
    let fs = Fat16::new(&disk).unwrap();
    fs.create("keep.txt").unwrap();
    fs.write("keep.txt", b"keep my data").unwrap();
    let (image, kernel, mbr, stage2) = install_fixture();
    let before = disk.bytes.borrow().clone();
    let plan = bootdisk::plan(&disk, kernel).unwrap();
    assert!(plan.preserve_data);
    assert_eq!(*disk.bytes.borrow(), before); // Preview is read-only.
    bootdisk::install(&disk, plan, &image, &mbr, &stage2).unwrap();
    let after = disk.bytes.borrow();
    assert_eq!(&before[2048 * 512..], &after[2048 * 512..]);
    assert_eq!(&before[440..510], &after[440..510]);
    assert_eq!(&after[..440], &mbr[..440]);
    assert_eq!(&after[512..33 * 512], &stage2);
    assert_eq!(&after[63 * 512..64 * 512], &kernel.header());
    assert_eq!(&after[64 * 512..64 * 512 + image.len()], &image);
    assert!(after[64 * 512 + image.len()..67 * 512]
        .iter()
        .all(|b| *b == 0));
    drop(after);
    let mut output = [0; 1024];
    assert_eq!(
        Fat16::new(&disk).unwrap().read("keep.txt", &mut output),
        Ok(12)
    );
    assert_eq!(&output[..12], b"keep my data");
}
#[test]
fn installer_reformats_conflicting_layouts_only_when_explicitly_committed() {
    for old_mbr in [false, true] {
        let disk = Disk::new(old_mbr); // Whole volume or partition at LBA 63.
        let (image, kernel, mbr, stage2) = install_fixture();
        let before = disk.bytes.borrow().clone();
        let plan = bootdisk::plan(&disk, kernel).unwrap();
        assert!(!plan.preserve_data);
        assert_eq!(*disk.bytes.borrow(), before);
        bootdisk::install(&disk, plan, &image, &mbr, &stage2).unwrap();
        assert_eq!(Fat16::new(&disk).unwrap().start_sector(), 2048);
    }
}
#[test]
fn installer_rejects_invalid_images_and_changed_targets_without_writes() {
    let disk = Disk::new(false);
    let (mut image, kernel, mbr, stage2) = install_fixture();
    for mut bad in [kernel; 3].into_iter().enumerate() {
        match bad.0 {
            0 => bad.1.entry = 0,
            1 => bad.1.bytes = bootdisk::MAX_IMAGE as u32 + 1,
            _ => bad.1.memory = 1,
        }
        assert_eq!(bootdisk::plan(&disk, bad.1), Err(Error::InvalidKernel));
    }
    let plan = bootdisk::plan(&disk, kernel).unwrap();
    let before = disk.bytes.borrow().clone();
    image[100] ^= 1;
    assert_eq!(
        bootdisk::install(&disk, plan, &image, &mbr, &stage2),
        Err(Error::InvalidKernel)
    );
    assert_eq!(*disk.bytes.borrow(), before);
    image[100] ^= 1;
    disk.bytes.borrow_mut()[440] ^= 1;
    let changed = disk.bytes.borrow().clone();
    assert_eq!(
        bootdisk::install(&disk, plan, &image, &mbr, &stage2),
        Err(Error::DiskChanged)
    );
    assert_eq!(*disk.bytes.borrow(), changed);
}
#[test]
fn installer_failure_leaves_boot_signature_unpublished() {
    let disk = Disk::new(false);
    format_disk(&disk, FormatPlan::new(disk.block_count()).unwrap()).unwrap();
    let (image, kernel, mbr, stage2) = install_fixture();
    let plan = bootdisk::plan(&disk, kernel).unwrap();
    disk.fail_after.set(Some(1));
    assert_eq!(
        bootdisk::install(&disk, plan, &image, &mbr, &stage2),
        Err(Error::Io)
    );
    assert_eq!(&disk.bytes.borrow()[510..512], &[0, 0]);
    assert!(Fat16::new(&disk).is_err()); // Incomplete MBR is not advertised as usable.
}

#[test]
fn pci_ide_routing_distinguishes_legacy_native_and_disabled_channels() {
    use pci::{ide_channel, IdeChannel};
    assert_eq!(
        ide_channel(0x80, 1, [0; 4], false),
        Some(IdeChannel::PRIMARY)
    );
    assert_eq!(
        ide_channel(0x80, 1, [0; 4], true),
        Some(IdeChannel::SECONDARY)
    );
    let bars = [0xc001, 0xc009, 0xc011, 0xc019];
    assert_eq!(
        ide_channel(0x85, 1, bars, false),
        Some(IdeChannel {
            io: 0xc000,
            control: 0xc00a
        })
    );
    assert_eq!(
        ide_channel(0x85, 1, bars, true),
        Some(IdeChannel {
            io: 0xc010,
            control: 0xc01a
        })
    );
    assert_eq!(
        ide_channel(0x81, 1, bars, true),
        Some(IdeChannel::SECONDARY)
    );
    assert_eq!(ide_channel(0x85, 0, bars, false), None);
    for bad in [
        [0, 0, 0, 0],
        [0xc000, 0xc009, 0, 0],
        [0xc001, 0xc008, 0, 0],
        [0x10001, 0xc009, 0, 0],
        [0xfffd, 0xc009, 0, 0],
    ] {
        assert_eq!(ide_channel(0x85, 1, bad, false), None);
    }
}

#[test]
fn ata_timeout_uses_elapsed_ticks_even_after_many_poll_iterations() {
    let mut budget = ata_poll::Budget::new(0, 500);
    // More iterations than the former one-million total limit, but ticks advance.
    for tick in 0..100 {
        for _ in 0..20_000 {
            assert!(!budget.expired(tick));
        }
    }
    assert!(!budget.expired(499));
    assert!(budget.expired(500));
    let mut wrapping = ata_poll::Budget::new(u32::MAX - 100, 200);
    assert!(!wrapping.expired(98));
    assert!(wrapping.expired(99));
    let mut stopped = ata_poll::Budget::new(50, 500);
    for _ in 0..9_999_999 {
        assert!(!stopped.expired(50));
    }
    assert!(stopped.expired(50));
}

#[test]
fn disk_target_records_validate_paths_and_detect_corruption() {
    use disk_target::{parse_bdf, Target};
    assert_eq!(parse_bdf("00:1f.2"), Some(0x1f02));
    for bad in ["00:20.0", "00:1f.8", "100:1.0", "0:0.+", "0:0.00", "foo"] {
        assert_eq!(parse_bdf(bad), None);
    }
    for target in [
        Target::legacy(false, false),
        Target::legacy(true, true),
        Target {
            bdf: 0x1f02,
            id: 0x29208086,
            position: 1,
        },
    ] {
        let mut record = target.encode();
        assert_eq!(Target::decode(&record), Some(target));
        record[7] ^= 1;
        assert_eq!(Target::decode(&record), None);
    }
    assert!(!Target {
        bdf: 0x2000,
        id: 0x29208086,
        position: 0
    }
    .valid());
    assert!(!Target {
        bdf: 0x1f02,
        id: 0x29208086,
        position: 4
    }
    .valid());
}

#[test]
fn identify_capacity_distinguishes_full_size_from_the_legacy_prefix() {
    fn identify(legacy: u32, full: Option<u64>) -> [u16; 256] {
        let mut words = [0; 256];
        words[60] = legacy as u16;
        words[61] = (legacy >> 16) as u16;
        if let Some(full) = full {
            words[83] = 0x4400;
            for i in 0..4 {
                words[100 + i] = (full >> (i * 16)) as u16;
            }
        }
        words
    }
    use ata_capacity::{decode, Capacity};
    assert_eq!(
        decode(&identify(32768, None)),
        Some(Capacity {
            accessible: 32768,
            physical: Some(32768),
            lba48: false,
        })
    );
    assert_eq!(
        decode(&identify(0x0fffffff, Some(500_000_000))),
        Some(Capacity {
            accessible: 500_000_000,
            physical: Some(500_000_000),
            lba48: true,
        })
    );
    assert_eq!(decode(&identify(0x0fffffff, None)).unwrap().physical, None);
    assert_eq!(decode(&identify(32768, Some(100))).unwrap().physical, None);
    assert_eq!(
        decode(&identify(32768, Some(1u64 << 48))).unwrap().physical,
        None
    );
    assert_eq!(decode(&identify(0, None)), None);
    assert_eq!(decode(&identify((1 << 28) + 1, None)), None);
    let large = decode(&identify(0x0fffffff, Some(1u64 << 33))).unwrap();
    assert_eq!(large.accessible, u32::MAX);
    assert_eq!(large.physical, Some(1u64 << 33));
    assert!(large.lba48);
    let invalid = decode(&identify(32768, Some(100))).unwrap();
    assert_eq!(invalid.accessible, 32768);
    assert!(!invalid.lba48);
}

struct LimitedDisk<'a> {
    inner: &'a Disk,
    physical: Option<u64>,
}
impl BlockDevice for LimitedDisk<'_> {
    fn read_block(&self, lba: u32, out: &mut [u8]) -> bool {
        self.inner.read_block(lba, out)
    }
    fn write_block(&self, lba: u32, data: &[u8]) -> bool {
        self.inner.write_block(lba, data)
    }
    fn block_size(&self) -> usize {
        512
    }
    fn block_count(&self) -> u32 {
        self.inner.block_count()
    }
    fn physical_block_count(&self) -> Option<u64> {
        self.physical
    }
    fn flush(&self) -> bool {
        self.inner.flush()
    }
}

#[test]
fn limited_or_unknown_full_capacity_refuses_destructive_operations_without_writes() {
    for physical in [None, Some(500_000_000)] {
        let disk = Disk::new(false);
        let limited = LimitedDisk {
            inner: &disk,
            physical,
        };
        let (image, kernel, mbr, stage2) = install_fixture();
        let expected = bootdisk::plan(&disk, kernel).unwrap();
        let before = disk.bytes.borrow().clone();
        assert_eq!(
            format_disk(&limited, FormatPlan::new(limited.block_count()).unwrap()),
            Err(Error::LimitedCapacity)
        );
        assert_eq!(
            bootdisk::plan(&limited, kernel),
            Err(Error::LimitedCapacity)
        );
        assert_eq!(
            bootdisk::install(&limited, expected, &image, &mbr, &stage2),
            Err(Error::LimitedCapacity)
        );
        assert_eq!(*disk.bytes.borrow(), before);
    }
}

#[test]
fn limited_capacity_allows_existing_accessible_partitions_and_preserving_install() {
    let disk = Disk::new(false);
    format_disk(&disk, FormatPlan::new(disk.block_count()).unwrap()).unwrap();
    let limited = LimitedDisk {
        inner: &disk,
        physical: Some(500_000_000),
    };
    let fs = Fat16::new(&limited).unwrap();
    fs.create("keep.txt").unwrap();
    fs.write("keep.txt", b"preserved").unwrap();
    let (image, kernel, mbr, stage2) = install_fixture();
    let plan = bootdisk::plan(&limited, kernel).unwrap();
    assert!(plan.preserve_data);
    let before = disk.bytes.borrow()[2048 * 512..].to_vec();
    bootdisk::install(&limited, plan, &image, &mbr, &stage2).unwrap();
    assert_eq!(&disk.bytes.borrow()[2048 * 512..], &before);
    let mut out = [0; 1024];
    assert_eq!(
        Fat16::new(&limited).unwrap().read("keep.txt", &mut out),
        Ok(9)
    );
    assert_eq!(&out[..9], b"preserved");
}

#[path = "fsck_tests.rs"]
mod fsck_tests;

#[path = "../src/stream_protocol.rs"]
mod stream_protocol;
#[path = "stream_tests.rs"]
mod stream_tests;
