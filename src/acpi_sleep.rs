// Static _S5 packages only. General AML control methods require an interpreter.
pub fn sleep_types(aml: &[u8], bare_root: bool) -> Option<(u16, u16)> {
    for i in 0..aml.len() {
        let prefix = if aml.get(i..i + 6) == Some(&[8, 92, 95, 83, 53, 95]) {
            6
        } else if bare_root && aml.get(i..i + 5) == Some(&[8, 95, 83, 53, 95]) {
            5
        } else {
            continue;
        };
        let op = i + prefix;
        if aml.get(op) != Some(&0x12) {
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
        if end > aml.len() || end < at + 1 || aml[at] < 2 {
            continue;
        }
        at += 1;
        let a = integer(&aml[..end], &mut at)?;
        let b = integer(&aml[..end], &mut at)?;
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
