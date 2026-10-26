use super::{Disk, Error, Fat16};

#[test]
fn folders_span_multiple_sectors_per_cluster_and_extend_cleanly() {
    let disk = Disk::new(false);
    {
        let mut bytes = disk.bytes.borrow_mut();
        bytes.resize(16384 * 512, 0);
        bytes[13] = 2;
        bytes[19..21].copy_from_slice(&16384u16.to_le_bytes());
    }
    let fs = Fat16::new(&disk).unwrap();
    fs.mkdir("d").unwrap();
    for index in 0..40 {
        fs.create(&format!("d/f{}.txt", index)).unwrap();
    }
    fs.write("d/f39.txt", &[77; 1024]).unwrap();
    let fs = Fat16::new(&disk).unwrap();
    let mut out = [0; 1024];
    assert_eq!(fs.read("d/f39.txt", &mut out), Ok(1024));
    assert_eq!(out, [77; 1024]);
    let mut count = 0;
    fs.list_entries("d", |_, _, _| count += 1).unwrap();
    assert_eq!(count, 40);
    for index in 0..40 {
        fs.delete(&format!("d/f{}.txt", index)).unwrap();
    }
    fs.rmdir("d").unwrap();
    for copy in 0..2 {
        assert_eq!(
            &disk.bytes.borrow()[(1 + copy * 32) * 512 + 4..(1 + copy * 32) * 512 + 10],
            &[0; 6]
        );
    }
}

#[test]
fn paths_normalize_separators_parent_and_depth() {
    use crate::fs_path::Path;
    let docs = Path::parse("/docs/sub", &Path::ROOT).unwrap();
    let path = Path::parse("../other\\note.txt", &docs).unwrap();
    let mut bytes = [0; crate::fs_path::MAX_PATH];
    let count = path.encode(&mut bytes);
    assert_eq!(&bytes[..count], b"/DOCS/OTHER/NOTE.TXT");
    assert_eq!(Path::parse("/docs/../..", &docs).unwrap(), Path::ROOT);
    assert_eq!(
        Path::parse("a/b/c/d/e/f/g/h/i", &Path::ROOT),
        Err(Error::PathTooDeep)
    );
    assert!(Path::parse("/docs", &Path::ROOT).unwrap().contains(&docs));
}

#[test]
fn nested_folders_persist_and_removal_preserves_siblings() {
    for partitioned in [false, true] {
        let disk = Disk::new(partitioned);
        let fs = Fat16::new(&disk).unwrap();
        fs.mkdir("docs").unwrap();
        fs.mkdir("docs/sub").unwrap();
        for (name, data) in [
            ("note.txt", &b"root"[..]),
            ("docs/note.txt", &b"folder"[..]),
            ("docs/sub/note.txt", &b"nested"[..]),
        ] {
            fs.create(name).unwrap();
            fs.write(name, data).unwrap();
        }
        let fs = Fat16::new(&disk).unwrap();
        let mut out = [0; 1024];
        assert_eq!(fs.read("/DOCS/SUB/../NOTE.TXT", &mut out), Ok(6));
        assert_eq!(&out[..6], b"folder");
        assert_eq!(fs.read("docs\\sub\\note.txt", &mut out), Ok(6));
        assert_eq!(&out[..6], b"nested");
        assert_eq!(fs.mkdir("DOCS"), Err(Error::Exists));
        assert_eq!(fs.is_directory("note.txt"), Err(Error::NotDirectory));
        assert_eq!(fs.mkdir("missing/child"), Err(Error::NotFound));
        assert_eq!(fs.delete("docs"), Err(Error::NotFile));
        assert_eq!(fs.rmdir("docs"), Err(Error::DirectoryNotEmpty));
        assert_eq!(fs.rmdir("/"), Err(Error::DirectoryBusy));
        let mut items = Vec::new();
        fs.list_entries("docs", |n, size, dir| items.push((*n, size, dir)))
            .unwrap();
        assert_eq!(
            items,
            vec![(*b"SUB        ", 0, true), (*b"NOTE    TXT", 6, false)]
        );
        let start = if partitioned { 63 } else { 0 };
        let bytes = disk.bytes.borrow();
        assert_eq!(
            &bytes[(start + 67) * 512..(start + 67) * 512 + 11],
            b".          "
        );
        assert_eq!(
            &bytes[(start + 67) * 512 + 58..(start + 67) * 512 + 60],
            &[0, 0]
        );
        assert_eq!(
            &bytes[(start + 68) * 512 + 58..(start + 68) * 512 + 60],
            &[2, 0]
        );
        drop(bytes);
        fs.delete("docs/sub/note.txt").unwrap();
        fs.rmdir("docs/sub").unwrap();
        fs.delete("docs/note.txt").unwrap();
        fs.rmdir("docs").unwrap();
        assert_eq!(fs.read("note.txt", &mut out), Ok(4));
        assert_eq!(&out[..4], b"root");
        fs.mkdir("docs").unwrap();
    }
}

