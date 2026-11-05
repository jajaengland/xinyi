#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
use xinyi::{directory,files};
fn content(path:&[u8],expected:&[u8]) {
    let mut b=[0;1024];let n=xinyi::read_file(path,&mut b);assert_eq!(n,expected.len() as i32);assert_eq!(&b[..expected.len()],expected);
}
fn action(path:&[u8],op:u16)->i32 {directory::app_action(path,op,directory::volume())}
fn main()->u32 {
    let mut args=[0;128];let n=xinyi::arguments(&mut args).max(0) as usize;
    match &args[..n] {
        b"restricted"=>assert_eq!(xinyi::syscall(102,u32::MAX,220,0),-13),
        b"suite"=>{
            assert_eq!(action(b"/BASE/SAMPLE",0),0);
            content(b"/APPS/SAMPLE/DATA.TXT",b"old asset");
            assert_eq!(action(b"/LOW/SAMPLE",2),-8);
            assert_eq!(action(b"/ESC/SAMPLE",2),-8);
            assert_eq!(action(b"/BAD/SAMPLE",2),-8);
            content(b"/APPS/SAMPLE/DATA.TXT",b"old asset");
            assert_eq!(directory::action(b"/APPS/_STAGE",0),0);
            assert_eq!(files::save(b"/APPS/_STAGE/KEEP.TXT",b"recovery"),8);
            assert_eq!(action(b"/NEW/SAMPLE",2),-17);
            content(b"/APPS/_STAGE/KEEP.TXT",b"recovery");content(b"/APPS/SAMPLE/DATA.TXT",b"old asset");
            assert_eq!(directory::action(b"/APPS/_STAGE/KEEP.TXT",1),0);assert_eq!(directory::action(b"/APPS/_STAGE",2),0);
            assert_eq!(action(b"/NEW/SAMPLE",2),0);
            content(b"/APPS/SAMPLE/DATA.TXT",b"new asset");content(b"/APPS/_BACKUP/DATA.TXT",b"old asset");
            assert_eq!(action(b"/APPS/OTHER",4),-22);
            content(b"/APPS/_BACKUP/DATA.TXT",b"old asset");
            assert_eq!(action(b"/NEW/SAMPLE",2),-8);
            let mut original=[0;1024];let oldn=xinyi::read_file(b"/BASE/SAMPLE/SAMPLE.BIN",&mut original);assert!(oldn>0);
            assert_eq!(files::save(b"/APPS/_BACKUP/SAMPLE.BIN",b"\x7fELFbad"),7);
            assert_eq!(action(b"/APPS/SAMPLE",3),-8);content(b"/APPS/SAMPLE/DATA.TXT",b"new asset");
            assert_eq!(files::save(b"/APPS/_BACKUP/SAMPLE.BIN",&original[..oldn as usize]),oldn);
            assert_eq!(action(b"/APPS/SAMPLE",3),0);
            content(b"/APPS/SAMPLE/DATA.TXT",b"old asset");
            assert_eq!(action(b"/NEW/SAMPLE",2),0);
            assert_eq!(action(b"/APPS/SAMPLE",4),0);
            content(b"/APPS/SAMPLE/DATA.TXT",b"new asset");
            assert_eq!(action(b"/APPS/SAMPLE",3),-2);
            // Old manifests remain installable and are version zero for upgrades.
            assert_eq!(action(b"/BASE/RUNBOX",0),0);
        },
        b"busy"=>assert_eq!(action(b"/NEW/RUNBOX",2),-16),
        b"finish"=>{
            content(b"/APPS/RUNBOX/DATA.TXT",b"new asset");
            // Simulate an interrupted publication: old bundle survives in _BACKUP.
            assert_eq!(action(b"/APPS/RUNBOX",1),0);
            assert_eq!(action(b"/APPS/RUNBOX",3),0);
            content(b"/APPS/RUNBOX/DATA.TXT",b"old asset");
            assert_eq!(action(b"/APPS/RUNBOX",1),0);
            assert_eq!(action(b"/APPS/SAMPLE",1),0);
        },
        _=>return 2,
    }
    xinyi::print("App update checks passed!\n");0
}
