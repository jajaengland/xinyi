#[path="../src/app_catalog.rs"]mod app_catalog;
use app_catalog::Catalog;
#[test] fn catalog_filters_sorts_and_uses_explicit_app_paths() {
    let mut c=Catalog::new();c.add(b"NOTEPAD BIN",false);c.add(b"HELLO   BIN",false);c.add(b"NOTES   TXT",false);c.add(b"FOLDER  BIN",true);c.add(b"BAD/PATHBIN",false);c.sort();
    assert_eq!(c.len,2);assert_eq!(&c.entries[0].path[..c.entries[0].path_len],b"/APPS/HELLO.BIN");assert_eq!(&c.entries[1].label[..c.entries[1].label_len],b"NOTEPAD");
}
#[test] fn catalog_bounds_popup_count_and_reports_more_files() {
    let mut c=Catalog::new();for i in 0..17 {let mut name=*b"APP00   BIN";name[3]=b'0'+i/10;name[4]=b'0'+i%10;c.add(&name,false);}assert_eq!(c.len,16);assert!(c.truncated);
}

#[test]fn elf_apps_are_listed_and_bin_wins_duplicate_stems(){let mut c=Catalog::new();c.add(b"TEST    ELF",false);assert_eq!(&c.entries[0].path[..c.entries[0].path_len],b"/APPS/TEST.ELF");c.add(b"TEST    BIN",false);c.add(b"TEST    ELF",false);assert_eq!(c.len,1);assert_eq!(&c.entries[0].path[..c.entries[0].path_len],b"/APPS/TEST.BIN");}
#[test]fn bundles_have_explicit_paths_and_legacy_duplicates_win(){
    let mut c=Catalog::new();c.add_bundle(b"NOTEPAD BIN");c.add_bundle(b"ABCDEFGHELF");
    assert_eq!(&c.entries[0].path[..c.entries[0].path_len],b"/APPS/NOTEPAD/NOTEPAD.BIN");
    assert_eq!(&c.entries[1].path[..c.entries[1].path_len],b"/APPS/ABCDEFGH/ABCDEFGH.ELF");
    c.add_bundle(b"NOTEPAD BIN");assert_eq!(c.len,2);
    let mut c=Catalog::new();c.add(b"HELLO   BIN",false);c.add_bundle(b"HELLO   BIN");
    assert_eq!(&c.entries[0].path[..c.entries[0].path_len],b"/APPS/HELLO.BIN");
}
