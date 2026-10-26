// Kernel-owned Global Descriptor Table.
//
// The multiboot loader provides a usable flat GDT, but the kernel should not
// depend on descriptors it does not own. Selectors used by the kernel:
//
//   0x08 - ring 0 code, flat 4 GiB, 32-bit
//   0x10 - ring 0 data, flat 4 GiB
//   0x18 - ring 3 code, flat 4 GiB, 32-bit (user)
//   0x20 - ring 3 data, flat 4 GiB (user)
//   0x28 - TSS (Task State Segment)

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtEntry {
    const fn missing() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }

    const fn new(base: u32, limit: u32, access: u8, flags: u8) -> Self {
        Self {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_middle: ((base >> 16) & 0xFF) as u8,
            access,
            granularity: (((limit >> 16) & 0x0F) as u8) | (flags & 0xF0),
            base_high: ((base >> 24) & 0xFF) as u8,
        }
    }
}

#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Tss {
    prev_link: u32,
    esp0: u32,
    ss0: u32,
    esp1: u32,
    ss1: u32,
    esp2: u32,
    ss2: u32,
    cr3: u32,
    eip: u32,
    eflags: u32,
    eax: u32,
    ecx: u32,
    edx: u32,
    ebx: u32,
    esp: u32,
    ebp: u32,
    esi: u32,
    edi: u32,
    es: u32,
    cs: u32,
    ss: u32,
    ds: u32,
    fs: u32,
    gs: u32,
    ldt: u32,
    trap: u16,
    iomap_base: u16,
}

impl Tss {
    const fn missing() -> Self {
        Self {
            prev_link: 0,
            esp0: 0,
            ss0: 0,
            esp1: 0,
            ss1: 0,
            esp2: 0,
            ss2: 0,
            cr3: 0,
            eip: 0,
            eflags: 0,
            eax: 0,
            ecx: 0,
            edx: 0,
            ebx: 0,
            esp: 0,
            ebp: 0,
            esi: 0,
            edi: 0,
            es: 0,
            cs: 0,
            ss: 0,
            ds: 0,
            fs: 0,
            gs: 0,
            ldt: 0,
            trap: 0,
            iomap_base: 0,
        }
    }
}

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_CODE_SELECTOR: u16 = 0x1B; // 0x18 | 3
pub const USER_DATA_SELECTOR: u16 = 0x23; // 0x20 | 3
pub const TSS_SELECTOR: u16 = 0x28;

static mut GDT: [GdtEntry; 6] = [
    GdtEntry::missing(),
    // 0x08: ring 0 code
    GdtEntry::new(0, 0xFFFFF, 0x9A, 0xCF),
    // 0x10: ring 0 data
    GdtEntry::new(0, 0xFFFFF, 0x92, 0xCF),
    // 0x18: ring 3 code (DPL 3)
    GdtEntry::new(0, 0xFFFFF, 0xFA, 0xCF),
    // 0x20: ring 3 data (DPL 3)
    GdtEntry::new(0, 0xFFFFF, 0xF2, 0xCF),
    // 0x28: TSS
    GdtEntry::missing(),
];

static mut TSS: Tss = Tss::missing();

unsafe extern "C" {
    fn gdt_flush(pointer: *const GdtPointer);
    fn tss_flush();
}

extern "C" {
    static stack_top: u8;
}

fn set_tss_entry(tss_addr: u32, limit: u32) {
    let entries = unsafe { core::ptr::addr_of_mut!(GDT) };

    // TSS descriptor: present, DPL 0, type 0x9 (available 32-bit TSS)
    unsafe {
        (*entries)[5] = GdtEntry::new(tss_addr, limit, 0x89, 0x40);
    }
}

pub fn init() {
    unsafe {
        let tss_ptr = core::ptr::addr_of_mut!(TSS);
        (*tss_ptr).iomap_base = core::mem::size_of::<Tss>() as u16;
        (*tss_ptr).ss0 = KERNEL_DATA_SELECTOR as u32;
        (*tss_ptr).esp0 = core::ptr::addr_of!(stack_top) as u32;
    }

    let tss_addr = core::ptr::addr_of!(TSS) as *const u8 as u32;
    let tss_limit = (core::mem::size_of::<Tss>() - 1) as u32;
    set_tss_entry(tss_addr, tss_limit);

    let pointer = GdtPointer {
        limit: (core::mem::size_of::<[GdtEntry; 6]>() - 1) as u16,
        base: core::ptr::addr_of!(GDT) as *const u8 as u32,
    };

    unsafe {
        gdt_flush(&pointer);
        tss_flush();
    }
}

pub fn set_kernel_stack(stack: u32) {
    unsafe {
        let tss_ptr = core::ptr::addr_of_mut!(TSS);
        (*tss_ptr).esp0 = stack;
    }
}
pub fn kernel_stack() -> u32 {
    unsafe { (*core::ptr::addr_of!(TSS)).esp0 }
}

