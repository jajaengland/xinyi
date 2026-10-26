// Volatile fallback. Called only by the shell; IRQs never access file state.
use crate::fat16::{Error, Result, MAX_FILE_SIZE};
use crate::fs_path::Path;
const ROOT: u8 = 32;
#[derive(Clone, Copy)]
struct File {
    name: [u8; 11],
    data: [u8; MAX_FILE_SIZE],
    size: usize,
    used: bool,
    parent: u8,
    directory: bool,
}
static mut FILES: [File; 32] = [File {
    name: [0; 11],
    data: [0; MAX_FILE_SIZE],
    size: 0,
    used: false,
    parent: ROOT,
    directory: false,
}; 32];
pub fn init() {
    for file in unsafe { &mut *core::ptr::addr_of_mut!(FILES) } {
        file.used = false;
        file.size = 0;
    }
}
fn walk(files: &[File; 32], path: &Path, count: usize) -> Result<u8> {
    let mut parent = ROOT;
    for name in &path.names[..count] {
        let index = files
            .iter()
            .position(|file| file.used && file.parent == parent && &file.name == name)
            .ok_or(Error::NotFound)?;
        if !files[index].directory {
            return Err(Error::NotDirectory);
        }
        parent = index as u8;
    }
    Ok(parent)
}
fn parent(text: &str, files: &[File; 32]) -> Result<(u8, [u8; 11])> {
    let path = Path::parse(text, &Path::ROOT)?;
    if path.depth == 0 {
        return Err(Error::InvalidName);
    }
    Ok((
        walk(files, &path, path.depth - 1)?,
        path.names[path.depth - 1],
    ))
}
pub fn create(name: &str) -> Result<()> {
    create_node(name, false)
}
pub fn mkdir(name: &str) -> Result<()> {
    create_node(name, true)
}
fn create_node(name: &str, directory: bool) -> Result<()> {
    let files = unsafe { &mut *core::ptr::addr_of_mut!(FILES) };
    let (parent, name) = parent(name, files)?;
    if files
        .iter()
        .any(|f| f.used && f.parent == parent && f.name == name)
    {
        return Err(Error::Exists);
    }
    let file = files.iter_mut().find(|f| !f.used).ok_or(Error::Full)?;
    file.name = name;
    file.parent = parent;
    file.directory = directory;
    file.size = 0;
    file.used = true;
    Ok(())
}
pub fn write(name: &str, data: &[u8]) -> Result<()> {
    if data.len() > MAX_FILE_SIZE {
        return Err(Error::TooLarge);
    }
    let files = unsafe { &mut *core::ptr::addr_of_mut!(FILES) };
    let (parent, name) = parent(name, files)?;
    let file = files
        .iter_mut()
        .find(|f| f.used && f.parent == parent && f.name == name)
        .ok_or(Error::NotFound)?;
    if file.directory {
        return Err(Error::NotFile);
    }
    file.data[..data.len()].copy_from_slice(data);
    file.size = data.len();
    Ok(())
}
pub fn read(name: &str, output: &mut [u8; MAX_FILE_SIZE]) -> Result<usize> {
    let files = unsafe { &*core::ptr::addr_of!(FILES) };
    let (parent, name) = parent(name, files)?;
    let file = files
        .iter()
        .find(|f| f.used && f.parent == parent && f.name == name)
        .ok_or(Error::NotFound)?;
    if file.directory {
        return Err(Error::NotFile);
    }
    output[..file.size].copy_from_slice(&file.data[..file.size]);
    Ok(file.size)
}
pub fn delete(name: &str) -> Result<()> {
    let files = unsafe { &mut *core::ptr::addr_of_mut!(FILES) };
    let (parent, name) = parent(name, files)?;
    let file = files
        .iter_mut()
        .find(|file| file.used && file.parent == parent && file.name == name)
        .ok_or(Error::NotFound)?;
    if file.directory {
        return Err(Error::NotFile);
    }
    file.data.fill(0);
    file.name.fill(0);
    file.size = 0;
    file.used = false;
    Ok(())
}
pub fn rmdir(name: &str) -> Result<()> {
    if Path::parse(name, &Path::ROOT)?.depth == 0 {
        return Err(Error::DirectoryBusy);
    }
    let files = unsafe { &mut *core::ptr::addr_of_mut!(FILES) };
    let (parent, name) = parent(name, files)?;
    let index = files
        .iter()
        .position(|file| file.used && file.parent == parent && file.name == name)
        .ok_or(Error::NotFound)?;
    if !files[index].directory {
        return Err(Error::NotDirectory);
    }
    if files
        .iter()
        .any(|file| file.used && file.parent == index as u8)
    {
        return Err(Error::DirectoryNotEmpty);
    }
    files[index].used = false;
    Ok(())
}
pub fn is_directory(text: &str) -> Result<()> {
    let path = Path::parse(text, &Path::ROOT)?;
    walk(unsafe { &*core::ptr::addr_of!(FILES) }, &path, path.depth).map(|_| ())
}
pub fn list_entries(text: &str, mut visit: impl FnMut(&[u8; 11], usize, bool)) -> Result<()> {
    let path = Path::parse(text, &Path::ROOT)?;
    let files = unsafe { &*core::ptr::addr_of!(FILES) };
    let parent = walk(files, &path, path.depth)?;
    for file in files {
        if file.used && file.parent == parent {
            visit(&file.name, file.size, file.directory);
        }
    }
    Ok(())
}
