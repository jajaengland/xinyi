// Shared bounded directory-listing ABI; unused slots and padding are initialized.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub name: [u8; 12],
    pub length: u32,
    pub size: u32,
    pub flags: u32,
}
impl Entry {
    pub const EMPTY: Self = Self {
        name: [0; 12],
        length: 0,
        size: 0,
        flags: 0,
    };
}
#[repr(C)]
pub struct Listing {
    pub entries: [Entry; 32],
    pub count: u32,
    pub total: u32,
}
impl Listing {
    pub const fn new() -> Self {
        Self {
            entries: [Entry::EMPTY; 32],
            count: 0,
            total: 0,
        }
    }
}
const _: () = assert!(core::mem::size_of::<Listing>() == 776);
