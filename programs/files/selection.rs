// Bounded, filename-based selection survives paging without selecting a different file.
#[derive(Clone, Copy)]
pub struct Selection {
    pub names: [[u8; 12]; 32],
    pub lengths: [u8; 32],
    pub flags: [u8; 32],
    pub count: usize,
    pub focus: Option<u16>,
    pub anchor: Option<u16>,
}
impl Selection {
    pub const fn new() -> Self {
        Self {
            names: [[0; 12]; 32],
            lengths: [0; 32],
            flags: [0; 32],
            count: 0,
            focus: None,
            anchor: None,
        }
    }
    pub fn clear(&mut self) {
        *self = Self::new();
    }
    pub fn contains(&self, name: &[u8]) -> bool {
        (0..self.count).any(|i| &self.names[i][..self.lengths[i] as usize] == name)
    }
    pub fn add(&mut self, name: &[u8], flags: u8) -> bool {
        if self.contains(name) {
            return true;
        }
        if self.count == 32 || name.len() > 12 {
            return false;
        }
        let i = self.count;
        self.names[i] = [0; 12];
        self.names[i][..name.len()].copy_from_slice(name);
        self.lengths[i] = name.len() as u8;
        self.flags[i] = flags;
        self.count += 1;
        true
    }
    pub fn plain(&mut self, name: &[u8], flags: u8, index: u16) {
        self.clear();
        self.add(name, flags);
        self.focus = Some(index);
        self.anchor = Some(index);
    }
    pub fn toggle(&mut self, name: &[u8], flags: u8, index: u16) -> bool {
        self.focus = Some(index);
        self.anchor = Some(index);
        if let Some(i) =
            (0..self.count).find(|&i| &self.names[i][..self.lengths[i] as usize] == name)
        {
            for j in i + 1..self.count {
                self.names[j - 1] = self.names[j];
                self.lengths[j - 1] = self.lengths[j];
                self.flags[j - 1] = self.flags[j];
            }
            self.count -= 1;
            true
        } else {
            self.add(name, flags)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_toggle_and_identity() {
        let mut s = Selection::new();
        s.plain(b"A.TXT", 0, 0);
        assert!(s.toggle(b"B.TXT", 0, 20));
        assert_eq!(s.count, 2);
        assert!(s.toggle(b"A.TXT", 0, 0));
        assert_eq!(s.count, 1);
        assert!(s.contains(b"B.TXT"));
        s.clear();
        assert_eq!(s.anchor, None);
    }
    #[test]
    fn bound_is_atomic() {
        let mut s = Selection::new();
        for i in 0..32 {
            s.add(&[b'A', i + 32], 0);
        }
        let before = s.names;
        assert!(!s.add(b"EXTRA", 0));
        assert_eq!(s.names, before);
    }
}
