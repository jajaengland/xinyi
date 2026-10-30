//! Bounded tree snapshot shared by app installation and whole-system installation.
use crate::{diskfs, fat16::{Error,Result},fs_path::{Path,MAX_DEPTH,MAX_PATH}};
pub const LIMIT:usize=64;
#[derive(Clone,Copy)]
pub struct Item {pub path:Path,pub directory:bool,pub size:usize}
impl Item {const EMPTY:Self=Self{path:Path::ROOT,directory:false,size:0};}
pub struct Tree {pub items:[Item;LIMIT],pub len:usize,total:usize}
impl Tree {
    pub fn scan(root:Path)->Result<Self> {
        let mut tree=Self{items:[Item::EMPTY;LIMIT],len:1,total:0};
        tree.items[0]=Item{path:root,directory:true,size:0};tree.children(root)?;Ok(tree)
    }
    fn children(&mut self,parent:Path)->Result<()> {
        let mut bytes=[0;MAX_PATH];let n=parent.encode(&mut bytes);let start=self.len;let mut error=None;
        diskfs::list(core::str::from_utf8(&bytes[..n]).unwrap(),|name,size,directory| {
            if name[0]==b'.' {return;}
            let mut display=[0;12];let n=crate::fat16::display_name(name,&mut display);
            if !core::str::from_utf8(&display[..n]).ok().is_some_and(|text|crate::fat16::short_name(text).is_ok_and(|encoded|encoded==*name)) {error=Some(Error::InvalidName);return;}
            if self.len==LIMIT||parent.depth==MAX_DEPTH {error=Some(Error::TooLarge);return;}
            if size>crate::loader::MAX_BINARY_BYTES {error=Some(Error::ImageTooLarge);return;}
            self.total=self.total.saturating_add(size);
            if self.total>8*1024*1024 {error=Some(Error::TooLarge);return;}
            let mut path=parent;path.names[path.depth]=*name;path.depth+=1;
            self.items[self.len]=Item{path,directory,size};self.len+=1;
        })?;
        if let Some(e)=error {return Err(e);}
        let end=self.len;
        for i in start..end {if self.items[i].directory {self.children(self.items[i].path)?;}}
        Ok(())
    }
    pub fn mapped(&self,index:usize,destination:Path)->Result<Path> {
        let source=self.items[0].path;let item=self.items[index].path;
        let depth=destination.depth+item.depth-source.depth;
        if depth>MAX_DEPTH {return Err(Error::PathTooDeep);}
        let mut path=destination;
        path.names[destination.depth..depth].copy_from_slice(&item.names[source.depth..item.depth]);path.depth=depth;Ok(path)
    }
    pub fn remove(&self,destination:Path)->Result<()> {
        // Children may follow siblings in the snapshot; depth order is explicit.
        for depth in (destination.depth..=MAX_DEPTH).rev() {
            for i in (0..self.len).rev() {
                let path=self.mapped(i,destination)?;if path.depth!=depth {continue;}
                let mut bytes=[0;MAX_PATH];let n=path.encode(&mut bytes);let text=core::str::from_utf8(&bytes[..n]).unwrap();
                if self.items[i].directory {diskfs::rmdir(text)?;}else{diskfs::delete(text)?;}
            }
        }Ok(())
    }
}
