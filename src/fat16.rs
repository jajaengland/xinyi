// Bounded FAT16 root-directory storage. No heap, directories, or long names.
use crate::blockdev::BlockDevice;
pub const MAX_FILE_SIZE: usize = 1024;
pub const MAX_PROGRAM_SIZE: usize = 64 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Io,
    InvalidVolume,
    InvalidName,
    NotFound,
    Exists,
    Full,
    TooLarge,
    ProgramTooLarge,
    OutOfMemory,
    Corrupt,
    ReadOnly,
    NotFile,
    NotDirectory,
    DirectoryNotEmpty,
    DirectoryBusy,
    DirectoryTooLarge,
    PathTooDeep,
    Mounted,
    ConfirmationRequired,
    DiskTooSmall,
    InvalidKernel,
    InstallConfirmationRequired,
    DiskChanged,
    LimitedCapacity,
}
impl Error {
    pub fn message(self) -> &'static str {
        match self {
            Self::Io => "disk I/O failed (or timed out)",
            Self::InvalidVolume => "no supported FAT16 volume",
            Self::InvalidName => "use an ASCII 8.3 filename (for example NOTE.TXT)",
            Self::NotFound => "file not found",
            Self::Exists => "file already exists",
            Self::Full => "filesystem is full",
            Self::TooLarge => "file exceeds 1024 bytes",
            Self::ProgramTooLarge => "program exceeds 64 KiB",
            Self::OutOfMemory => "not enough memory",
            Self::Corrupt => "invalid FAT chain or inconsistent metadata",
            Self::ReadOnly => "file is read-only",
            Self::NotFile => "path is a directory (expected a regular file)",
            Self::NotDirectory => "path is not a directory",
            Self::DirectoryNotEmpty => "directory is not empty",
            Self::DirectoryBusy => "cannot remove the root, current directory or an ancestor",
            Self::DirectoryTooLarge => "directory exceeds 32 clusters",
            Self::PathTooDeep => "path exceeds 8 components",
            Self::Mounted => "unmount the disk before changing its layout",
            Self::ConfirmationRequired => {
                "run format first, then format confirm with no intervening command"
            }
            Self::DiskTooSmall => "disk is too small for the supported FAT16 layout",
            Self::InvalidKernel => "invalid kernel image or image exceeds the reserved boot area",
            Self::InstallConfirmationRequired => {
                "run install first, then install confirm with no intervening command"
            }
            Self::DiskChanged => "disk layout/capacity changed; preview the installation again",
            Self::LimitedCapacity => {
                "whole-disk format refused: full capacity is unknown or exceeds driver access"
            }
        }
    }
}
pub type Result<T> = core::result::Result<T, Error>;
pub fn require_full_disk(device: &dyn BlockDevice) -> Result<()> {
    if device.physical_block_count() != Some(device.block_count() as u64) {
        Err(Error::LimitedCapacity)
    } else {
        Ok(())
    }
}

// A whole-disk quick format: one aligned FAT16 data partition, no OS loader.
// Keep cluster counts away from FAT type boundaries and reserved FAT16 IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatPlan {
    pub disk_sectors: u32,
    pub volume_sectors: u32,
    pub sectors_per_cluster: u8,
    pub sectors_per_fat: u16,
}
impl FormatPlan {
    pub const START: u32 = 2048;
    pub const MAX_VOLUME_SECTORS: u32 = 4_192_256;
    pub fn new(disk_sectors: u32) -> Result<Self> {
        let total = disk_sectors
            .checked_sub(Self::START)
            .ok_or(Error::DiskTooSmall)?
            .min(Self::MAX_VOLUME_SECTORS);
        if total <= 33 {
            return Err(Error::DiskTooSmall);
        }
        for spc in [1u32, 2, 4, 8, 16, 32, 64] {
            // Microsoft FAT sizing formula; rounds up conservatively.
            let spf = (total - 33 + (256 * spc + 2) - 1) / (256 * spc + 2);
            let overhead = 33 + 2 * spf;
            if total <= overhead {
                continue;
            }
            let clusters = (total - overhead) / spc;
            if (4101..=65508).contains(&clusters) && (clusters + 2) * 2 <= spf * 512 {
                return Ok(Self {
                    disk_sectors,
                    volume_sectors: total,
                    sectors_per_cluster: spc as u8,
                    sectors_per_fat: spf as u16,
                });
            }
        }
        Err(Error::DiskTooSmall)
    }
}

