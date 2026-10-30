//! Range reads and copy-on-write range edits. Memory is bounded independently
//! of file size. Full chain validation precedes every read and mutation.
use super::*;
use crate::stream_protocol::{MAX_CHUNK, MAX_CLUSTERS, MAX_FILE};

impl Fat16<'_> {
    /// Kernel loader/installer path. User whole-file APIs retain their own limits.
    pub fn save_image(&self,path:&str,input:&[u8])->Result<()> {
        if input.len()>MAX_FILE {return Err(Error::ImageTooLarge);}
        self.edit_range(path,0,input,Some(input.len()))
    }

    pub fn read_at(&self, path: &str, offset: u32, output: &mut [u8]) -> Result<usize> {
        if output.len() > MAX_CHUNK {
            return Err(Error::TooLarge);
        }
        let (directory, name) = self.parent(path)?;
        let entry = directory.find(&name)?;
        if entry.data[11] & 0x10 != 0 {
            return Err(Error::NotFile);
        }
        let (chain, _) = self.bounded_chain::<MAX_CLUSTERS>(&entry, MAX_FILE)?;
        let offset = offset as usize;
        let length = entry.size().saturating_sub(offset).min(output.len());
        let cluster_bytes = self.sectors_per_cluster as usize * 512;
        let mut copied = 0;
        while copied < length {
            let pos = offset + copied;
            let cluster = chain[pos / cluster_bytes];
            let mut data = [0; 512];
            self.read_sector(
                self.cluster_sector(cluster) + (pos % cluster_bytes / 512) as u32,
                &mut data,
            )?;
            let inside = pos % 512;
            let count = (512 - inside).min(length - copied);
            output[copied..copied + count].copy_from_slice(&data[inside..inside + count]);
            copied += count;
        }
        Ok(copied)
    }
    /// Creates missing files, preserves bytes outside the patch, zero-fills gaps.
    pub fn write_at(&self, path: &str, offset: u32, input: &[u8]) -> Result<()> {
        if input.len() > MAX_CHUNK {
            return Err(Error::TooLarge);
        }
        let end = (offset as usize)
            .checked_add(input.len())
            .ok_or(Error::ProgramTooLarge)?;
        if end > MAX_FILE {
            return Err(Error::ProgramTooLarge);
        }
        self.edit_range(path, offset as usize, input, None)
    }
    /// Creates an empty file when missing; growth is physically zero-filled.
    pub fn resize_file(&self, path: &str, length: u32) -> Result<()> {
        if length as usize > MAX_FILE {
            return Err(Error::ProgramTooLarge);
        }
        self.edit_range(path, 0, &[], Some(length as usize))
    }
    fn edit_range(
        &self,
        path: &str,
        offset: usize,
        input: &[u8],
        resize: Option<usize>,
    ) -> Result<()> {
        let (mut directory, name) = self.parent(path)?;
        if directory.directory_readonly {
            return Err(Error::ReadOnly);
        }
        let mut missing = false;
        let mut entry = match directory.find(&name) {
            Ok(entry) => entry,
            Err(Error::NotFound) => {
                missing = true;
                let mut entry = directory.free_entry(0)?;
                entry.data = [0; 32];
                entry.data[..11].copy_from_slice(&name);
                entry.data[11] = 0x20;
                for p in [16, 18, 24] {
                    entry.data[p..p + 2].copy_from_slice(&0x21u16.to_le_bytes());
                }
                entry
            }
            Err(e) => return Err(e),
        };
        if entry.data[11] & 0x10 != 0 {
            return Err(Error::NotFile);
        }
        if entry.data[11] & 1 != 0 {
            return Err(Error::ReadOnly);
        }
        let (old, old_count) = self.bounded_chain::<MAX_CLUSTERS>(&entry, MAX_FILE)?;
        let old_size = entry.size();
        let size = resize.unwrap_or_else(|| {
            if input.is_empty() {
                old_size
            } else {
                old_size.max(offset + input.len())
            }
        });
        if !missing && input.is_empty() && size == old_size {
            return Ok(());
        }
        let cluster_bytes = self.sectors_per_cluster as usize * 512;
        let count = (size + cluster_bytes - 1) / cluster_bytes;
        let mut chain = [0u16; MAX_CLUSTERS];
        let mut found = 0;
        for c in 2..=(self.clusters + 1).min(0xffef) {
            if found == count {
                break;
            }
            if self.fat(c as u16)? == 0 {
                chain[found] = c as u16;
                found += 1;
            }
        }
        if found != count {
            return Err(Error::Full);
        }
        // Nothing in the old allocation is modified, including its last sector.
        for i in 0..count {
            for sector in 0..self.sectors_per_cluster as usize {
                let pos = i * cluster_bytes + sector * 512;
                let mut data = [0; 512];
                if pos < old_size.min(size) {
                    self.read_sector(self.cluster_sector(old[i]) + sector as u32, &mut data)?;
                    let keep = (old_size.min(size) - pos).min(512);
                    data[keep..].fill(0);
                }
                let begin = pos.max(offset);
                let end = (pos + 512).min(offset + input.len()).min(size);
                if begin < end {
                    data[begin - pos..end - pos]
                        .copy_from_slice(&input[begin - offset..end - offset]);
                }
                self.write_sector(self.cluster_sector(chain[i]) + sector as u32, &data)?;
            }
            self.set_fat(chain[i], if i + 1 == count { 0xffff } else { chain[i + 1] })?;
        }
        self.flush()?;
        entry.data[26..28]
            .copy_from_slice(&(if count == 0 { 0u16 } else { chain[0] }).to_le_bytes());
        entry.data[28..32].copy_from_slice(&(size as u32).to_le_bytes());
        entry.data[11] |= 0x20;
        directory.store_entry(&entry)?;
        self.flush()?;
        for c in &old[..old_count] {
            self.set_fat(*c, 0)?;
        }
        self.flush()
    }
}
