#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
use xinyi::{files,directory};
fn save_manifest(path:&[u8],exec:&[u8],access:&[u8],title:&[u8],handles:&[u8]) {
    let mut out=[0;512];let mut n=0;
    for part in [b"XINYI-APP 1\nexec=" as &[u8],exec,b"\ntitle=",title,b"\ndescription=Manifest fixture\nicon=document\naccess=",access,b"\nhandles=",handles,b"\n"] {out[n..n+part.len()].copy_from_slice(part);n+=part.len();}
    assert_eq!(files::save(path,&out[..n]),n as i32);
}
fn main()->u32 {
    let mut args=[0;128];let n=xinyi::arguments(&mut args).max(0) as usize;let args=&args[..n];
    match args {
        b"make-ro"=>save_manifest(b"/APPS/APTEST.APP",b"APTEST.ELF",b"read-only",b"Restricted app",b""),
        b"make-media"=>save_manifest(b"/APPS/APTEST.APP",b"APTEST.ELF",b"media",b"Media app",b""),
        b"make-trusted"=>save_manifest(b"/APPS/APTEST.APP",b"APTEST.ELF",b"trusted",b"Test app",b""),
        b"make-bad"=>{assert!(files::save(b"/APPS/APTEST.APP",b"XINYI-APP 1\nexec=WRONG.ELF\n")>0);},
        b"remove"=>{assert_eq!(directory::action(b"/APPS/APTEST.APP",1),0);},
        b"title"=>save_manifest(b"/APPS/NOTEPAD/NOTEPAD.APP",b"NOTEPAD.BIN",b"trusted",b"Notebook Plus",b"txt,log,md,cfg,ini,csv,app,-"),
        b"player-trusted"=>save_manifest(b"/APPS/PLAYER/PLAYER.APP",b"PLAYER.ELF",b"trusted",b"Player",b"wav"),
        b"ambiguous"=>save_manifest(b"/APPS/METAHELP.APP",b"METAHELP.ELF",b"trusted",b"Other handler",b"txt"),
        b"clear"=>{assert_eq!(directory::action(b"/APPS/METAHELP.APP",1),0);},
        b"api"=>{
            let volume=directory::volume();assert_eq!(xinyi::launch_on(b"counter",volume.wrapping_add(1)),-116);
            assert_eq!(xinyi::syscall(101,1,10,volume),-14);
            assert_eq!(xinyi::associations::open(b"unknown.zzz",volume),-2);
            assert_eq!(xinyi::associations::open(b"test.bin",volume),-22);
            assert_eq!(xinyi::associations::open(b"readme.txt",volume),-16); // Resolves, but physical mode cannot queue a GUI app.
        }
        b"ambiguous-check"=>{assert_eq!(xinyi::associations::open(b"readme.txt",directory::volume()),-17);},
        b"docs"=>{
            assert_eq!(directory::action(b"/DOCS",0),0);
            assert_eq!(files::save(b"/DOCS/README.TXT",b"manifest document"),17);
            let mut wav=[0u8;4140];wav[..4].copy_from_slice(b"RIFF");wav[4..8].copy_from_slice(&4132u32.to_le_bytes());wav[8..16].copy_from_slice(b"WAVEfmt ");wav[16..20].copy_from_slice(&16u32.to_le_bytes());wav[20..22].copy_from_slice(&1u16.to_le_bytes());wav[22..24].copy_from_slice(&1u16.to_le_bytes());wav[24..28].copy_from_slice(&16000u32.to_le_bytes());wav[28..32].copy_from_slice(&32000u32.to_le_bytes());wav[32..34].copy_from_slice(&2u16.to_le_bytes());wav[34..36].copy_from_slice(&16u16.to_le_bytes());wav[36..40].copy_from_slice(b"data");wav[40..44].copy_from_slice(&4096u32.to_le_bytes());
            for i in 0..2048 {let value=if i%32<16 {10000i16}else{-10000};wav[44+i*2..46+i*2].copy_from_slice(&value.to_le_bytes());}
            assert_eq!(files::save(b"/DOCS/TONE.WAV",&wav),4140);assert_eq!(files::save(b"/DOCS/UNKNOWN.ZZZ",b"unknown"),7);
            assert_eq!(files::save(b"/DOCS/DATA.DAT",&[0xde,0xad,0xbe,0xef]),4);
        }
        _=>{
            let expected=match args {b"ro"=>1,b"media"=>33,b"none"=>0,b"trusted"=>63,_=>return 2};
            assert_eq!(xinyi::permissions::current(),expected);
            if expected!=63 {assert_eq!(files::save(b"/SHOULD.TXT",b"bad"),-13);assert_eq!(xinyi::syscall(101,u32::MAX,u32::MAX,u32::MAX),-13);}
        }
    }
    xinyi::print("Manifest checks passed!\n");0
}