// No writes occur until the caller has obtained explicit shell confirmation.
// On failure the caller must leave the filesystem detached, never report success.
pub fn format_disk(device: &dyn BlockDevice, plan: FormatPlan) -> Result<()> {
    require_full_disk(device)?; // Must precede every media write.
    if device.block_size() != 512 || FormatPlan::new(device.block_count())? != plan {
        return Err(Error::InvalidVolume);
    }
    let write = |lba: u32, data: &[u8; 512]| -> Result<()> {
        if !device.write_block(lba, data) {
            return Err(Error::Io);
        }
        let mut readback = [0; 512];
        if !device.read_block(lba, &mut readback) || readback != *data {
            return Err(Error::Io);
        }
        Ok(())
    };
    let flush = || {
        if device.flush() {
            Ok(())
        } else {
            Err(Error::Io)
        }
    };
    let zero = [0; 512];
    // Invalidate the old partition table before initializing replacement metadata.
    write(0, &zero)?;
    flush()?;
    // Remove primary/backup GPT signatures so readers do not prefer a stale GPT.
    for sector in 1..34 {
        write(sector, &zero)?;
    }
    for sector in plan.disk_sectors - 33..plan.disk_sectors {
        write(sector, &zero)?;
    }
    let mut boot = [0; 512];
    boot[..11].copy_from_slice(b"\xeb\x3c\x90XINYI   ");
    boot[11..13].copy_from_slice(&512u16.to_le_bytes());
    boot[13] = plan.sectors_per_cluster;
    boot[14..16].copy_from_slice(&1u16.to_le_bytes());
    boot[16] = 2;
    boot[17..19].copy_from_slice(&512u16.to_le_bytes());
    if plan.volume_sectors < 65536 {
        boot[19..21].copy_from_slice(&(plan.volume_sectors as u16).to_le_bytes());
    } else {
        boot[32..36].copy_from_slice(&plan.volume_sectors.to_le_bytes());
    }
    boot[21] = 0xf8;
    boot[22..24].copy_from_slice(&plan.sectors_per_fat.to_le_bytes());
    boot[24..26].copy_from_slice(&63u16.to_le_bytes());
    boot[26..28].copy_from_slice(&255u16.to_le_bytes());
    boot[28..32].copy_from_slice(&FormatPlan::START.to_le_bytes());
    boot[36] = 0x80;
    boot[38] = 0x29;
    boot[39..43].copy_from_slice(&0x58494e59u32.to_le_bytes());
    boot[43..54].copy_from_slice(b"XINYI DATA ");
    boot[54..62].copy_from_slice(b"FAT16   ");
    write(FormatPlan::START, &boot)?; // Signature remains invalid during initialization.
    let spf = plan.sectors_per_fat as u32;
    for copy in 0..2 {
        for sector in 0..spf {
            let mut fat = [0; 512];
            if sector == 0 {
                fat[..4].copy_from_slice(&[0xf8, 0xff, 0xff, 0xff]);
            }
            write(FormatPlan::START + 1 + copy * spf + sector, &fat)?;
        }
    }
    for sector in 0..32 {
        write(FormatPlan::START + 1 + 2 * spf + sector, &zero)?;
    }
    flush()?;
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    write(FormatPlan::START, &boot)?;
    flush()?;
    let mut mbr = [0; 512];
    mbr[450] = 0x0e; // FAT16 LBA, deliberately not marked bootable.
    mbr[447..450].copy_from_slice(&[0xfe, 0xff, 0xff]);
    mbr[451..454].copy_from_slice(&[0xfe, 0xff, 0xff]);
    mbr[454..458].copy_from_slice(&FormatPlan::START.to_le_bytes());
    mbr[458..462].copy_from_slice(&plan.volume_sectors.to_le_bytes());
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
    write(0, &mbr)?;
    flush()?;
    // Re-open through the same validation path used by mount before success.
    Fat16::new(device)?;
    Ok(())
}
fn u16_at(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([b[p], b[p + 1]])
}
fn u32_at(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}
pub fn short_name(name: &str) -> Result<[u8; 11]> {
    let mut output = [b' '; 11];
    let mut parts = name.split('.');
    let base = parts.next().ok_or(Error::InvalidName)?;
    let extension = parts.next();
    if parts.next().is_some()
        || base.is_empty()
        || base.len() > 8
        || extension.map_or(false, |s| s.is_empty() || s.len() > 3)
    {
        return Err(Error::InvalidName);
    }
    for (part, start) in [(base, 0), (extension.unwrap_or(""), 8)] {
        for (i, b) in part.bytes().enumerate() {
            if !b.is_ascii_alphanumeric() && !b"!#$%&'()-@^_`{}~".contains(&b) {
                return Err(Error::InvalidName);
            }
            output[start + i] = b.to_ascii_uppercase();
        }
    }
    Ok(output)
}
pub fn display_name(name: &[u8; 11], output: &mut [u8; 12]) -> usize {
    let mut length = 0;
    for b in &name[..8] {
        if *b != b' ' {
            output[length] = *b;
            length += 1;
        }
    }
    if name[8] != b' ' {
        output[length] = b'.';
        length += 1;
        for b in &name[8..] {
            if *b != b' ' {
                output[length] = *b;
                length += 1;
            }
        }
    }
    length
}
#[derive(Clone, Copy)]
struct Entry {
    slot: u32,
    sector: u32,
    data: [u8; 32],
}
impl Entry {
    fn cluster(&self) -> u16 {
        u16_at(&self.data, 26)
    }
    fn size(&self) -> usize {
        u32_at(&self.data, 28) as usize
    }
}
#[derive(Clone, Copy)]
pub struct Fat16<'a> {
    device: &'a dyn BlockDevice,
    start: u32,
    fat_start: u32,
    root_start: u32,
    data_start: u32,
    sectors_per_fat: u32,
    fats: u32,
    root_entries: u32,
    sectors_per_cluster: u32,
    clusters: u32,
    directory: u16,
    directory_chain: [u16; 32],
    directory_len: usize,
    directory_readonly: bool,
}
impl<'a> Fat16<'a> {
    // Accept a whole-volume image or the first primary MBR FAT16 partition.
    pub fn new(device: &'a dyn BlockDevice) -> Result<Self> {
        if device.block_size() != 512 || device.block_count() == 0 {
            return Err(Error::InvalidVolume);
        }
        let mut sector = [0; 512];
        if !device.read_block(0, &mut sector) {
            return Err(Error::Io);
        }
        if let Ok(fs) = Self::at(device, 0, device.block_count(), &sector) {
            return Ok(fs);
        }
        if sector[510..512] != [0x55, 0xaa] {
            return Err(Error::InvalidVolume);
        }
        for p in (446..510).step_by(16) {
            if ![0x04, 0x06, 0x0e].contains(&sector[p + 4]) {
                continue;
            }
            let start = u32_at(&sector, p + 8);
            let count = u32_at(&sector, p + 12);
            if start == 0
                || count == 0
                || start
                    .checked_add(count)
                    .map_or(true, |e| e > device.block_count())
            {
                return Err(Error::InvalidVolume);
            }
            let mut boot = [0; 512];
            if !device.read_block(start, &mut boot) {
                return Err(Error::Io);
            }
            return Self::at(device, start, count, &boot);
        }
        Err(Error::InvalidVolume)
    }
    fn at(device: &'a dyn BlockDevice, start: u32, limit: u32, b: &[u8; 512]) -> Result<Self> {
        let spc = b[13] as u32;
        let reserved = u16_at(b, 14) as u32;
        let fats = b[16] as u32;
        let entries = u16_at(b, 17) as u32;
        let spf = u16_at(b, 22) as u32;
        let total = if u16_at(b, 19) != 0 {
            u16_at(b, 19) as u32
        } else {
            u32_at(b, 32)
        };
        if b[510..512] != [0x55, 0xaa]
            || u16_at(b, 11) != 512
            || reserved == 0
            || !spc.is_power_of_two()
            || spc > 64
            || !(1..=2).contains(&fats)
            || entries == 0
            || spf == 0
            || total > limit
        {
            return Err(Error::InvalidVolume);
        }
        let root_sectors = (entries * 32 + 511) / 512;
        let overhead = reserved + fats * spf + root_sectors;
        let clusters = total.checked_sub(overhead).ok_or(Error::InvalidVolume)? / spc;
        if !(4085..65525).contains(&clusters) || (clusters + 2) * 2 > spf * 512 {
            return Err(Error::InvalidVolume);
        }
        let fs = Self {
            device,
            start,
            fat_start: start + reserved,
            root_start: start + reserved + fats * spf,
            data_start: start + overhead,
            sectors_per_fat: spf,
            fats,
            root_entries: entries,
            sectors_per_cluster: spc,
            clusters,
            directory: 0,
            directory_chain: [0; 32],
            directory_len: 0,
            directory_readonly: false,
        };
        let header = fs.fat(0)?;
        if header != 0xff00 | b[21] as u16 || fs.fat(1)? < 0xfff8 {
            return Err(Error::InvalidVolume);
        }
        Ok(fs)
    }
    pub fn start_sector(&self) -> u32 {
        self.start
    }
    fn read_sector(&self, lba: u32, data: &mut [u8; 512]) -> Result<()> {
        if self.device.read_block(lba, data) {
            Ok(())
        } else {
            Err(Error::Io)
        }
    }
    fn write_sector(&self, lba: u32, data: &[u8; 512]) -> Result<()> {
        if self.device.write_block(lba, data) {
            Ok(())
        } else {
            Err(Error::Io)
        }
    }
    fn flush(&self) -> Result<()> {
        if self.device.flush() {
            Ok(())
        } else {
            Err(Error::Io)
        }
    }
    fn fat(&self, cluster: u16) -> Result<u16> {
        let offset = cluster as u32 * 2;
        let mut sector = [0; 512];
        self.read_sector(self.fat_start + offset / 512, &mut sector)?;
        let value = u16_at(&sector, offset as usize % 512);
        for copy in 1..self.fats {
            self.read_sector(
                self.fat_start + copy * self.sectors_per_fat + offset / 512,
                &mut sector,
            )?;
            if u16_at(&sector, offset as usize % 512) != value {
                return Err(Error::Corrupt);
            }
        }
        Ok(value)
    }
    fn set_fat(&self, cluster: u16, value: u16) -> Result<()> {
        let offset = cluster as u32 * 2;
        for copy in 0..self.fats {
            let lba = self.fat_start + copy * self.sectors_per_fat + offset / 512;
            let mut sector = [0; 512];
            self.read_sector(lba, &mut sector)?;
            sector[offset as usize % 512..offset as usize % 512 + 2]
                .copy_from_slice(&value.to_le_bytes());
            self.write_sector(lba, &sector)?;
        }
        Ok(())
    }
    fn entry_count(&self) -> u32 {
        if self.directory == 0 {
            self.root_entries
        } else {
            self.directory_len as u32 * self.sectors_per_cluster * 16
        }
    }
    fn valid_cluster(&self, cluster: u16) -> bool {
        cluster >= 2 && cluster < 0xfff0 && cluster as u32 <= self.clusters + 1
    }
    fn load_directory(&self, cluster: u16, parent: u16, readonly: bool) -> Result<Self> {
        let mut view = *self;
        view.directory = cluster;
        view.directory_len = 0;
        view.directory_readonly = readonly;
        let mut next = cluster;
        loop {
            if !self.valid_cluster(next)
                || view.directory_chain[..view.directory_len].contains(&next)
            {
                return Err(Error::Corrupt);
            }
            if view.directory_len == 32 {
                return Err(Error::DirectoryTooLarge);
            }
            view.directory_chain[view.directory_len] = next;
            view.directory_len += 1;
            next = self.fat(next)?;
            if next >= 0xfff8 {
                break;
            }
        }
        for (slot, name, expected) in [(0, b".          ", cluster), (1, b"..         ", parent)] {
            let entry = view.entry(slot)?;
            if &entry.data[..11] != name
                || entry.data[11] & 0x18 != 0x10
                || entry.cluster() != expected
                || entry.size() != 0
                || u16_at(&entry.data, 20) != 0
            {
                return Err(Error::Corrupt);
            }
        }
        Ok(view)
    }
    fn walk(&self, path: &crate::fs_path::Path, count: usize) -> Result<Self> {
        let mut directory = *self;
        directory.directory = 0;
        directory.directory_len = 0;
        directory.directory_readonly = false;
        for name in &path.names[..count] {
            let entry = directory.find(name)?;
            if entry.data[11] & 0x10 == 0 {
                return Err(Error::NotDirectory);
            }
            if entry.size() != 0 || u16_at(&entry.data, 20) != 0 {
                return Err(Error::Corrupt);
            }
            directory = directory.load_directory(
                entry.cluster(),
                directory.directory,
                entry.data[11] & 1 != 0,
            )?;
        }
        Ok(directory)
    }
    fn parent(&self, text: &str) -> Result<(Self, [u8; 11])> {
        let path = crate::fs_path::Path::parse(text, &crate::fs_path::Path::ROOT)?;
        if path.depth == 0 {
            return Err(Error::InvalidName);
        }
        Ok((
            self.walk(&path, path.depth - 1)?,
            path.names[path.depth - 1],
        ))
    }
    pub fn is_directory(&self, text: &str) -> Result<()> {
        let path = crate::fs_path::Path::parse(text, &crate::fs_path::Path::ROOT)?;
        self.walk(&path, path.depth).map(|_| ())
    }
    fn free_cluster(&self, exclude: u16) -> Result<u16> {
        for cluster in 2..=(self.clusters + 1).min(0xffef) {
            if cluster as u16 != exclude && self.fat(cluster as u16)? == 0 {
                return Ok(cluster as u16);
            }
        }
        Err(Error::Full)
    }
    fn zero_cluster(&self, cluster: u16) -> Result<()> {
        for sector in 0..self.sectors_per_cluster {
            self.write_sector(self.cluster_sector(cluster) + sector, &[0; 512])?;
        }
        Ok(())
    }
    fn free_entry(&mut self, exclude: u16) -> Result<Entry> {
        if self.directory_readonly {
            return Err(Error::ReadOnly);
        }
        for slot in 0..self.entry_count() {
            let entry = self.entry(slot)?;
            if entry.data[0] == 0 || entry.data[0] == 0xe5 {
                if entry.data[0] == 0 && slot + 1 < self.entry_count() {
                    let mut next = self.entry(slot + 1)?;
                    next.data[0] = 0;
                    self.store_entry(&next)?;
                }
                return Ok(entry);
            }
        }
        if self.directory == 0 {
            return Err(Error::Full);
        }
        if self.directory_len == 32 {
            return Err(Error::DirectoryTooLarge);
        }
        let cluster = self.free_cluster(exclude)?;
        self.zero_cluster(cluster)?;
        self.flush()?;
        self.set_fat(cluster, 0xffff)?;
        self.flush()?;
        self.set_fat(self.directory_chain[self.directory_len - 1], cluster)?;
        self.flush()?;
        let slot = self.entry_count();
        self.directory_chain[self.directory_len] = cluster;
        self.directory_len += 1;
        self.entry(slot)
    }
    pub fn mkdir(&self, text: &str) -> Result<()> {
        let (mut parent, name) = self.parent(text)?;
        match parent.find(&name) {
            Ok(_) => return Err(Error::Exists),
            Err(Error::NotFound) => (),
            Err(error) => return Err(error),
        }
        if parent.directory_readonly {
            return Err(Error::ReadOnly);
        }
        let cluster = self.free_cluster(0)?;
        let mut entry = parent.free_entry(cluster)?;
        self.zero_cluster(cluster)?;
        let mut contents = [0; 512];
        for (offset, name, link) in [
            (0, b".          ", cluster),
            (32, b"..         ", parent.directory),
        ] {
            contents[offset..offset + 11].copy_from_slice(name);
            contents[offset + 11] = 0x10;
            contents[offset + 26..offset + 28].copy_from_slice(&link.to_le_bytes());
            for date in [16, 18, 24] {
                contents[offset + date..offset + date + 2].copy_from_slice(&0x21u16.to_le_bytes());
            }
        }
        self.write_sector(self.cluster_sector(cluster), &contents)?;
        self.flush()?;
        self.set_fat(cluster, 0xffff)?;
        self.flush()?;
        entry.data = [0; 32];
        entry.data[..11].copy_from_slice(&name);
        entry.data[11] = 0x10;
        entry.data[26..28].copy_from_slice(&cluster.to_le_bytes());
        for date in [16, 18, 24] {
            entry.data[date..date + 2].copy_from_slice(&0x21u16.to_le_bytes());
        }
        parent.store_entry(&entry)?;
        self.flush()
    }
    pub fn rmdir(&self, text: &str) -> Result<()> {
        let path = crate::fs_path::Path::parse(text, &crate::fs_path::Path::ROOT)?;
        if path.depth == 0 {
            return Err(Error::DirectoryBusy);
        }
        let (parent, name) = self.parent(text)?;
        let mut entry = parent.find(&name)?;
        if entry.data[11] & 0x10 == 0 {
            return Err(Error::NotDirectory);
        }
        if entry.size() != 0 || u16_at(&entry.data, 20) != 0 {
            return Err(Error::Corrupt);
        }
        if parent.directory_readonly || entry.data[11] & 1 != 0 {
            return Err(Error::ReadOnly);
        }
        let directory = self.load_directory(entry.cluster(), parent.directory, false)?;
        for slot in 2..directory.entry_count() {
            let child = directory.entry(slot)?;
            if child.data[0] == 0 {
                break;
            }
            if child.data[0] != 0xe5 && child.data[11] & 8 == 0 {
                return Err(Error::DirectoryNotEmpty);
            }
        }
        entry.data[0] = 0xe5;
        self.store_entry(&entry)?;
        self.flush()?;
        for cluster in &directory.directory_chain[..directory.directory_len] {
            self.set_fat(*cluster, 0)?;
        }
        self.flush()
    }
    fn entry(&self, slot: u32) -> Result<Entry> {
        let mut sector = [0; 512];
        if slot >= self.entry_count() {
            return Err(Error::Corrupt);
        }
        let lba = if self.directory == 0 {
            self.root_start + slot / 16
        } else {
            let per_cluster = self.sectors_per_cluster * 16;
            self.cluster_sector(self.directory_chain[(slot / per_cluster) as usize])
                + (slot % per_cluster) / 16
        };
        self.read_sector(lba, &mut sector)?;
        let mut data = [0; 32];
        let offset = (slot as usize % 16) * 32;
        data.copy_from_slice(&sector[offset..offset + 32]);
        Ok(Entry {
            slot,
            sector: lba,
            data,
        })
    }
    fn store_entry(&self, entry: &Entry) -> Result<()> {
        let lba = entry.sector;
        let mut sector = [0; 512];
        self.read_sector(lba, &mut sector)?;
        let offset = (entry.slot as usize % 16) * 32;
        sector[offset..offset + 32].copy_from_slice(&entry.data);
        self.write_sector(lba, &sector)
    }
    fn find(&self, name: &[u8; 11]) -> Result<Entry> {
        for slot in 0..self.entry_count() {
            let e = self.entry(slot)?;
            if e.data[0] == 0 {
                break;
            }
            if e.data[0] != 0xe5 && e.data[11] & 0x08 == 0 && e.data[..11] == name[..] {
                return Ok(e);
            }
        }
        Err(Error::NotFound)
    }
    pub fn list_entries(
        &self,
        path: &str,
        mut visit: impl FnMut(&[u8; 11], usize, bool),
    ) -> Result<()> {
        let path = crate::fs_path::Path::parse(path, &crate::fs_path::Path::ROOT)?;
        let directory = self.walk(&path, path.depth)?;
        for slot in 0..directory.entry_count() {
            let e = directory.entry(slot)?;
            if e.data[0] == 0 {
                break;
            }
            if e.data[0] == 0xe5 || e.data[11] & 0x08 != 0 || e.data[0] == b'.' {
                continue;
            }
            let mut name = [0; 11];
            name.copy_from_slice(&e.data[..11]);
            visit(&name, e.size(), e.data[11] & 0x10 != 0);
        }
        Ok(())
    }
    pub fn create(&self, name: &str) -> Result<()> {
        let (mut directory, name) = self.parent(name)?;
        match directory.find(&name) {
            Ok(_) => return Err(Error::Exists),
            Err(Error::NotFound) => (),
            Err(e) => return Err(e),
        }
        let mut entry = directory.free_entry(0)?;
        entry.data = [0; 32];
        entry.data[..11].copy_from_slice(&name);
        entry.data[11] = 0x20;
        // Valid baseline DOS dates: 1980-01-01 (no timestamp support yet).
        for offset in [16, 18, 24] {
            entry.data[offset..offset + 2].copy_from_slice(&0x21u16.to_le_bytes());
        }
        directory.store_entry(&entry)?;
        directory.flush()
    }
    // Validate the complete bounded chain before reading or replacing a file.
    fn chain(&self, entry: &Entry) -> Result<([u16; 2], usize)> {
        self.bounded_chain::<2>(entry, MAX_FILE_SIZE)
    }
    fn bounded_chain<const N: usize>(&self, entry: &Entry, limit: usize) -> Result<([u16; N], usize)> {
        if entry.size() > limit {
            return Err(if limit == MAX_FILE_SIZE { Error::TooLarge } else { Error::ProgramTooLarge });
        }
        let count = (entry.size() + self.sectors_per_cluster as usize * 512 - 1)
            / (self.sectors_per_cluster as usize * 512);
        if count > N { return Err(Error::Corrupt); }
        let mut chain = [0; N];
        let mut next = entry.cluster();
        if count == 0 {
            return if next == 0 {
                Ok((chain, 0))
            } else {
                Err(Error::Corrupt)
            };
        }
        for i in 0..count {
            if next < 2
                || next as u32 > self.clusters + 1
                || next >= 0xfff0
                || chain[..i].contains(&next)
            {
                return Err(Error::Corrupt);
            }
            chain[i] = next;
            next = self.fat(next)?;
        }
        if next < 0xfff8 {
            return Err(Error::Corrupt);
        }
        Ok((chain, count))
    }
    fn cluster_sector(&self, cluster: u16) -> u32 {
        self.data_start + (cluster as u32 - 2) * self.sectors_per_cluster
    }
    pub fn read(&self, name: &str, output: &mut [u8; MAX_FILE_SIZE]) -> Result<usize> {
        self.read_bounded(name, output, MAX_FILE_SIZE)
    }
    pub fn read_program(&self, name: &str, output: &mut [u8]) -> Result<usize> {
        self.read_bounded(name, output, MAX_PROGRAM_SIZE)
    }
    fn read_bounded(&self, name: &str, output: &mut [u8], limit: usize) -> Result<usize> {
        let (directory, name) = self.parent(name)?;
        let entry = directory.find(&name)?;
        if entry.data[11] & 0x10 != 0 {
            return Err(Error::NotFile);
        }
        let (chain, count) = self.bounded_chain::<128>(&entry, limit)?;
        if entry.size() > output.len() { return Err(Error::TooLarge); }
        let mut copied = 0;
        for cluster in &chain[..count] {
            for sector in 0..self.sectors_per_cluster {
                if copied == entry.size() {
                    break;
                }
                let mut data = [0; 512];
                self.read_sector(self.cluster_sector(*cluster) + sector, &mut data)?;
                let bytes = (entry.size() - copied).min(512);
                output[copied..copied + bytes].copy_from_slice(&data[..bytes]);
                copied += bytes;
            }
        }
        Ok(copied)
    }
    pub fn write(&self, name: &str, input: &[u8]) -> Result<()> {
        self.write_bounded(name,input,MAX_FILE_SIZE)
    }
    pub fn write_program(&self,name:&str,input:&[u8])->Result<()> {
        self.write_bounded(name,input,MAX_PROGRAM_SIZE)
    }
    fn write_bounded(&self,name:&str,input:&[u8],limit:usize)->Result<()> {
        if input.len()>limit {return Err(Error::TooLarge);}
        let (directory, name) = self.parent(name)?;
        if directory.directory_readonly {
            return Err(Error::ReadOnly);
        }
        let mut entry = directory.find(&name)?;
        if entry.data[11] & 0x10 != 0 {
            return Err(Error::NotFile);
        }
        if entry.data[11] & 1 != 0 {
            return Err(Error::ReadOnly);
        }
        let (old, old_count) = self.bounded_chain::<128>(&entry,limit)?;
        let count = (input.len() + self.sectors_per_cluster as usize * 512 - 1)
            / (self.sectors_per_cluster as usize * 512);
        let mut chain = [0u16; 128];
        let mut found = 0;
        // Select free clusters without changing metadata; a full disk is a clean error.
        for cluster in 2..=(self.clusters + 1).min(0xffef) {
            if found == count {
                break;
            }
            if self.fat(cluster as u16)? == 0 {
                chain[found] = cluster as u16;
                found += 1;
            }
        }
        if found != count {
            return Err(Error::Full);
        }
        let mut copied = 0;
        for (i, cluster) in chain[..count].iter().enumerate() {
            for sector in 0..self.sectors_per_cluster {
                let mut data = [0; 512];
                let bytes = (input.len() - copied).min(512);
                data[..bytes].copy_from_slice(&input[copied..copied + bytes]);
                copied += bytes;
                self.write_sector(self.cluster_sector(*cluster) + sector, &data)?;
            }
            self.set_fat(*cluster, if i + 1 == count { 0xffff } else { chain[i + 1] })?;
        }
        // Persist the replacement chain before publishing it in the directory.
        self.flush()?;
        entry.data[26..28]
            .copy_from_slice(&(if count == 0 { 0u16 } else { chain[0] }).to_le_bytes());
        entry.data[28..32].copy_from_slice(&(input.len() as u32).to_le_bytes());
        entry.data[11] |= 0x20;
        self.store_entry(&entry)?;
        self.flush()?;
        for cluster in &old[..old_count] {
            self.set_fat(*cluster, 0)?;
        }
        self.flush()
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        let (directory, name) = self.parent(name)?;
        if directory.directory_readonly {
            return Err(Error::ReadOnly);
        }
        let mut entry = directory.find(&name)?;
        if entry.data[11] & 0x10 != 0 {
            return Err(Error::NotFile);
        }
        if entry.data[11] & 1 != 0 {
            return Err(Error::ReadOnly);
        }
        let (chain, count) = self.bounded_chain::<128>(&entry, MAX_PROGRAM_SIZE)?;
        // Persist the tombstone before releasing clusters. A failed operation
        // may leak space, but must not leave a live entry pointing to free data.
        entry.data[0] = 0xe5;
        self.store_entry(&entry)?;
        self.flush()?;
        for cluster in &chain[..count] {
            self.set_fat(*cluster, 0)?;
        }
        self.flush()
    }
}
