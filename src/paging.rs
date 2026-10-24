// Identity-mapped paging using 4 MiB pages.
//
// The kernel, stack, IDT/GDT, VGA buffer, multiboot information, and all
// currently available RAM live below 4 GiB. A single page directory with 4 MiB
// pages therefore maps the entire 32-bit physical address space without
// needing a frame allocator. This is deliberately the simplest mapping that
// can be tested now; user-mode address spaces will need 4 KiB page tables and
// per-process directories later.

use core::arch::{asm, x86::__cpuid};

pub const PAGE_SIZE: u32 = 4 * 1024 * 1024;
pub const MAPPED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

const PAGE_PRESENT: u32 = 1 << 0;
const PAGE_WRITABLE: u32 = 1 << 1;
const PAGE_USER: u32 = 1 << 2;
const PAGE_LARGE: u32 = 1 << 7;

const CPUID_PSE: u32 = 1 << 3;
const CR0_PG: u32 = 1 << 31;
const CR4_PSE: u32 = 1 << 4;

#[repr(C, align(4096))]
struct PageDirectory {
    entries: [u32; 1024],
}

static mut PAGE_DIRECTORY: PageDirectory = PageDirectory {
    entries: [0; 1024],
};

pub struct Status {
    pub enabled: bool,
    pub page_directory: u32,
    pub cr0: u32,
    pub cr4: u32,
}

fn read_cr0() -> u32 {
    let value: u32;

    unsafe {
        asm!("mov {0}, cr0", out(reg) value, options(nomem, nostack, preserves_flags));
    }

    value
}

fn read_cr3() -> u32 {
    let value: u32;

    unsafe {
        asm!("mov {0}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }

    value
}

fn read_cr4() -> u32 {
    let value: u32;

    unsafe {
        asm!("mov {0}, cr4", out(reg) value, options(nomem, nostack, preserves_flags));
    }

    value
}

pub fn status() -> Status {
    let cr0 = read_cr0();

    Status {
        enabled: cr0 & CR0_PG != 0,
        page_directory: read_cr3(),
        cr0,
        cr4: read_cr4(),
    }
}

/// Identity-map the full 32-bit physical address space with 4 MiB pages.
///
/// Returns an error without changing CPU state when PSE is unavailable.
pub fn init() -> Result<(), &'static str> {
    if __cpuid(1).edx & CPUID_PSE == 0 {
        return Err("CPU does not support 4 MiB pages (PSE)");
    }

    let directory = core::ptr::addr_of_mut!(PAGE_DIRECTORY) as *mut u32;

    // One page-directory entry covers 4 MiB, so 1024 entries cover 4 GiB.
    for index in 0..1024u32 {
        let address = index * PAGE_SIZE;
        let entry = address | PAGE_PRESENT | PAGE_WRITABLE | PAGE_USER | PAGE_LARGE;

        unsafe {
            directory.add(index as usize).write(entry);
        }
    }

    unsafe {
        asm!(
            "mov cr3, {directory}",
            "mov {tmp}, cr4",
            "or {tmp}, {pse}",
            "mov cr4, {tmp}",
            "mov {tmp}, cr0",
            "or {tmp}, {pg}",
            "mov cr0, {tmp}",
            directory = in(reg) directory,
            pse = in(reg) CR4_PSE,
            pg = in(reg) CR0_PG,
            tmp = out(reg) _,
            options(nostack),
        );
    }

    Ok(())
}
