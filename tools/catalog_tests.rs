#[path="../src/app_catalog.rs"]mod app_catalog;
use app_catalog::Catalog;
#[test] fn catalog_filters_sorts_and_uses_explicit_app_paths() {
    let mut c=Catalog::new();c.add(b"NOTEPAD BIN",false);c.add(b"HELLO   BIN",false);c.add(b"NOTES   TXT",false);c.add(b"FOLDER  BIN",true);c.add(b"BAD/PATHBIN",false);c.sort();
    assert_eq!(c.len,2);assert_eq!(&c.entries[0].path[..c.entries[0].path_len],b"/APPS/HELLO.BIN");assert_eq!(&c.entries[1].label[..c.entries[1].label_len],b"NOTEPAD");
}
#[test] fn catalog_bounds_popup_count_and_reports_more_files() {
    let mut c=Catalog::new();for _ in 0..17 {c.add(b"HELLO   BIN",false);}assert_eq!(c.len,16);assert!(c.truncated);
}
