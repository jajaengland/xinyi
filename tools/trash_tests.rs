#[path="../src/trash_model.rs"]mod model;
#[test]fn records_validate_paths_and_state(){for p in [b"/DOCS/A.TXT" as &[u8],b"/DESKTOP/FOLDER"] {let mut r=model::Record::new(p,false).unwrap();let mut data=[0;112];r.ready=true;let n=r.encode(&mut data);let copy=model::Record::parse(&data[..n]).unwrap();assert!(copy.ready);assert_eq!(&copy.path[..copy.len],p);data[4]=3;assert!(model::Record::parse(&data[..n]).is_none());}for p in [b"/TRASH/A" as &[u8],b"/trash",b"/A/../B",b"/A//B",b"/",b"/LONGERNAME.TXT",b"/A/",b"/A.TOOO"]{assert!(!model::valid_path(p));}}
#[test]fn ids_are_bounded_and_roundtrip(){for i in 0..1024 {let path=model::entry(i);assert_eq!(model::id(&path[7..]),Some(i));}assert!(model::id(b"T0001024").is_none());assert!(model::id(b"TAAAAAAA").is_none());}

#[test]fn purge_state_roundtrips_and_plan_is_bounded(){
 let mut r=model::Record::new(b"/DOCS/A",true).unwrap();r.purging=true;let mut data=[0;112];let n=r.encode(&mut data);let r=model::Record::parse(&data[..n]).unwrap();assert!(r.purging&&!r.ready);
 let mut plan=model::Plan::new(b"/TRASH/T0000000/ITEM",true);assert!(plan.append(0,b"..",true).is_err());assert!(plan.append(0,b"/BAD",false).is_err());plan.append(0,b"DIR",true).unwrap();plan.append(1,b"FILE.TXT",false).unwrap();assert_eq!(&plan.nodes[2].path[..plan.nodes[2].len],b"/TRASH/T0000000/ITEM/DIR/FILE.TXT");
 let mut plan=model::Plan::new(b"/TRASH/T0000000/ITEM",true);for i in 0..31 {let name=[b'A'+(i/26) as u8,b'A'+(i%26) as u8];plan.append(0,&name,false).unwrap();}assert_eq!(plan.append(0,b"OVER",false),Err(-27));assert_eq!(plan.count,32);
}
