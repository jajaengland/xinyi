// IDENTIFY capacity is separate from the addresses supported by this driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacity {
    pub accessible: u32,
    pub physical: Option<u64>,
    pub lba48: bool,
}
pub fn decode(identify: &[u16; 256]) -> Option<Capacity> {
    let legacy = identify[60] as u32 | (identify[61] as u32) << 16;
    if legacy == 0 || legacy > 1 << 28 {
        return None;
    }
    let supports_extended = identify[83] & 0xc400 == 0x4400;
    let physical = if supports_extended {
        let extended = identify[100] as u64
            | (identify[101] as u64) << 16
            | (identify[102] as u64) << 32
            | (identify[103] as u64) << 48;
        if extended >= legacy as u64 && extended < 1u64 << 48 {
            Some(extended)
        } else {
            None
        }
    } else if legacy < 0x0fffffff {
        Some(legacy as u64)
    } else {
        // Saturated legacy fields cannot establish the physical end of disk.
        None
    };
    let lba48 = supports_extended && physical.is_some();
    Some(Capacity {
        // BlockDevice and MBR geometry currently use a u32 sector count.
        // Retain the full u64 capacity so whole-disk operations still refuse
        // devices beyond this interface's range rather than truncating silently.
        accessible: if lba48 {
            physical.unwrap().min(u32::MAX as u64) as u32
        } else {
            legacy
        },
        physical,
        lba48,
    })
}
