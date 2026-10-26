// Stable, validated ATA target record stored in an installed kernel header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub bdf: u32,
    pub id: u32,
    pub position: u8,
}
impl Target {
    pub const fn legacy(secondary: bool, slave: bool) -> Self {
        Self {
            bdf: u32::MAX,
            id: 0,
            position: secondary as u8 | (slave as u8) << 1,
        }
    }
    pub fn valid(self) -> bool {
        self.position < 4
            && if self.bdf == u32::MAX {
                self.id == 0
            } else {
                self.bdf & 0xff00e0f8 == 0 && self.id as u16 != 0xffff && self.id as u16 != 0
            }
    }
    pub fn secondary(self) -> bool {
        self.position & 1 != 0
    }
    pub fn drive(self) -> u8 {
        self.position >> 1
    }
    pub fn encode(self) -> [u8; 20] {
        let mut bytes = [0; 20];
        bytes[..4].copy_from_slice(b"XDT1");
        bytes[4..8].copy_from_slice(&self.bdf.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.id.to_le_bytes());
        bytes[12..16].copy_from_slice(&(self.position as u32).to_le_bytes());
        let sum = hash(&bytes[..16]);
        bytes[16..20].copy_from_slice(&sum.to_le_bytes());
        bytes
    }
    pub fn decode(bytes: &[u8; 20]) -> Option<Self> {
        let word = |i| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        if &bytes[..4] != b"XDT1" || word(16) != hash(&bytes[..16]) || word(12) > 3 {
            return None;
        }
        let target = Self {
            bdf: word(4),
            id: word(8),
            position: word(12) as u8,
        };
        if target.valid() {
            Some(target)
        } else {
            None
        }
    }
}
fn hash(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |h, b| {
        (h ^ *b as u32).wrapping_mul(0x01000193)
    })
}
pub fn parse_bdf(text: &str) -> Option<u32> {
    let (bus, rest) = text.split_once(':')?;
    let (slot, function) = rest.split_once('.')?;
    if bus.is_empty()
        || bus.len() > 2
        || slot.is_empty()
        || slot.len() > 2
        || function.len() != 1
        || ![bus, slot, function]
            .iter()
            .all(|s| s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return None;
    }
    let bus = u8::from_str_radix(bus, 16).ok()?;
    let slot = u8::from_str_radix(slot, 16).ok()?;
    let function = u8::from_str_radix(function, 16).ok()?;
    if slot >= 32 || function >= 8 {
        return None;
    }
    Some((bus as u32) << 16 | (slot as u32) << 8 | function as u32)
}
