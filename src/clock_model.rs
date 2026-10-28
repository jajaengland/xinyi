// Fractional 100 Hz accounting from the ACPI free-running PM timer. The
// counter is sampled at every IRQ and on both sides of each firmware call.
pub const PM_HZ: u64 = 3_579_545;

pub struct Clock {
    last: u32,
    mask: u32,
    fraction: u64,
    ticks: u32,
}
impl Clock {
    pub const fn new(value: u32, mask: u32) -> Self {
        Self { last: value & mask, mask, fraction: 0, ticks: 0 }
    }
    pub fn sample(&mut self, value: u32) -> u32 {
        let value = value & self.mask;
        let elapsed = value.wrapping_sub(self.last) & self.mask;
        self.last = value;
        self.fraction += elapsed as u64 * 100;
        self.ticks = self.ticks.wrapping_add((self.fraction / PM_HZ) as u32);
        self.fraction %= PM_HZ;
        self.ticks
    }
}

pub fn pm_timer(fadt: &[u8]) -> Option<(u16, u32)> {
    if fadt.len() < 116 { return None; }
    let flags = u32::from_le_bytes(fadt[112..116].try_into().ok()?);
    if flags & (1 << 20) != 0 { return None; } // Hardware-reduced ACPI.
    let mut port = u32::from_le_bytes(fadt[76..80].try_into().ok()?) as u64;
    if fadt[91] < 4 { port = 0; }
    if port == 0 && fadt.len() >= 220 {
        // Extended PM timer GAS: System I/O, no offset, DWORD access.
        if fadt[208] != 1 || fadt[209] < 24 || fadt[210] != 0 || !matches!(fadt[211], 0 | 3) {
            return None;
        }
        port = u64::from_le_bytes(fadt[212..220].try_into().ok()?);
    }
    if port == 0 || port > 65532 { return None; }
    Some((port as u16, if flags & (1 << 8) != 0 { u32::MAX } else { 0x00ff_ffff }))
}
