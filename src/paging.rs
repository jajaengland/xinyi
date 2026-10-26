// Supervisor identity map plus a private 4 KiB user window per address space.

use core::arch::{asm, x86::__cpuid};

pub const PAGE_SIZE: u32 = 4 * 1024 * 1024;
pub const MAPPED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const USER_BASE: u32 = 0x4000_0000;
pub const USER_WINDOW_BYTES: u32 = PAGE_SIZE;
pub const USER_STACK_TOP: u32 = USER_BASE + USER_WINDOW_BYTES;
pub const USER_STACK_GUARD: u32 = USER_STACK_TOP - 5 * 4096;

const PAGE_PRESENT: u32 = 1 << 0;
const PAGE_WRITABLE: u32 = 1 << 1;
const PAGE_USER: u32 = 1 << 2;
const PAGE_LARGE: u32 = 1 << 7;

const CPUID_PSE: u32 = 1 << 3;
const CR0_PG: u32 = 1 << 31;
const CR0_WP: u32 = 1 << 16;
const CR4_PSE: u32 = 1 << 4;

#[repr(C, align(4096))]
struct PageDirectory {
    entries: [u32; 1024],
}

static mut PAGE_DIRECTORY: PageDirectory = PageDirectory { entries: [0; 1024] };

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
        let entry = address | PAGE_PRESENT | PAGE_WRITABLE | PAGE_LARGE;

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
            pg = in(reg) (CR0_PG | CR0_WP),
            tmp = out(reg) _,
            options(nostack),
        );
    }

    Ok(())
}

pub struct AddressSpace {
    directory: crate::loader::Frames,
    _table: crate::loader::Frames,
}
pub fn isolation_available() -> bool {
    let state = status();
    state.enabled && state.cr0 & CR0_WP != 0 && state.cr4 & CR4_PSE != 0
}
impl AddressSpace {
    pub fn new(program: &crate::loader::LoadedProgram) -> Option<Self> {
        if !isolation_available() {
            return None;
        }
        let image_pages = (program.image.top() - program.image.base()) / 4096;
        let stack_pages = (program.stack.top() - program.stack.base()) / 4096;
        if image_pages == 0 || image_pages > 16 || stack_pages != 4 {
            return None;
        }
        let directory = crate::loader::Frames::new(1)?;
        let table = crate::loader::Frames::new(1)?;
        unsafe {
            let entries = directory.base() as *mut u32;
            core::ptr::copy_nonoverlapping(
                core::ptr::addr_of!(PAGE_DIRECTORY) as *const u32,
                entries,
                1024,
            );
            entries
                .add((USER_BASE / PAGE_SIZE) as usize)
                .write(table.base() | PAGE_PRESENT | PAGE_WRITABLE | PAGE_USER);
            let pages = table.base() as *mut u32;
            for index in 0..image_pages {
                pages
                    .add(index as usize)
                    .write((program.image.base() + index * 4096) | PAGE_PRESENT | PAGE_USER);
            }
            for index in 0..stack_pages {
                pages.add((1024 - stack_pages + index) as usize).write(
                    (program.stack.base() + index * 4096)
                        | PAGE_PRESENT
                        | PAGE_WRITABLE
                        | PAGE_USER,
                );
            }
        }
        Some(Self {
            directory,
            _table: table,
        })
    }
    pub fn directory(&self) -> u32 {
        self.directory.base()
    }
}
impl Drop for AddressSpace {
    fn drop(&mut self) {
        assert!(
            read_cr3() != self.directory(),
            "cannot free active page tables"
        );
    }
}
