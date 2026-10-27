use core::arch::asm;
use crate::debug;
use crate::println;
use core::sync::atomic::{AtomicU32, Ordering};

static TIMER_TICKS: AtomicU32 = AtomicU32::new(0);

const PIT_FREQUENCY: u32 = 100;

const IDT_ENTRIES: usize = 256;
const EXCEPTION_COUNT: usize = 32;
const IRQ_COUNT: usize = 16;
const IRQ_VECTOR_OFFSET: usize = 32;
const SYSCALL_VECTOR: usize = 0x80;
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

    fn new_user(handler_address: u32) -> Self {
        Self {
            offset_low: handler_address as u16,
            selector: KERNEL_CODE_SELECTOR,
            zero: 0,
            attributes: 0xEE, // Present, DPL 3, interrupt gate
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

    fn irq0();
    fn irq1();
    fn irq2();
    fn irq3();
    fn irq4();
    fn irq5();
    fn irq6();
    fn irq7();
    fn irq8();
    fn irq9();
    fn irq10();
    fn irq11();
    fn irq12();
    fn irq13();
    fn irq14();
    fn irq15();

    fn isr128(); // syscall
}

#[no_mangle]
pub extern "C" fn xinyi_exception_handler(
    vector: u32,
    error_code: u32,
    eip: u32,
    cs: u32,
    eflags: u32,
) -> u32 {
    let mut fault_address = 0;
    if vector == 14 {
        unsafe { asm!("mov {}, cr2", out(reg) fault_address, options(nomem, nostack)); }
    }
    let recover = crate::usermode::request_fault(cs, vector, error_code, eip, fault_address);
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
    if recover {crate::window_api::finish();}else{crate::graphics::restore();}
    println!("Xinyi: CPU exception!");
    println!("Exception: {}", name);
    println!("Vector: {}", vector);
    println!("Error code: {:#010x}", error_code);
    println!("EIP: {:#010x}  CS: {:#06x}", eip, cs);
    println!("EFLAGS: {:#010x}", eflags);

    if vector == 14 {
        println!("CR2 (fault address): {:#010x}", fault_address);
        debug::write_fmt(format_args!("Fault address: {:08x}\n", fault_address));
    }

    if recover { println!("User fault; returning to shell."); }
    else { println!("System halted."); }

    // Mirror the essential fault information to QEMU's debug console so
    // faults can be diagnosed from a headless or automated run.
    debug::write_str("Xinyi exception: ");
    debug::write_str(name);
    debug::write_str(" vector=");
    debug::write_u32(vector);
    debug::write_str(" error=");
    debug::write_hex32(error_code);
    debug::write_str(" eip=");
    debug::write_hex32(eip);
    debug::write_str(" cs=");
    debug::write_hex32(cs);
    debug::write_str(" eflags=");
    debug::write_hex32(eflags);
    debug::write_str("\r\n");

    if recover { return 1; }

    loop {
        unsafe {
            asm!("cli", "hlt", options(nomem, nostack));
        }
    }
}

#[no_mangle]
pub extern "C" fn xinyi_irq_handler(vector: u32) {
    if vector == 32 {
        TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    } else if vector == 33 || vector == 44 {
        crate::keyboard::handle_irq();
    }

    unsafe {
        // Acknowledge the slave PIC first for IRQ vectors 40–47.
        if (40..48).contains(&vector) {
            asm!(
                "out dx, al",
                in("dx") 0xA0u16,
                in("al") 0x20u8,
                options(nomem, nostack, preserves_flags)
            );
        }

        // Acknowledge the master PIC.
        asm!(
            "out dx, al",
            in("dx") 0x20u16,
            in("al") 0x20u8,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[no_mangle]
pub extern "C" fn xinyi_syscall_handler(frame: &crate::syscall_frame::SyscallFrame) -> u32 {
    let syscall_num = frame.eax;
    let (arg1, arg2, arg3) = (frame.ebx, frame.ecx, frame.edx);
    debug::write_fmt(format_args!(
        "Syscall frame: nr={} args={:08x}/{:08x}/{:08x} cs={:04x}\n",
        syscall_num, arg1, arg2, arg3, frame.cs
    ));

    if crate::window_api::active()&&matches!(syscall_num,4|50|53..=55) {return (-16i32) as u32;}
    match syscall_num {
        1 => {
            // sys_exit
            if crate::usermode::request_exit(frame.cs, arg1) { 0 }
            else { 0xffffffff }
        }
        4 => {
            crate::usermode::write(frame.cs, arg1, arg2, arg3)
        }
        20 => {
            // sys_getpid
            crate::usermode::getpid()
        }
        50..=56 => crate::terminal::syscall(frame.cs, syscall_num, arg1, arg2, arg3),
        60..=64 => crate::window_api::syscall(frame.cs,syscall_num,arg1,arg2,arg3),
        57..=59 => crate::files_api::syscall(frame.cs,syscall_num,arg1,arg2,arg3),
        _ => {
            0xFFFFFFFF
        }
    }
}
/// Remap the PIC so hardware IRQs use IDT vectors 32–47.
/// Initially unmask IRQ1 and IRQ0 (the PIT timer).
pub fn remap_pic() {
    unsafe {
        // Start PIC initialization.
        asm!("out dx, al", in("dx") 0x20u16, in("al") 0x11u8,
             options(nomem, nostack, preserves_flags));
        asm!("out dx, al", in("dx") 0xA0u16, in("al") 0x11u8,
             options(nomem, nostack, preserves_flags));

        // Master PIC starts at vector 32; slave PIC at vector 40.
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0x20u8,
             options(nomem, nostack, preserves_flags));
        asm!("out dx, al", in("dx") 0xA1u16, in("al") 0x28u8,
             options(nomem, nostack, preserves_flags));

        // Tell each PIC how they are connected.
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0x04u8,
             options(nomem, nostack, preserves_flags));
        asm!("out dx, al", in("dx") 0xA1u16, in("al") 0x02u8,
             options(nomem, nostack, preserves_flags));

        // Set 8086 mode.
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0x01u8,
             options(nomem, nostack, preserves_flags));
        asm!("out dx, al", in("dx") 0xA1u16, in("al") 0x01u8,
             options(nomem, nostack, preserves_flags));

        // Unmask IRQ0 (PIT timer) and IRQ1 (keyboard).
		asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFCu8,
			options(nomem, nostack, preserves_flags));

		// Keep all slave PIC IRQs masked.
		asm!("out dx, al", in("dx") 0xA1u16, in("al") 0xFFu8,
			options(nomem, nostack, preserves_flags));
    }
}

