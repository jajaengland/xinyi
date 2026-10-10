
use core::arch::asm;
use crate::println;

const IDT_ENTRIES: usize = 256;
const EXCEPTION_COUNT: usize = 32;
const KERNEL_CODE_SELECTOR: u16 = 0x08;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    zero: u8,
    attributes: u8,
    offset_high: u16,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            zero: 0,
            attributes: 0,
            offset_high: 0,
        }
    }

    fn new(handler_address: u32) -> Self {
        Self {
            offset_low: handler_address as u16,
            selector: KERNEL_CODE_SELECTOR,
            zero: 0,
            attributes: 0x8E,
            offset_high: (handler_address >> 16) as u16,
        }
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u32,
}

static mut IDT: [IdtEntry; IDT_ENTRIES] =
    [IdtEntry::missing(); IDT_ENTRIES];

unsafe extern "C" {
    fn isr0();
    fn isr1();
    fn isr2();
    fn isr3();
    fn isr4();
    fn isr5();
    fn isr6();
    fn isr7();
    fn isr8();
    fn isr9();
    fn isr10();
    fn isr11();
    fn isr12();
    fn isr13();
    fn isr14();
    fn isr15();
    fn isr16();
    fn isr17();
    fn isr18();
    fn isr19();
    fn isr20();
    fn isr21();
    fn isr22();
    fn isr23();
    fn isr24();
    fn isr25();
    fn isr26();
    fn isr27();
    fn isr28();
    fn isr29();
    fn isr30();
    fn isr31();
}

#[no_mangle]
pub extern "C" fn xinyi_exception_handler(
    vector: u32,
    error_code: u32,
) -> ! {
    let name = match vector {
        0 => "Divide Error",
        1 => "Debug",
        2 => "Non-Maskable Interrupt",
        3 => "Breakpoint",
        4 => "Overflow",
        5 => "BOUND Range Exceeded",
        6 => "Invalid Opcode",
        7 => "Device Not Available",
        8 => "Double Fault",
        9 => "Coprocessor Segment Overrun",
        10 => "Invalid TSS",
        11 => "Segment Not Present",
        12 => "Stack-Segment Fault",
        13 => "General Protection Fault",
        14 => "Page Fault",
        15 => "Reserved",
        16 => "x87 Floating-Point Exception",
        17 => "Alignment Check",
        18 => "Machine Check",
        19 => "SIMD Floating-Point Exception",
        20 => "Virtualization Exception",
        21 => "Control Protection Exception",
        22..=27 => "Reserved",
        28 => "Hypervisor Injection Exception",
        29 => "VMM Communication Exception",
        30 => "Security Exception",
        31 => "Reserved",
        _ => "Unknown Exception",
    };

    println!();
    println!("Xinyi: CPU exception!");
    println!("Exception: {}", name);
    println!("Vector: {}", vector);
    println!("Error code: {:#010x}", error_code);
    println!("System halted.");

    loop {
        unsafe {
            asm!("cli", "hlt", options(nomem, nostack));
        }
    }
}

pub fn init() {
    unsafe {
        let handlers: [u32; EXCEPTION_COUNT] = [
            isr0 as usize as u32,
            isr1 as usize as u32,
            isr2 as usize as u32,
            isr3 as usize as u32,
            isr4 as usize as u32,
            isr5 as usize as u32,
            isr6 as usize as u32,
            isr7 as usize as u32,
            isr8 as usize as u32,
            isr9 as usize as u32,
            isr10 as usize as u32,
            isr11 as usize as u32,
            isr12 as usize as u32,
            isr13 as usize as u32,
            isr14 as usize as u32,
            isr15 as usize as u32,
            isr16 as usize as u32,
            isr17 as usize as u32,
            isr18 as usize as u32,
            isr19 as usize as u32,
            isr20 as usize as u32,
            isr21 as usize as u32,
            isr22 as usize as u32,
            isr23 as usize as u32,
            isr24 as usize as u32,
            isr25 as usize as u32,
            isr26 as usize as u32,
            isr27 as usize as u32,
            isr28 as usize as u32,
            isr29 as usize as u32,
            isr30 as usize as u32,
            isr31 as usize as u32,
        ];

        let idt = core::ptr::addr_of_mut!(IDT);

        for vector in 0..EXCEPTION_COUNT {
            (*idt)[vector] = IdtEntry::new(handlers[vector]);
        }

        let pointer = IdtPointer {
            limit: (core::mem::size_of::<IdtEntry>() * IDT_ENTRIES - 1)
                as u16,
            base: idt as u32,
        };

        asm!(
            "lidt [{0}]",
            in(reg) &pointer,
            options(readonly, nostack, preserves_flags)
        );
    }
}
