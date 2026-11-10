#[path="../src/app_manifest.rs"]mod app_manifest;
#[path="../src/app_package.rs"]mod app_package;
use app_package::{Package,crc};
fn fixture(entries:Vec<(&str,bool,Vec<u8>)>)->Vec<u8> {
    let mut b=b"XINYIPK1DEMO\0\0\0\0".to_vec();b.extend_from_slice(&(entries.len() as u16).to_le_bytes());b.extend_from_slice(&[0;6]);
    for (path,dir,data) in entries {b.extend_from_slice(&[path.len() as u8,dir as u8,0,0]);b.extend_from_slice(&(data.len() as u32).to_le_bytes());b.extend_from_slice(path.as_bytes());b.extend(data);}
    seal(&mut b);b
}
fn seal(b:&mut[u8]) {let c=crc(b[..20].iter().chain(b[24..].iter()).copied());b[20..24].copy_from_slice(&c.to_le_bytes());}
fn base()->Vec<(&'static str,bool,Vec<u8>)> {vec![("DEMO.BIN",false,vec![1]),("DEMO.APP",false,b"XINYI-APP 1\nexec=DEMO.BIN\ntitle=Demo\ndescription=Offline test\nicon=application\naccess=read-only\nhandles=\nversion=2\n".to_vec())]}
#[test]fn files_directories_and_manifest() {let mut e=base();e.extend([("DATA",true,vec![]),("DATA/A.TXT",false,b"content".to_vec()),("EMPTY",true,vec![])]);let b=fixture(e);let p=Package::parse(&b).unwrap();assert_eq!(p.id,b"DEMO");assert_eq!(p.manifest.version,2);assert_eq!(p.entries[3].data,b"content");assert_eq!(p.len,5);}
#[test]fn corruption_and_truncation() {let b=fixture(base());for n in 0..b.len(){assert!(Package::parse(&b[..n]).is_err());}let mut corrupt=b.clone();corrupt[8]=b'X';assert!(Package::parse(&corrupt).is_err());let n=corrupt.len()-1;corrupt[n]^=1;assert!(Package::parse(&corrupt).is_err());}
#[test]fn unsafe_paths_and_conflicts() {for path in ["../A","/A","A/../B","A//B","A/B","a.txt","A\\B","A.TOOO","A.",".",".."][..].iter() {let mut e=base();e.push((path,false,vec![]));assert!(Package::parse(&fixture(e)).is_err(),"{path}");}let mut e=base();e.push(("DEMO.APP",false,vec![]));assert!(Package::parse(&fixture(e)).is_err());}
#[test]fn invalid_headers_lengths_and_counts() {let valid=fixture(base());for (index,value) in [(0,b'Z'),(16,64),(18,1),(8,b'1'),(24,255),(25,2),(26,1)] {let mut b=valid.clone();b[index]=value;seal(&mut b);assert!(Package::parse(&b).is_err());}let mut b=valid.clone();b[28..32].copy_from_slice(&u32::MAX.to_le_bytes());seal(&mut b);assert!(Package::parse(&b).is_err());let mut b=valid;b.push(0);seal(&mut b);assert!(Package::parse(&b).is_err());}
#[test]fn invalid_bundle_identity_and_manifest() {let mut e=base();e.push(("DEMO.ELF",false,vec![1]));assert!(Package::parse(&fixture(e)).is_err());let mut e=base();e[1].2=b"garbage".to_vec();assert!(Package::parse(&fixture(e)).is_err());let mut e=base();e[0].2.clear();assert!(Package::parse(&fixture(e)).is_err());}
#[test]fn limits_and_directory_payloads() {let mut e=base();e.push(("D",true,vec![1]));assert!(Package::parse(&fixture(e)).is_err());let mut e=base();e.push(("BIG.BIN",false,vec![0;app_package::MAX_BYTES]));assert!(Package::parse(&fixture(e)).is_err());assert_eq!(crc(b"123456789".iter().copied()),0xcbf43926);}

#[test]fn maximum_entry_count_and_file_parents() {
    let names:Vec<String>=(0..62).map(|i|format!("A{i:02}.DAT")).collect();let mut e=base();
    for name in &names[..61] {e.push((name.as_str(),false,vec![]));}
    assert!(Package::parse(&fixture(e.clone())).is_ok());e.push((&names[61],false,vec![]));assert!(Package::parse(&fixture(e)).is_err());
    let mut e=base();e.extend([("DATA",false,vec![]),("DATA/A.TXT",false,vec![])]);assert!(Package::parse(&fixture(e)).is_err());
}
#[test]fn directory_depth_bound() {
    let paths:Vec<String>=(1..=7).map(|n|vec!["D";n].join("/")).collect();let mut e=base();
    for path in &paths[..6] {e.push((path.as_str(),true,vec![]));}
    assert!(Package::parse(&fixture(e.clone())).is_ok());e.push((&paths[6],true,vec![]));assert!(Package::parse(&fixture(e)).is_err());
}
