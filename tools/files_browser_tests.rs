#[path = "../programs/files/browser.rs"] mod browser;
#[path = "../programs/files/selection.rs"] mod selection;
use browser::{History, Sort};

#[test]
fn every_drawn_cell_has_the_same_hit_target_and_partial_rows_are_excluded() {
    use browser::View;
    for view in [View::Details,View::Compact,View::Icons] {
        for (w,h) in [(640,400),(792,512),(680,432)] {
            let (_,_,_,count)=view.layout(w,h);
            for index in 0..count as usize {
                let (x,y,cw,ch)=view.cell(w,h,index);
                assert_eq!(view.hit(w,h,(x+cw/2) as i32,(y+ch/2) as i32,count as usize),Some(index));
                assert!(x+cw<=w-8 && y+ch<=h-32);
            }
            assert_eq!(view.hit(w,h,7,80,count as usize),None);
            assert_eq!(view.hit(w,h,8,(h-32) as i32,count as usize),None);
            let (x,y,_,_)=view.cell(w,h,1);
            assert_eq!(view.hit(w,h,x as i32,y as i32,1),None);
        }
    }
}

#[test]
fn favorites_codec_is_bounded_and_rejects_corrupt_or_duplicate_records() {
    use browser::Favorites;
    let mut f=Favorites::new();
    for path in [b"/".as_slice(),b"/DOCS",b"/APPS/FILES"] {assert_eq!(f.toggle(path),Ok(true));}
    let mut data=[0;1024];let n=f.encode(&mut data);
    let decoded=Favorites::decode(&data[..n]).unwrap();assert_eq!(decoded.count,3);
    for length in 0..n {assert!(Favorites::decode(&data[..length]).is_none());}
    assert!(Favorites::decode(&data[..n+1]).is_none());
    assert!(Favorites::decode(b"XFV1\x02\x01/\x01/").is_none());
    assert!(Favorites::decode(b"XFV1\x01\x05/../X").is_none());
    assert_eq!(f.toggle(b"/docs"),Ok(false));assert_eq!(f.count,2);
    for ch in b'A'..=b'F' {assert_eq!(f.toggle(&[b'/',ch]),Ok(true));}
    assert_eq!(f.toggle(b"/EXTRA"),Err(()));assert_eq!(f.count,8);
    assert_eq!(f.toggle(b"relative"),Err(()));
}

#[test]
fn history_branches_and_evicts_oldest_without_losing_current() {
    let mut h = History::new();
    h.record(b"/"); h.record(b"/A"); h.record(b"/B");
    h.cursor = h.target(false).unwrap();
    assert_eq!(h.target(true), Some(2));
    h.record(b"/C");
    assert_eq!(h.target(true), None);
    assert_eq!(&h.paths[2][..h.lengths[2]], b"/C");
    for n in b'D'..=b'Z' { h.record(&[b'/', n]); }
    assert_eq!(h.count, 8);
    assert_eq!(h.cursor, 7);
    assert_eq!(&h.paths[0][..2], b"/S");
    assert_eq!(&h.paths[7][..2], b"/Z");
}

#[test]
fn duplicate_and_invalid_history_entries_preserve_forward_navigation() {
    let mut h = History::new();
    assert_eq!(h.target(false), None); assert_eq!(h.target(true), None);
    h.record(b"/"); h.record(b"/A"); h.cursor = 0;
    h.record(b"/"); h.record(b""); h.record(&[b'A'; 106]);
    assert_eq!(h.target(true), Some(1)); assert_eq!(h.count, 2);
}

#[test]
fn sorts_whole_dataset_with_folder_grouping_and_deterministic_ties() {
    let mut rows = [(b"Z.TXT".as_slice(), 0, 9), (b"DIR".as_slice(), 1, 0),
                    (b"A.BIN".as_slice(), 0, 9), (b"B.TXT".as_slice(), 0, 1)];
    let mut sort = Sort::default(); sort.choose(3);
    rows.sort_unstable_by(|a,b| sort.compare(*a,*b));
    assert_eq!(rows.map(|r| r.0), [b"DIR".as_slice(), b"B.TXT", b"A.BIN", b"Z.TXT"]);
    sort.choose(3); rows.sort_unstable_by(|a,b| sort.compare(*a,*b));
    assert_eq!(rows.map(|r| r.0), [b"DIR".as_slice(), b"Z.TXT", b"A.BIN", b"B.TXT"]);
    sort.choose(2); assert!(!sort.descending);
    rows.sort_unstable_by(|a,b| sort.compare(*a,*b));
    assert_eq!(rows.map(|r| r.0), [b"DIR".as_slice(), b"A.BIN", b"B.TXT", b"Z.TXT"]);
}