/// Configure the PIT to interrupt 100 times per second.
pub fn init_pit() {
    let divisor = 1_193_182u32 / PIT_FREQUENCY;

    unsafe {
        // Channel 0, low byte then high byte, square-wave mode.
        asm!(
            "out dx, al",
            in("dx") 0x43u16,
            in("al") 0x36u8,
            options(nomem, nostack, preserves_flags)
        );

        asm!(
            "out dx, al",
            in("dx") 0x40u16,
            in("al") (divisor & 0xFF) as u8,
            options(nomem, nostack, preserves_flags)
        );

        asm!(
            "out dx, al",
            in("dx") 0x40u16,
            in("al") ((divisor >> 8) & 0xFF) as u8,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Return the number of timer ticks since PIT initialization.
pub fn timer_ticks() -> u32 {
    TIMER_TICKS.load(Ordering::Relaxed)
}

pub fn uptime_seconds() -> u32 {
    timer_ticks() / PIT_FREQUENCY
}

pub fn init() {
    unsafe {
        let exception_handlers: [u32; EXCEPTION_COUNT] = [
            isr0 as *const () as usize as u32,
            isr1 as *const () as usize as u32,
            isr2 as *const () as usize as u32,
            isr3 as *const () as usize as u32,
            isr4 as *const () as usize as u32,
            isr5 as *const () as usize as u32,
            isr6 as *const () as usize as u32,
            isr7 as *const () as usize as u32,
            isr8 as *const () as usize as u32,
            isr9 as *const () as usize as u32,
            isr10 as *const () as usize as u32,
            isr11 as *const () as usize as u32,
            isr12 as *const () as usize as u32,
            isr13 as *const () as usize as u32,
            isr14 as *const () as usize as u32,
            isr15 as *const () as usize as u32,
            isr16 as *const () as usize as u32,
            isr17 as *const () as usize as u32,
            isr18 as *const () as usize as u32,
            isr19 as *const () as usize as u32,
            isr20 as *const () as usize as u32,
            isr21 as *const () as usize as u32,
            isr22 as *const () as usize as u32,
            isr23 as *const () as usize as u32,
            isr24 as *const () as usize as u32,
            isr25 as *const () as usize as u32,
            isr26 as *const () as usize as u32,
            isr27 as *const () as usize as u32,
            isr28 as *const () as usize as u32,
            isr29 as *const () as usize as u32,
            isr30 as *const () as usize as u32,
            isr31 as *const () as usize as u32,
        ];

        let irq_handlers: [u32; IRQ_COUNT] = [
            irq0 as *const () as usize as u32,
            irq1 as *const () as usize as u32,
            irq2 as *const () as usize as u32,
            irq3 as *const () as usize as u32,
            irq4 as *const () as usize as u32,
            irq5 as *const () as usize as u32,
            irq6 as *const () as usize as u32,
            irq7 as *const () as usize as u32,
            irq8 as *const () as usize as u32,
            irq9 as *const () as usize as u32,
            irq10 as *const () as usize as u32,
            irq11 as *const () as usize as u32,
            irq12 as *const () as usize as u32,
            irq13 as *const () as usize as u32,
            irq14 as *const () as usize as u32,
            irq15 as *const () as usize as u32,
        ];

        let idt = core::ptr::addr_of_mut!(IDT);

        // Install CPU exception handlers.
        for vector in 0..EXCEPTION_COUNT {
            (*idt)[vector] =
                IdtEntry::new(exception_handlers[vector]);
        }

        // Install hardware IRQ handlers at vectors 32–47.
        for irq in 0..IRQ_COUNT {
            (*idt)[IRQ_VECTOR_OFFSET + irq] =
                IdtEntry::new(irq_handlers[irq]);
        }

        // Install syscall handler at 0x80 (accessible from ring 3)
        (*idt)[SYSCALL_VECTOR] = IdtEntry::new_user(isr128 as *const () as usize as u32);

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