#[test]
fn growing_directory_chains_are_reclaimed_and_corruption_is_bounded() {
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    fs.mkdir("d").unwrap();
    for index in 0..40 {
        fs.create(&format!("d/f{}.txt", index)).unwrap();
    }
    assert_eq!(&disk.bytes.borrow()[516..520], &[3, 0, 4, 0]);
    fs.write("d/f39.txt", &[123; 1024]).unwrap();
    let fs = Fat16::new(&disk).unwrap();
    let mut out = [0; 1024];
    assert_eq!(fs.read("d/f39.txt", &mut out), Ok(1024));
    assert_eq!(out, [123; 1024]);
    for index in 0..40 {
        fs.delete(&format!("d/f{}.txt", index)).unwrap();
    }
    fs.rmdir("d").unwrap();
    for copy in 0..2 {
        assert_eq!(
            &disk.bytes.borrow()[(1 + copy * 32) * 512 + 4..(1 + copy * 32) * 512 + 14],
            &[0; 10]
        );
    }
    for corruption in 0..4 {
        let disk = Disk::new(false);
        let fs = Fat16::new(&disk).unwrap();
        fs.mkdir("d").unwrap();
        match corruption {
            0 => disk.set_fat(2, 2),
            1 => disk.bytes.borrow_mut()[67 * 512 + 58] = 1,
            2 => disk.bytes.borrow_mut()[65 * 512 + 20] = 1,
            _ => disk.bytes.borrow_mut()[65 * 512 + 28] = 1,
        }
        let before = disk.bytes.borrow().clone();
        assert_eq!(fs.create("d/file"), Err(Error::Corrupt));
        assert_eq!(*disk.bytes.borrow(), before);
    }
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    fs.mkdir("d").unwrap();
    for cluster in 2..34 {
        disk.set_fat(cluster, cluster as u16 + 1);
    }
    disk.set_fat(34, 0xffff);
    assert_eq!(fs.is_directory("d"), Err(Error::DirectoryTooLarge));
}

#[test]
fn folder_write_failures_preserve_files_and_readonly_folders_refuse_changes() {
    for after in 0..6 {
        let disk = Disk::new(false);
        let fs = Fat16::new(&disk).unwrap();
        fs.create("keep").unwrap();
        fs.write("keep", b"safe").unwrap();
        disk.fail_after.set(Some(after));
        assert_eq!(fs.mkdir("d"), Err(Error::Io));
        let mut out = [0; 1024];
        assert_eq!(fs.read("keep", &mut out), Ok(4));
        assert_eq!(&out[..4], b"safe");
    }
    let disk = Disk::new(false);
    let fs = Fat16::new(&disk).unwrap();
    fs.mkdir("locked").unwrap();
    disk.bytes.borrow_mut()[65 * 512 + 11] |= 1;
    let before = disk.bytes.borrow().clone();
    assert_eq!(fs.create("locked/file"), Err(Error::ReadOnly));
    assert_eq!(fs.mkdir("locked/sub"), Err(Error::ReadOnly));
    assert_eq!(fs.rmdir("locked"), Err(Error::ReadOnly));
    assert_eq!(*disk.bytes.borrow(), before);
}
