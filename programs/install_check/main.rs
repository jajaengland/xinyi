#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
use xinyi::{directory,files};
fn main()->u32 {
    let mut args=[0;128];let n=xinyi::arguments(&mut args).max(0) as usize;
    let volume=directory::volume();
    match &args[..n] {
        b"restricted"=>assert_eq!(xinyi::syscall(102,u32::MAX,220,0),-13),
        b"api"=>{
            assert_eq!(xinyi::syscall(102,1,220,0),-14);
            assert_eq!(xinyi::syscall(102,0,219,0),-22);
            assert_eq!(directory::app_action(b"/INBOX/SAMPLE",0,volume.wrapping_add(1)),-116);
            assert_eq!(directory::app_action(b"/INBOX/SAMPLE",1,volume),-22);
            assert_eq!(directory::app_action(b"/APPS",1,volume),-22);
            assert_eq!(directory::app_action(b"/APPS/_STAGE",1,volume),-22);
            assert_eq!(directory::app_action(b"/INBOX/INVALID",0,volume),-8);
            assert_eq!(directory::action(b"/INBOX/SAMPLE/ASSETS",0),0);
            assert_eq!(directory::action(b"/INBOX/SAMPLE/ASSETS/MORE",0),0);
            assert_eq!(files::save(b"/INBOX/SAMPLE/ASSETS/MORE/DATA.TXT",b"keep this asset"),15);
            assert_eq!(files::save(b"/INBOX/SAMPLE/ASSETS/EMPTY.TXT",b""),0);
        },
        b"install"=>{
            assert_eq!(directory::app_action(b"/INBOX/SAMPLE",0,volume),0);
            assert_eq!(directory::app_action(b"/INBOX/SAMPLE",0,volume),-17);
            assert_eq!(directory::app_action(b"/APPS/SAMPLE",0,volume),-22);
            assert_eq!(directory::app_action(b"/APPS/SAMPLE",1,volume),0);
            assert_eq!(directory::app_action(b"/INBOX/BIGTEST",0,volume),0);
        },
        b"cleanup"=>assert_eq!(directory::app_action(b"/APPS/BIGTEST",1,volume),0),
        b"stage"=>{
            assert_eq!(directory::action(b"/APPS/_STAGE",0),0);
            assert_eq!(files::save(b"/APPS/_STAGE/KEEP.TXT",b"recovery"),8);
            assert_eq!(directory::app_action(b"/INBOX/SAMPLE",0,volume),-17);
            let mut b=[0;1024];assert_eq!(xinyi::read_file(b"/APPS/_STAGE/KEEP.TXT",&mut b),8);
            assert_eq!(directory::action(b"/APPS/_STAGE/KEEP.TXT",1),0);
            assert_eq!(directory::action(b"/APPS/_STAGE",2),0);
        },
        _=>return 2,
    }
    xinyi::print("App installation checks passed!\n");0
}
