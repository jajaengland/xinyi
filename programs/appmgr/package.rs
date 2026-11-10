//! User-mode extraction into a reserved, freshly-created folder; kernel validates publication.
use alloc::vec::Vec;
use crate::xinyi::{directory,files};


use crate::app_package::{Package,MAX_BYTES};
pub fn load(path:&[u8],volume:u32)->Result<Vec<u8>,i32> {
    let size=files::stream_size_on(path,volume);if size<0{return Err(size);}if size<24||size as usize>MAX_BYTES{return Err(-7);}
    let mut bytes=Vec::new();bytes.try_reserve_exact(size as usize).map_err(|_|-12)?;bytes.resize(size as usize,0);
    for (i,chunk) in bytes.chunks_mut(files::MAX_CHUNK).enumerate() {
        let n=files::read_at_on(path,(i*files::MAX_CHUNK) as u32,chunk,volume);
        if n!=chunk.len() as i32{return Err(if n<0 {n}else{-74});}
    }
    if files::stream_size_on(path,volume)!=size {return Err(-74);}
    Package::parse(&bytes).map_err(|_|-74)?;Ok(bytes)
}
fn checked(result:i32)->Result<(),i32> {if result<0 {Err(result)}else{Ok(())}}
pub fn install(bytes:&[u8],operation:u16,volume:u32)->Result<(),i32> {
    let pkg=Package::parse(bytes).map_err(|_|-74)?;
    // Refuse leftovers: never replace or clean up a pre-existing extraction folder.
    checked(directory::action_on(b"/PKGIN",0,volume))?;
    let mut root=[0;16];root[..7].copy_from_slice(b"/PKGIN/");root[7..7+pkg.id.len()].copy_from_slice(pkg.id);let root=&root[..7+pkg.id.len()];
    let mut created=0;let mut bundle=false;
    let result=(|| {
        checked(directory::action_on(root,0,volume))?;bundle=true;
        for e in &pkg.entries[..pkg.len] {
            let mut path=[0;105];let n=root.len();path[..n].copy_from_slice(root);path[n]=b'/';path[n+1..n+1+e.path.len()].copy_from_slice(e.path);let path=&path[..n+1+e.path.len()];
            if e.directory {checked(directory::action_on(path,0,volume))?;created+=1;}
            else {
                checked(files::resize_on(path,0,volume))?;created+=1;
                for (i,chunk) in e.data.chunks(files::MAX_CHUNK).enumerate() {
                    let n=files::write_at_on(path,(i*files::MAX_CHUNK) as u32,chunk,volume);
                    if n!=chunk.len() as i32 {return Err(if n<0 {n}else{-74});}
                }
            }
        }
        checked(directory::app_action(root,operation,volume))
    })();
    let mut cleanup=Ok(());
    for e in pkg.entries[..created].iter().rev() {
        let mut path=[0;105];let n=root.len();path[..n].copy_from_slice(root);path[n]=b'/';path[n+1..n+1+e.path.len()].copy_from_slice(e.path);
        if let Err(code)=checked(directory::action_on(&path[..n+1+e.path.len()],if e.directory {2}else{1},volume)) {cleanup=Err(code);}
    }
    if bundle {if let Err(code)=checked(directory::action_on(root,2,volume)){cleanup=Err(code);}}
    if let Err(code)=checked(directory::action_on(b"/PKGIN",2,volume)){cleanup=Err(code);}
    // A failed cleanup is reported distinctly even if publication succeeded.
    if cleanup.is_err() {return Err(-117);}result
}
