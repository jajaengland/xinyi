#![allow(dead_code)]
#[path="../src/app_manifest.rs"]mod app_manifest;
use app_manifest::*;
const VALID:&[u8]=b"XINYI-APP 1\nexec=TEST.ELF\ntitle=Example\ndescription=A test app\nicon=document\naccess=read-only\nhandles=txt,log,-\n";
#[test]fn manifest_fields_aliases_and_binding_are_bounded() {
 let m=Manifest::parse(VALID,b"test.elf").unwrap();assert_eq!(m.title(),b"Example");assert_eq!(m.description(),b"A test app");
 assert_eq!(m.access,Access::ReadOnly);assert_eq!(m.access.prefix(),b"--read-only ");assert_eq!(m.icon,3);
 assert!(m.handles(b"TXT"));assert!(m.handles(b"log"));assert!(m.handles(b""));assert!(!m.handles(b"wav"));
 assert!(matches!(Manifest::parse(VALID,b"OTHER.ELF"),Err(Error::Executable)));
 let mut out=[0;105];assert_eq!(sidecar(b"/APPS/TEST.ELF",&mut out),Some(14));assert_eq!(&out[..14],b"/APPS/TEST.APP");
 assert_eq!(sidecar(b"/APPS/../X.TXT",&mut out),None);
}
#[test]fn corrupt_oversized_duplicate_and_privilege_typos_fail_closed() {
 for (from,to) in [("access=read-only","access=admin"),("icon=document","icon=../../bad"),("handles=txt,log,-","handles=txt,TXT"),("handles=txt,log,-","handles=BIN"),("title=Example","title=   "),("title=Example","title=Bad\rTitle"),("exec=TEST.ELF","exec=../TEST.ELF")] {
  let bytes=std::str::from_utf8(VALID).unwrap().replace(from,to);assert!(Manifest::parse(bytes.as_bytes(),b"TEST.ELF").is_err(),"{to}");
 }
 let mut duplicate=VALID.to_vec();duplicate.extend_from_slice(b"access=trusted\n");assert!(matches!(Manifest::parse(&duplicate,b"TEST.ELF"),Err(Error::Duplicate)));
 assert!(matches!(Manifest::parse(&[b'x';513],b"TEST.ELF"),Err(Error::Size)));
 assert!(Manifest::parse(b"XINYI-APP 1\nexec=TEST.ELF\n",b"TEST.ELF").is_err());
 let crlf=std::str::from_utf8(VALID).unwrap().replace('\n',"\r\n");assert!(Manifest::parse(crlf.as_bytes(),b"TEST.ELF").is_ok());
}
#[test]fn arbitrary_input_never_panics_or_creates_unbounded_fields() {
 let mut seed=11u32;
 for size in 0..600 {
  let mut bytes=vec![0;size];for b in &mut bytes {seed=seed.wrapping_mul(1664525).wrapping_add(1013904223);*b=(seed>>24) as u8;}
  let _=Manifest::parse(&bytes,b"TEST.ELF");
 }
 for _ in 0..5000 {
  let mut bytes=VALID.to_vec();seed=seed.wrapping_mul(1664525).wrapping_add(1013904223);let at=seed as usize%bytes.len();bytes[at]=(seed>>24) as u8;
  if let Ok(m)=Manifest::parse(&bytes,b"TEST.ELF") {assert!(m.title().len()<=32);assert!(m.description().len()<=64);assert!(m.icon<=7);assert_eq!(&m.executable[..m.executable_len],b"TEST.ELF");}
 }
}
#[test]fn all_bundled_manifests_are_valid_and_associations_are_unambiguous() {
 let mut expected=std::collections::BTreeMap::new();
 for line in std::fs::read_to_string("programs/apps.txt").unwrap().lines() {
  let line=line.trim();if line.is_empty()||line.starts_with('#'){continue;}let fields:Vec<_>=line.split('|').collect();assert!((3..=5).contains(&fields.len()));
  let format=fields.get(3).copied().unwrap_or("flat");let ext=fields.get(4).copied().unwrap_or(if format=="elf"{"elf"}else{"bin"});
  let exec=format!("{}.{}",fields[0],ext).to_ascii_uppercase().into_bytes();assert!(expected.insert(format!("{}.app",fields[0]),exec).is_none());
 }
 assert!(!expected.is_empty());let mut extensions=std::collections::HashMap::new();
 for p in std::fs::read_dir("programs/manifests").unwrap() {
  let p=p.unwrap().path();if p.extension().and_then(|x|x.to_str())!=Some("app"){continue;}let bytes=std::fs::read(&p).unwrap();let exec=expected.remove(p.file_name().unwrap().to_str().unwrap()).expect("Manifest without a bundled app");
  let m=Manifest::parse(&bytes,&exec).unwrap();
  for ext in [b"txt" as &[u8],b"wav",b"dat",b"raw",b"hex",b"log",b"app",b""] {if m.handles(ext) {assert!(extensions.insert(ext.to_vec(),p.clone()).is_none(),"Duplicate type: {ext:?}");}}
 }
 assert!(expected.is_empty(),"Missing bundled manifests: {expected:?}");assert!(extensions.get(b"wav".as_slice()).unwrap().ends_with("player.app"));assert!(extensions.get(b"txt".as_slice()).unwrap().ends_with("notepad.app"));
}
