//! Filesystem adapter. Loads beside the resolved executable, never beside a guessed name.
use crate::{app_manifest::{self,Manifest,Access},permissions::Profile};
pub fn load(executable:&str)->Result<Option<Manifest>,&'static str> {
    let mut path=[0;105];let Some(n)=app_manifest::sidecar(executable.as_bytes(),&mut path) else {return Ok(None);};
    let path=core::str::from_utf8(&path[..n]).unwrap();
    let size=match crate::diskfs::file_size(path) {Ok(n)=>n,Err(crate::fat16::Error::NotFound)=>return Ok(None),Err(e)=>return Err(e.message())};
    if size==0||size>app_manifest::MAX_BYTES {return Err("Invalid application manifest size.");}
    let mut bytes=[0;app_manifest::MAX_BYTES];
    if crate::diskfs::read_at(path,0,&mut bytes[..size]).map_err(|e|e.message())?!=size {return Err("Application manifest changed while loading.");}
    Manifest::parse(&bytes[..size],executable.rsplit('/').next().unwrap().as_bytes()).map(Some).map_err(|_|"Invalid application manifest.")
}
pub fn profile(access:Access)->Profile {match access {Access::Trusted=>Profile::Trusted,Access::ReadOnly=>Profile::ReadOnly,Access::Media=>Profile::Media,Access::Isolated=>Profile::Isolated}}
