use crate::{app_access::{Table,Restriction,MAX_BYTES},permissions::Profile};
pub fn profile(resolved:&str)->Result<Profile,&'static str> {
 let path=crate::fs_path::Path::parse(resolved,&crate::fs_path::Path::ROOT).map_err(|e|e.message())?;
 if path.depth<3||path.names[0]!=*b"APPS       " {return Ok(Profile::Trusted);}
 let mut id=[0;12];let n=crate::fat16::display_name(&path.names[1],&mut id);
 let size=match crate::diskfs::file_size("/ACCESS.CFG"){Ok(s)=>s,Err(crate::fat16::Error::NotFound)=>return Ok(Profile::Trusted),Err(e)=>return Err(e.message())};
 if size>MAX_BYTES{return Err("Invalid /ACCESS.CFG. Use the console to inspect or remove it.");}
 let mut bytes=[0;MAX_BYTES];if crate::diskfs::read_at("/ACCESS.CFG",0,&mut bytes[..size]).map_err(|e|e.message())?!=size{return Err("Access configuration changed while loading.");}
 let table=Table::parse(&bytes[..size]).map_err(|_|"Invalid /ACCESS.CFG. Use the console to inspect or remove it.")?;
 Ok(match table.get(&id[..n]){Some(Restriction::ReadOnly)=>Profile::ReadOnly,Some(Restriction::Media)=>Profile::Media,Some(Restriction::Isolated)=>Profile::Isolated,None=>Profile::Trusted})
}
