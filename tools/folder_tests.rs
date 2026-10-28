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
#[test]
fn rename_preserves_file_and_folder_contents_and_refuses_conflicts() {
 let disk=Disk::new(false);let fs=Fat16::new(&disk).unwrap();fs.mkdir("docs").unwrap();fs.create("docs/a.txt").unwrap();fs.write("docs/a.txt",b"hello").unwrap();
 let fat=disk.bytes.borrow()[512..65*512].to_vec();
 fs.rename("docs/a.txt","docs/b.txt").unwrap();assert_eq!(fs.read("docs/a.txt",&mut [0;1024]),Err(Error::NotFound));
 let mut out=[0;1024];assert_eq!(fs.read("docs/b.txt",&mut out),Ok(5));assert_eq!(&out[..5],b"hello");assert_eq!(&disk.bytes.borrow()[512..65*512],&fat);
 fs.create("docs/c.txt").unwrap();let before=disk.bytes.borrow().clone();assert_eq!(fs.rename("docs/b.txt","docs/c.txt"),Err(Error::Exists));assert_eq!(*disk.bytes.borrow(),before);
 fs.rename("docs","notes").unwrap();assert_eq!(fs.read("notes/b.txt",&mut out),Ok(5));assert!(fs.is_directory("docs").is_err());assert_eq!(fs.rename("notes/b.txt","other/b.txt"),Err(Error::NotFound));
}
#[test]
fn file_copy_and_move_preserve_large_images_and_have_independent_chains() {
 let disk=Disk::new(false);let fs=Fat16::new(&disk).unwrap();fs.mkdir("from").unwrap();fs.mkdir("to").unwrap();fs.create("from/a.bin").unwrap();let data:Vec<u8>=(0..65536).map(|x|(x%251) as u8).collect();fs.write_program("from/a.bin",&data).unwrap();let mut buffer=vec![0;65536];
 fs.copy_file("from/a.bin","to/b.bin",&mut buffer).unwrap();assert_eq!(buffer,data);fs.delete("from/a.bin").unwrap();buffer.fill(0);assert_eq!(fs.read_program("to/b.bin",&mut buffer),Ok(65536));assert_eq!(buffer,data);
 fs.move_file("to/b.bin","from/c.bin",&mut buffer).unwrap();assert_eq!(fs.read_program("to/b.bin",&mut buffer),Err(Error::NotFound));assert_eq!(fs.read_program("from/c.bin",&mut buffer),Ok(65536));assert_eq!(buffer,data);
 fs.delete("from/c.bin").unwrap();fs.rmdir("from").unwrap();fs.rmdir("to").unwrap();assert!(disk.bytes.borrow()[512+4..33*512].iter().all(|&b|b==0));
}
#[test]
fn transfers_refuse_existing_self_invalid_and_full_targets_without_losing_source() {
 let disk=Disk::new(false);let fs=Fat16::new(&disk).unwrap();fs.create("a.txt").unwrap();fs.write("a.txt",b"original").unwrap();fs.create("b.txt").unwrap();fs.write("b.txt",b"keep").unwrap();let mut buffer=vec![0;65536];let before=disk.bytes.borrow().clone();
 for target in ["a.txt","b.txt"] {assert_eq!(fs.copy_file("a.txt",target,&mut buffer),Err(Error::Exists));}assert_eq!(*disk.bytes.borrow(),before);
 assert_eq!(fs.copy_file("a.txt","bad name.txt",&mut buffer),Err(Error::InvalidName));
 // Fill every remaining data cluster. Copy must fail before publishing a target.
 for c in 2..8192 {if fs::fat_unused(&disk,c) {disk.set_fat(c,0xffff);}}
 assert_eq!(fs.copy_file("a.txt","new.txt",&mut buffer),Err(Error::Full));assert_eq!(fs.read("new.txt",&mut [0;1024]),Err(Error::NotFound));let mut out=[0;1024];assert_eq!(fs.read("a.txt",&mut out),Ok(8));assert_eq!(&out[..8],b"original");
 // Renaming does not need free data clusters.
 fs.move_file("a.txt","renamed.txt",&mut buffer).unwrap();assert_eq!(fs.read("renamed.txt",&mut out),Ok(8));
}
mod fs {pub fn fat_unused(d:&super::Disk,c:usize)->bool {let b=d.bytes.borrow();c*2+513<b.len()&&u16::from_le_bytes([b[512+c*2],b[513+c*2]])==0}}
#[test]
fn copy_failure_and_readonly_move_leave_source_untouched() {
 let disk=Disk::new(false);let fs=Fat16::new(&disk).unwrap();fs.mkdir("dest").unwrap();fs.create("a.txt").unwrap();fs.write("a.txt",b"original").unwrap();let mut buffer=vec![0;65536];
 disk.fail_after.set(Some(0));assert_eq!(fs.copy_file("a.txt","dest/new.txt",&mut buffer),Err(Error::Io));disk.fail_after.set(None);let mut out=[0;1024];assert_eq!(fs.read("a.txt",&mut out),Ok(8));assert_eq!(fs.read("dest/new.txt",&mut out),Err(Error::NotFound));
 // The first regular file after DEST is read-only.
 disk.bytes.borrow_mut()[65*512+32+11]|=1;
 let before=disk.bytes.borrow().clone();assert_eq!(fs.move_file("a.txt","dest/new.txt",&mut buffer),Err(Error::ReadOnly));assert_eq!(*disk.bytes.borrow(),before);
 fs.copy_file("a.txt","dest/ro.txt",&mut buffer).unwrap();assert_eq!(fs.write("dest/ro.txt",b"change"),Err(Error::ReadOnly));
 assert_eq!(fs.copy_file("dest","folder",&mut buffer),Err(Error::NotFile));
}
#[test]
fn recursive_transfers_preserve_subtrees_and_refuse_cycles_depth_and_readonly_moves() {
 let disk=Disk::new(false);let fs=Fat16::new(&disk).unwrap();fs.mkdir("tree").unwrap();fs.mkdir("tree/sub").unwrap();fs.create("tree/sub/a.txt").unwrap();fs.write("tree/sub/a.txt",b"nested").unwrap();let mut data=vec![0;65536];
 let before=disk.bytes.borrow().clone();assert_eq!(fs.transfer_tree("tree","tree/sub/copy",false,&mut data),Err(Error::InvalidName));assert_eq!(*disk.bytes.borrow(),before);
 fs.transfer_tree("tree","copy",false,&mut data).unwrap();let mut out=[0;1024];assert_eq!(fs.read("copy/sub/a.txt",&mut out),Ok(6));assert_eq!(&out[..6],b"nested");
 fs.mkdir("dest").unwrap();fs.transfer_tree("copy","dest/moved",true,&mut data).unwrap();assert!(fs.is_directory("copy").is_err());assert_eq!(fs.read("dest/moved/sub/a.txt",&mut out),Ok(6));assert_eq!(fs.read("tree/sub/a.txt",&mut out),Ok(6));
 assert_eq!(fs.transfer_tree("tree","dest/moved",false,&mut data),Err(Error::Exists));
 // Mark a nested leaf read-only; move must fail before creating a destination.
 {let mut bytes=disk.bytes.borrow_mut();let at=(0..bytes.len()).step_by(32).find(|&i|bytes.get(i..i+11)==Some(b"A       TXT")).unwrap();bytes[at+11]|=1;}
 let before=disk.bytes.borrow().clone();assert_eq!(fs.transfer_tree("tree","dest/denied",true,&mut data),Err(Error::ReadOnly));assert_eq!(*disk.bytes.borrow(),before);
 fs.transfer_tree("tree","readonly",false,&mut data).unwrap();assert_eq!(fs.write("readonly/sub/a.txt",b"change"),Err(Error::ReadOnly));

 fs.mkdir("a").unwrap();fs.mkdir("a/b").unwrap();fs.mkdir("a/b/c").unwrap();fs.mkdir("a/b/c/d").unwrap();fs.mkdir("a/b/c/d/e").unwrap();fs.mkdir("a/b/c/d/e/f").unwrap();fs.mkdir("a/b/c/d/e/f/g").unwrap();let before=disk.bytes.borrow().clone();assert_eq!(fs.transfer_tree("tree","a/b/c/d/e/f/g/t",false,&mut data),Err(Error::PathTooDeep));assert_eq!(*disk.bytes.borrow(),before);
}

