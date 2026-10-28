//! Read-only allocation audit and deliberately narrow, resumable orphan cleanup.
//! Caller serializes the device and supplies 384 KiB of scratch, never the stack.
use super::{u16_at, u32_at, BlockDevice, Error, Fat16, Result};
const ENTRIES: usize = 65536;
pub const WORKSPACE_WORDS: usize = ENTRIES * 3;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub volume_start: u32,
    pub cluster_bytes: u32,
    pub files: u32,
    pub directories: u32,
    pub referenced: u32,
    pub free: u32,
    pub orphans: u32,
    pub bad: u32,
    pub reserved: u32,
    pub mirrors: u32,
    pub unsafe_mirrors: u32,
    pub headers: u32,
    pub invalid_links: u32,
    pub broken: u32,
    pub loops: u32,
    pub cross_links: u32,
    pub sizes: u32,
    pub directory_errors: u32,
    pub limits: u32,
    pub fingerprint: u64,
}
impl Report {
    pub fn repairable(&self) -> bool {
        self.headers
            + self.invalid_links
            + self.broken
            + self.loops
            + self.cross_links
            + self.sizes
            + self.directory_errors
            + self.limits
            + self.unsafe_mirrors
            == 0
    }
    pub fn clean(&self) -> bool {
        self.repairable() && self.orphans == 0 && self.mirrors == 0
    }
}
fn hash(state: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *state = (*state ^ *b as u64).wrapping_mul(0x100000001b3);
    }
}
struct Audit<'a, 'd> {
    fs: Fat16<'d>,
    fat: &'a [u16],
    owners: &'a mut [u16],
    report: Report,
    budget: u32,
}
impl Audit<'_, '_> {
    fn valid(&self, cluster: u16) -> bool {
        cluster >= 2 && cluster as u32 <= (self.fs.clusters + 1).min(0xffef)
    }
    fn allocated(&self, value: u16) -> bool {
        self.valid(value) || value >= 0xfff8
    }
    fn read(&mut self, lba: u32, out: &mut [u8; 512]) -> Result<()> {
        self.fs.read_sector(lba, out)?;
        hash(&mut self.report.fingerprint, &lba.to_le_bytes());
        hash(&mut self.report.fingerprint, out);
        Ok(())
    }
    // Entire chains are claimed even when a directory end marker occurs early.
    fn claim(&mut self, start: u16) -> Option<u32> {
        let mut cluster = start;
        let mut count = 0;
        loop {
            if !self.valid(cluster) {
                self.report.broken += 1;
                return None;
            }
            let owner = self.owners[cluster as usize];
            if owner != 0 {
                if count != 0 && owner == start {
                    self.report.loops += 1;
                } else {
                    self.report.cross_links += 1;
                }
                return None;
            }
            self.owners[cluster as usize] = start;
            self.report.referenced += 1;
            count += 1;
            let next = self.fat[cluster as usize];
            if next >= 0xfff8 {
                return Some(count);
            }
            cluster = next;
        }
    }
    fn directory(&mut self, start: u16, parent: u16, depth: u32, length: u32) -> Result<()> {
        if depth > 8 || (start != 0 && length > 32) {
            self.report.limits += 1;
            return Ok(());
        }
        let mut cluster = start;
        let sectors = if start == 0 {
            (self.fs.root_entries * 32 + 511) / 512
        } else {
            length * self.fs.sectors_per_cluster
        };
        let mut index = 0u32;
        let mut dots = 0;
        let mut ended = false;
        for sector in 0..sectors {
            let lba = if start == 0 {
                self.fs.root_start + sector
            } else {
                self.fs.data_start
                    + (cluster as u32 - 2) * self.fs.sectors_per_cluster
                    + sector % self.fs.sectors_per_cluster
            };
            let mut data = [0; 512];
            self.read(lba, &mut data)?;
            for entry in data.chunks_exact(32) {
                if start == 0 && index >= self.fs.root_entries {
                    break;
                }
                let position = index;
                index += 1;
                if self.budget == 0 {
                    self.report.limits += 1;
                    return Ok(());
                }
                self.budget -= 1;
                if entry[0] == 0 {
                    ended = true;
                    break;
                }
                if entry[0] == 0xe5 || entry[11] == 0x0f {
                    continue;
                }
                let attr = entry[11];
                let first = u16_at(entry, 26);
                let size = u32_at(entry, 28);
                if attr & 0xc0 != 0 || u16_at(entry, 20) != 0 {
                    self.report.directory_errors += 1;
                }
                if &entry[..11] == b".          " || &entry[..11] == b"..         " {
                    let dot = &entry[..11] == b".          ";
                    if start == 0
                        || position != if dot { 0 } else { 1 }
                        || attr & 0x18 != 0x10
                        || size != 0
                        || first != if dot { start } else { parent }
                    {
                        self.report.directory_errors += 1;
                    } else {
                        dots |= if dot { 1 } else { 2 };
                    }
                    continue;
                }
                if attr & 8 != 0 {
                    if attr & 0x10 != 0 || first != 0 || size != 0 || start != 0 {
                        self.report.directory_errors += 1;
                    }
                    continue;
                }
                if attr & 0x10 != 0 {
                    self.report.directories += 1;
                    if size != 0 {
                        self.report.directory_errors += 1;
                    }
                    if let Some(len) = self.claim(first) {
                        self.directory(first, start, depth + 1, len)?;
                    }
                } else {
                    self.report.files += 1;
                    if first == 0 {
                        if size != 0 {
                            self.report.sizes += 1;
                        }
                    } else if let Some(len) = self.claim(first) {
                        let expected = (size as u64 + self.report.cluster_bytes as u64 - 1)
                            / self.report.cluster_bytes as u64;
                        if expected != len as u64 {
                            self.report.sizes += 1;
                        }
                    }
                }
            }
            if ended {
                break;
            }
            if start != 0 && (sector + 1) % self.fs.sectors_per_cluster == 0 && sector + 1 < sectors
            {
                cluster = self.fat[cluster as usize];
            }
        }
        if start != 0 && dots != 3 {
            self.report.directory_errors += 1;
        }
        Ok(())
    }
}
pub fn scan(device: &dyn BlockDevice, workspace: &mut [u16]) -> Result<Report> {
    if workspace.len() < WORKSPACE_WORDS {
        return Err(Error::OutOfMemory);
    }
    let fs = Fat16::open(device, false)?;
    if fs.sectors_per_fat > 256 {
        return Err(Error::CheckLimit);
    }
    let (fat, rest) = workspace.split_at_mut(ENTRIES);
    let (mirror, owners) = rest.split_at_mut(ENTRIES);
    owners[..ENTRIES].fill(0);
    let mut report = Report {
        volume_start: fs.start,
        cluster_bytes: fs.sectors_per_cluster * 512,
        directories: 1,
        fingerprint: 0xcbf29ce484222325,
        ..Report::default()
    };
    hash(&mut report.fingerprint, &device.block_count().to_le_bytes());
    let mut data = [0; 512];
    fs.read_sector(0, &mut data)?;
    hash(&mut report.fingerprint, &data);
    fs.read_sector(fs.start, &mut data)?;
    hash(&mut report.fingerprint, &data);
    let media = data[21];
    for copy in 0..fs.fats {
        for sector in 0..fs.sectors_per_fat {
            fs.read_sector(fs.fat_start + copy * fs.sectors_per_fat + sector, &mut data)?;
            hash(&mut report.fingerprint, &data);
            let dest = if copy == 0 { &mut *fat } else { &mut *mirror };
            for i in 0..256 {
                dest[sector as usize * 256 + i] = u16_at(&data, i * 2);
            }
        }
    }
    let entries = fs.sectors_per_fat as usize * 256;
    if fs.fats == 1 {
        mirror[..entries].copy_from_slice(&fat[..entries]);
    }
    for table in [&*fat, &*mirror] {
        if table[0] != 0xff00 | media as u16 || table[1] < 0xfff8 {
            report.headers += 1;
        }
    }
    let mut audit = Audit {
        fs,
        fat,
        owners,
        report,
        budget: 1_048_576,
    };
    audit.directory(0, 0, 0, 0)?;
    for i in 0..entries {
        let a = fat[i];
        let b = mirror[i];
        let valid = i >= 2 && i as u32 <= (audit.fs.clusters + 1).min(0xffef);
        let owned = valid && audit.owners[i] != 0;
        if a != b {
            audit.report.mirrors += 1;
            // Only an interrupted allocation/free of an UNREFERENCED entry is unambiguous.
            if !valid
                || owned
                || !((a == 0 && audit.allocated(b)) || (b == 0 && audit.allocated(a)))
            {
                audit.report.unsafe_mirrors += 1;
            }
        }
        if !valid {
            continue;
        }
        for v in [a, b] {
            if v != 0 && v != 0xfff7 && !(0xfff0..=0xfff6).contains(&v) && !audit.allocated(v) {
                audit.report.invalid_links += 1;
            }
        }
        if a == 0xfff7 || b == 0xfff7 {
            audit.report.bad += 1;
        } else if (0xfff0..=0xfff6).contains(&a) || (0xfff0..=0xfff6).contains(&b) {
            audit.report.reserved += 1;
        } else if !owned && (audit.allocated(a) || audit.allocated(b)) {
            audit.report.orphans += 1;
        } else if a == 0 && b == 0 {
            audit.report.free += 1;
        }
    }
    Ok(audit.report)
}
/// Revalidate the complete preview before the first write. No live pointers,
/// directory entries, data, bad markers or boot sectors are ever changed.
pub fn repair_orphans(
    device: &dyn BlockDevice,
    workspace: &mut [u16],
    preview: Report,
) -> Result<Report> {
    let current = scan(device, workspace)?;
    if current != preview {
        return Err(Error::DiskChanged);
    }
    if !current.repairable() {
        return Err(Error::Corrupt);
    }
    if current.orphans == 0 {
        return Ok(current);
    }
    let fs = Fat16::open(device, false)?;
    let (fat, rest) = workspace.split_at_mut(ENTRIES);
    let (mirror, owners) = rest.split_at_mut(ENTRIES);
    let mut desired = [0; 512];
    let mut readback = [0; 512];
    for sector in 0..fs.sectors_per_fat as usize {
        let base = sector * 256;
        for j in 0..256 {
            let i = base + j;
            let orphan = i >= 2
                && i as u32 <= (fs.clusters + 1).min(0xffef)
                && owners[i] == 0
                && (fat[i] != 0 || mirror[i] != 0)
                && fat[i] != 0xfff7
                && mirror[i] != 0xfff7
                && !(0xfff0..=0xfff6).contains(&fat[i])
                && !(0xfff0..=0xfff6).contains(&mirror[i]);
            let value = if orphan { 0 } else { fat[i] };
            desired[j * 2..j * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        for copy in 0..fs.fats {
            let source = if copy == 0 { &*fat } else { &*mirror };
            if (0..256).all(|j| source[base + j] == u16_at(&desired, j * 2)) {
                continue;
            }
            let lba = fs.fat_start + copy * fs.sectors_per_fat + sector as u32;
            fs.write_sector(lba, &desired)?;
            fs.read_sector(lba, &mut readback)?;
            if desired != readback {
                return Err(Error::Io);
            }
        }
    }
    fs.flush()?;
    let after = scan(device, workspace)?;
    if !after.clean() {
        return Err(Error::Corrupt);
    }
    Ok(after)
}
