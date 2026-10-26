// Hardware-independent layout and commit logic shared by installer and host tests.
use crate::disk_target::Target;
use crate::{
    blockdev::BlockDevice,
    fat16::{self, Error, Fat16, FormatPlan, Result},
};
pub const LOAD: u32 = 0x100000;
pub const HEADER_LBA: u32 = 63;
pub const KERNEL_LBA: u32 = 64;
pub const STAGE2_SECTORS: usize = 32;
pub const MAX_IMAGE: usize = (2048 - 64) * 512;
pub const MAX_MEMORY: u32 = 0x1000000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kernel {
    pub entry: u32,
    pub bytes: u32,
    pub memory: u32,
    pub checksum: u32,
}
impl Kernel {
    pub fn validate(self) -> Result<()> {
        if self.bytes == 0
            || self.bytes as usize > MAX_IMAGE
            || self.memory < self.bytes
            || self.memory > MAX_MEMORY
            || self.entry < LOAD
            || self.entry >= LOAD + self.bytes
        {
            return Err(Error::InvalidKernel);
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn header(self) -> [u8; 512] {
        self.header_with_target(Target::legacy(false, false))
    }
    pub fn header_with_target(self, target: Target) -> [u8; 512] {
        let mut header = [0; 512];
        header[..8].copy_from_slice(b"XINYIK1\0");
        for (offset, value) in [
            (8, 1),
            (12, LOAD),
            (16, self.entry),
            (20, self.bytes),
            (24, self.memory),
            (28, self.checksum),
        ] {
            header[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        header[32..52].copy_from_slice(&target.encode());
        header
    }
}
pub fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |hash, byte| {
        (hash ^ *byte as u32).wrapping_mul(0x01000193)
    })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallPlan {
    pub disk_sectors: u32,
    pub preserve_data: bool,
    pub kernel: Kernel,
    pub target: Target,
    original_mbr: u32,
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
pub fn plan(device: &dyn BlockDevice, kernel: Kernel) -> Result<InstallPlan> {
    kernel.validate()?;
    if device.block_size() != 512 {
        return Err(Error::InvalidVolume);
    }
    let mut mbr = [0; 512];
    if !device.read_block(0, &mut mbr) {
        return Err(Error::Io);
    }
    let mut preserve = mbr[510..512] == [0x55, 0xaa];
    let mut ranges = [(0u32, 0u32); 4];
    for (i, offset) in (446..510).step_by(16).enumerate() {
        let start = u32_at(&mbr, offset + 8);
        let count = u32_at(&mbr, offset + 12);
        if mbr[offset + 4] == 0 {
            if start != 0 || count != 0 {
                preserve = false;
            }
            continue;
        }
        let end = start.checked_add(count).unwrap_or(u32::MAX);
        if ![0, 0x80].contains(&mbr[offset])
            || mbr[offset + 4] == 0xee
            || start < 2048
            || count == 0
            || end <= start
            || end > device.block_count()
        {
            preserve = false;
        }
        for &(other_start, other_end) in &ranges[..i] {
            if start < other_end && other_start < end {
                preserve = false;
            }
        }
        ranges[i] = (start, end);
    }
    preserve &= Fat16::new(device).map_or(false, |fs| fs.start_sector() >= 2048);
    if !preserve {
        fat16::require_full_disk(device)?;
        FormatPlan::new(device.block_count())?;
    }
    Ok(InstallPlan {
        disk_sectors: device.block_count(),
        preserve_data: preserve,
        kernel,
        target: Target::legacy(false, false),
        original_mbr: checksum(&mbr),
    })
}

pub fn install(
    device: &dyn BlockDevice,
    expected: InstallPlan,
    image: &[u8],
    mbr_code: &[u8],
    stage2: &[u8],
) -> Result<()> {
    if image.len() != expected.kernel.bytes as usize
        || checksum(image) != expected.kernel.checksum
        || mbr_code.len() != 512
        || mbr_code[510..512] != [0x55, 0xaa]
        || stage2.len() != STAGE2_SECTORS * 512
    {
        return Err(Error::InvalidKernel);
    }
    let mut actual = plan(device, expected.kernel)?;
    actual.target = expected.target;
    if !expected.target.valid() || actual != expected {
        return Err(Error::DiskChanged);
    }
    if !expected.preserve_data {
        fat16::format_disk(device, FormatPlan::new(expected.disk_sectors)?)?;
    }
    let write = |lba: u32, bytes: &[u8; 512]| -> Result<()> {
        if !device.write_block(lba, bytes) {
            return Err(Error::Io);
        }
        let mut back = [0; 512];
        if !device.read_block(lba, &mut back) || back != *bytes {
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
    let mut mbr = [0; 512];
    if !device.read_block(0, &mut mbr) {
        return Err(Error::Io);
    }
    mbr[510..512].fill(0);
    write(0, &mbr)?;
    flush()?; // Never boot a partially replaced payload.
    write(HEADER_LBA, &[0; 512])?;
    flush()?;
    for (index, chunk) in stage2.chunks_exact(512).enumerate() {
        let mut sector = [0; 512];
        sector.copy_from_slice(chunk);
        write(1 + index as u32, &sector)?;
    }
    for (index, chunk) in image.chunks(512).enumerate() {
        let mut sector = [0; 512];
        sector[..chunk.len()].copy_from_slice(chunk);
        write(KERNEL_LBA + index as u32, &sector)?;
    }
    write(
        HEADER_LBA,
        &expected.kernel.header_with_target(expected.target),
    )?;
    flush()?;
    mbr[..440].copy_from_slice(&mbr_code[..440]); // Keep disk ID and partitions.
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
    write(0, &mbr)?;
    flush()
}