#[test]fn large_saves_create_replace_and_preserve_files_on_errors(){let disk=Disk::new(false);let fs=Fat16::new(&disk).unwrap();let data:Vec<u8>=(0..65536).map(|i|(i%251) as u8).collect();disk.fail_after.set(Some(0));assert_eq!(fs.save_large("new.txt",&data),Err(Error::Io));disk.fail_after.set(None);assert_eq!(fs.file_size("new.txt"),Err(Error::NotFound));fs.save_large("big.txt",&data).unwrap();assert_eq!(fs.file_size("big.txt"),Ok(65536));let mut out=vec![0;65536];assert_eq!(fs.read_program("big.txt",&mut out),Ok(65536));assert_eq!(out,data);disk.fail_after.set(Some(0));assert_eq!(fs.save_large("big.txt",b"replacement"),Err(Error::Io));disk.fail_after.set(None);assert_eq!(fs.read_program("big.txt",&mut out),Ok(65536));assert_eq!(out,data);disk.bytes.borrow_mut()[65*512+11]|=1;assert_eq!(fs.save_large("big.txt",b"change"),Err(Error::ReadOnly));assert_eq!(fs.save_large("too.txt",&vec![0;65537]),Err(Error::ProgramTooLarge));assert_eq!(fs.file_size("too.txt"),Err(Error::NotFound));}
