// Multiboot information parsing.
//
// The boot loader passes a magic value in EAX and a pointer to a Multiboot
// information structure in EBX. Only the memory fields needed for early
// diagnostics are read here. Paging is disabled, so these physical addresses
// are directly readable.

use core::sync::atomic::{AtomicU32, Ordering};

const BOOTLOADER_MAGIC: u32 = 0x2BADB002;
const INFO_MEMORY: u32 = 1 << 0;
const INFO_MEMORY_MAP: u32 = 1 << 6;

const INFO_FLAGS_OFFSET: u32 = 0;
const INFO_MEM_LOWER_OFFSET: u32 = 4;
const INFO_MEM_UPPER_OFFSET: u32 = 8;
const INFO_MMAP_LENGTH_OFFSET: u32 = 44;
const INFO_MMAP_ADDR_OFFSET: u32 = 48;

// Bounds used to reject obviously corrupt bootloader data.
const MMAP_ENTRY_MIN_SIZE: usize = 20;
const MMAP_MAX_BYTES: usize = 64 * 1024;

static INFO_ADDR: AtomicU32 = AtomicU32::new(0);
static INFO_FLAGS: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy)]
pub struct Region {
    pub base: u64,
    pub length: u64,
    pub kind: u32,
}

unsafe fn read_u32(base: u32, offset: u32) -> u32 {
    core::ptr::read_unaligned((base as *const u8).add(offset as usize).cast::<u32>())
}

unsafe fn read_u32_at(base: *const u8, offset: usize) -> u32 {
    core::ptr::read_unaligned(base.add(offset).cast::<u32>())
}

unsafe fn read_u64_at(base: *const u8, offset: usize) -> u64 {
    core::ptr::read_unaligned(base.add(offset).cast::<u64>())
}

pub fn init(magic: u32, info_addr: u32) {
    if magic != BOOTLOADER_MAGIC || info_addr == 0 {
        return;
    }

    let flags = unsafe { read_u32(info_addr, INFO_FLAGS_OFFSET) };

    INFO_FLAGS.store(flags, Ordering::Relaxed);
    INFO_ADDR.store(info_addr, Ordering::Relaxed);
}

/// Return the conventional lower/upper memory sizes in KiB.
pub fn memory_summary() -> Option<(u32, u32)> {
    let info_addr = INFO_ADDR.load(Ordering::Relaxed);
    let flags = INFO_FLAGS.load(Ordering::Relaxed);

    if info_addr == 0 || flags & INFO_MEMORY == 0 {
        return None;
    }

    let lower = unsafe { read_u32(info_addr, INFO_MEM_LOWER_OFFSET) };
    let upper = unsafe { read_u32(info_addr, INFO_MEM_UPPER_OFFSET) };

    Some((lower, upper))
}

/// Return the physical address of the multiboot information structure, if valid.
pub fn info_address() -> Option<u32> {
    let address = INFO_ADDR.load(Ordering::Relaxed);

    if address == 0 {
        None
    } else {
        Some(address)
    }
}

pub fn disk_target() -> Option<crate::disk_target::Target> {
    // Our BIOS loader places the validated header extension after the standard info.
    if INFO_ADDR.load(Ordering::Relaxed) != 0x4000 {
        return None;
    }
    let mut record = [0; 20];
    unsafe {
        core::ptr::copy_nonoverlapping(0x4080 as *const u8, record.as_mut_ptr(), 20);
    }
    crate::disk_target::Target::decode(&record)
}

/// Return the physical address and byte length of the multiboot memory map.
pub fn mmap_range() -> Option<(u32, u32)> {
    let info_addr = INFO_ADDR.load(Ordering::Relaxed);
    let flags = INFO_FLAGS.load(Ordering::Relaxed);

    if info_addr == 0 || flags & INFO_MEMORY_MAP == 0 {
        return None;
    }

    let length = unsafe { read_u32(info_addr, INFO_MMAP_LENGTH_OFFSET) };
    let address = unsafe { read_u32(info_addr, INFO_MMAP_ADDR_OFFSET) };

    if address == 0 || length == 0 || length as usize > MMAP_MAX_BYTES {
        return None;
    }

    Some((address, length))
}

/// Visit each valid Multiboot memory-map region, if a map was provided.
pub fn for_each_region(mut visit: impl FnMut(Region)) {
    let info_addr = INFO_ADDR.load(Ordering::Relaxed);
    let flags = INFO_FLAGS.load(Ordering::Relaxed);

    if info_addr == 0 || flags & INFO_MEMORY_MAP == 0 {
        return;
    }

    let length = unsafe { read_u32(info_addr, INFO_MMAP_LENGTH_OFFSET) } as usize;
    let address = unsafe { read_u32(info_addr, INFO_MMAP_ADDR_OFFSET) };

    if address == 0 || length < MMAP_ENTRY_MIN_SIZE || length > MMAP_MAX_BYTES {
        return;
    }

    let base = address as *const u8;
    let mut offset = 0usize;

    while offset + MMAP_ENTRY_MIN_SIZE <= length {
        let size = unsafe { read_u32_at(base, offset) } as usize;

        // The size field excludes itself, so each entry occupies size + 4
        // bytes. Reject corrupt or truncated entries.
        if size < MMAP_ENTRY_MIN_SIZE || size > length - offset - core::mem::size_of::<u32>() {
            break;
        }

        visit(Region {
            base: unsafe { read_u64_at(base, offset + 4) },
            length: unsafe { read_u64_at(base, offset + 12) },
            kind: unsafe { read_u32_at(base, offset + 20) },
        });

        offset += size + core::mem::size_of::<u32>();
    }
}

pub fn region_kind(kind: u32) -> &'static str {
    match kind {
        1 => "available",
        2 => "reserved",
        3 => "ACPI reclaimable",
        4 => "ACPI NVS",
        5 => "unusable",
        _ => "unknown",
    }
}
