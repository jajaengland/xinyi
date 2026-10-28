// Bounded filesystem launcher catalog; names, not embedded application code.
pub const MAX_APPS: usize = 16;
#[derive(Clone, Copy)]
pub struct Entry {
    pub label: [u8; 8],
    pub label_len: usize,
    pub path: [u8; 18],
    pub path_len: usize,
}
impl Entry {
    const EMPTY: Self = Self {
        label: [0; 8],
        label_len: 0,
        path: [0; 18],
        path_len: 0,
    };
}
pub struct Catalog {
    pub entries: [Entry; MAX_APPS],
    pub len: usize,
    pub truncated: bool,
}
impl Catalog {
    pub const fn new() -> Self {
        Self {
            entries: [Entry::EMPTY; MAX_APPS],
            len: 0,
            truncated: false,
        }
    }
    pub fn add(&mut self, name: &[u8; 11], directory: bool) {
        if directory
            || !(name[8..].eq_ignore_ascii_case(b"BIN")||name[8..].eq_ignore_ascii_case(b"ELF"))
            || !name
                .iter()
                .all(|b| (32..=126).contains(b) && !b"/\\:".contains(b))
        {
            return;
        }
        let length = name[..8]
            .iter()
            .rposition(|&b| b != b' ')
            .map_or(0, |n| n + 1);
        if length == 0 {
            return;
        }
        if let Some(i)=self.entries[..self.len].iter().position(|e|e.label_len==length&&e.label[..length]==name[..length]){if self.entries[i].path[..self.entries[i].path_len].ends_with(b"BIN")||name[8..].eq_ignore_ascii_case(b"ELF"){return;}let n=self.entries[i].path_len;self.entries[i].path[n-3..n].copy_from_slice(b"BIN");return;}
        if self.len == MAX_APPS {
            self.truncated = true;
            return;
        }
        let mut entry = Entry::EMPTY;
        entry.label[..length].copy_from_slice(&name[..length]);
        entry.label_len = length;
        entry.path[..6].copy_from_slice(b"/APPS/");
        entry.path[6..6 + length].copy_from_slice(&name[..length]);
        entry.path[6 + length] = b'.';
        entry.path[7 + length..10 + length].copy_from_slice(&name[8..]);
        entry.path_len = 10 + length;
        self.entries[self.len] = entry;
        self.len += 1;
    }
    pub fn sort(&mut self) {
        self.entries[..self.len].sort_unstable_by(|a, b| a.label.cmp(&b.label));
    }
}
