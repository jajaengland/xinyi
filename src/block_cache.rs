// A small read-through cache for verified BIOS sectors; never dirty/write-back.
#[derive(Clone, Copy)]
struct Entry { valid: bool, block: u32, age: u8, data: [u8; 512] }
impl Entry {
    const EMPTY: Self = Self { valid: false, block: 0, age: 0, data: [0; 512] };
}
pub struct Cache { entries: [Entry; 4] }
impl Cache {
    pub const fn new() -> Self { Self { entries: [Entry::EMPTY; 4] } }
    pub fn clear(&mut self) { self.entries = [Entry::EMPTY; 4]; }
    fn touch(&mut self, index: usize) {
        let age = if self.entries[index].valid { self.entries[index].age } else { 4 };
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if i != index && entry.valid && entry.age < age { entry.age += 1; }
        }
        self.entries[index].age = 0;
    }
    pub fn read(&mut self, block: u32, output: &mut [u8]) -> bool {
        if output.len() < 512 { return false; }
        let Some(index) = self.entries.iter().position(|entry| entry.valid && entry.block == block) else { return false; };
        output[..512].copy_from_slice(&self.entries[index].data);
        self.touch(index);
        true
    }
    pub fn store(&mut self, block: u32, input: &[u8]) {
        if input.len() < 512 { return; }
        let index = self.entries.iter().position(|entry| entry.valid && entry.block == block)
            .or_else(|| self.entries.iter().position(|entry| !entry.valid))
            .unwrap_or_else(|| self.entries.iter().enumerate().max_by_key(|(_, entry)| entry.age).unwrap().0);
        self.touch(index);
        self.entries[index].valid = true;
        self.entries[index].block = block;
        self.entries[index].data.copy_from_slice(&input[..512]);
    }
}
