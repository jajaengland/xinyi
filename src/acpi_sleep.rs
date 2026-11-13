// Static _S5 packages only. General AML control methods require an interpreter.
pub fn sleep_types(aml: &[u8], bare_root: bool) -> Option<(u16, u16)> {
    static_sleep_types(aml, bare_root, true)
}

pub fn static_sleep_types(aml: &[u8], bare_root: bool, needs_b: bool) -> Option<(u16, u16)> {
    for i in 0..aml.len() {
        let prefix = if aml.get(i..i + 6) == Some(&[8, 92, 95, 83, 53, 95]) {
            6
        } else if bare_root && aml.get(i..i + 5) == Some(&[8, 95, 83, 53, 95]) {
            5
        } else {
            continue;
        };
        let op = i + prefix;
        if !matches!(aml.get(op), Some(0x12 | 0x13)) {
            continue;
        }
        let p = op + 1;
        let first = *aml.get(p)?;
        let extra = (first >> 6) as usize;
        let mut length = if extra == 0 {
            first as usize & 63
        } else {
            first as usize & 15
        };
        for n in 0..extra {
            length |= (*aml.get(p + 1 + n)? as usize) << (4 + 8 * n);
        }
        let end = p.checked_add(length)?;
        let mut at = p + extra + 1;
        if end > aml.len() || end <= at {
            continue;
        }
        let body = &aml[..end];
        let count = if aml[op] == 0x12 {
            let count = u64::from(aml[at]);
            at += 1;
            count
        } else {
            let Some(count) = integer(body, &mut at) else {
                continue;
            };
            count
        };
        if count < if needs_b { 2 } else { 1 } {
            continue;
        }
        let Some(a) = integer(body, &mut at) else {
            continue;
        };
        let b = if needs_b {
            let Some(b) = integer(body, &mut at) else {
                continue;
            };
            b
        } else {
            0
        };
        if a <= 7 && b <= 7 {
            return Some(((a as u16) << 10, (b as u16) << 10));
        }
    }
    None
}
fn integer(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let op = *bytes.get(*at)?;
    *at += 1;
    let n = match op {
        0 => return Some(0),
        1 => return Some(1),
        0x0a => 1,
        0x0b => 2,
        0x0c => 4,
        0x0e => 8,
        _ => return None,
    };
    let mut value = 0;
    for i in 0..n {
        value |= (*bytes.get(*at + i)? as u64) << (8 * i);
    }
    *at += n;
    Some(value)
}

/// PM1 control ports from FADT. A nonzero extended descriptor must describe
/// a 16-bit System I/O register. Unsupported address spaces fail closed.
pub fn pm1_ports(fadt: &[u8]) -> Option<(u16, u16)> {
    fn port(fadt: &[u8], legacy: usize, extended: usize) -> Option<u16> {
        if let Some(gas) = fadt.get(extended..extended + 12) {
            let address = u64::from_le_bytes(gas[4..12].try_into().ok()?);
            if address != 0 {
                return (gas[0] == 1
                    && gas[1] == 16
                    && gas[2] == 0
                    && matches!(gas[3], 0 | 2)
                    && address <= 65534)
                    .then_some(address as u16);
            }
        }
        if *fadt.get(89)? < 2 {
            return None;
        }
        let address = u32::from_le_bytes(fadt.get(legacy..legacy + 4)?.try_into().ok()?);
        (address <= 65534).then_some(address as u16)
    }
    let a = port(fadt, 64, 172)?;
    let b = port(fadt, 68, 184)?;
    (a != 0 && (b == 0 || a.abs_diff(b) >= 2)).then_some((a, b))
}
