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

#[repr(C)]
pub struct Transfer {
    pub source: [u8; 105],
    pub destination: [u8; 105],
    pub source_length: u16,
    pub destination_length: u16,
    pub operation: u16,
    pub volume: u32,
}
const _: () = assert!(core::mem::size_of::<Transfer>() == 220);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FileClipboard {
    pub paths: [[u8; 105]; 32],
    pub lengths: [u16; 32],
    pub flags: [u8; 32],
    pub count: u16,
    pub cut: u16,
    pub volume: u32,
    pub version: u32,
}
impl FileClipboard {
    pub const fn new() -> Self {
        Self {
            paths: [[0; 105]; 32],
            lengths: [0; 32],
            flags: [0; 32],
            count: 0,
            cut: 0,
            volume: 0,
            version: 0,
        }
    }
}
const _: () = assert!(core::mem::size_of::<FileClipboard>() == 3468);
