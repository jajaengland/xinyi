// User-mode transition test.
//
// This drops from ring 0 to ring 3 using an IRET stack frame built in
// boot.asm. The user code writes a message straight to the VGA text buffer
// (no privileged I/O), then attempts a privileged instruction (CLI) which
// generates a General Protection Fault (GPF). The kernel's exception handler
// reports it with CS=0x1B, demonstrating privilege separation is working.
//
// Note: user code must not call kernel printing helpers. They use VGA cursor
// port I/O, which is privileged and faults in ring 3.

use core::arch::asm;

use crate::{gdt, println};

const VGA_BUFFER: *mut u16 = 0xB8000 as *mut u16;
const USER_MESSAGE_ROW: usize = 20;
const USER_MESSAGE: &[u8] = b"  In ring 3! Attempting privileged instruction (cli)...";

unsafe extern "C" {
    fn enter_user_mode(entry: u32) -> !;
}

fn write_user_message() {
    let blank = 0x0F20u16; // white space

    for column in 0..80 {
        unsafe {
            VGA_BUFFER
                .add(USER_MESSAGE_ROW * 80 + column)
                .write_volatile(blank);
        }
    }

    for (column, byte) in USER_MESSAGE.iter().enumerate() {
        unsafe {
            VGA_BUFFER
                .add(USER_MESSAGE_ROW * 80 + column)
                .write_volatile(0x0F00 | *byte as u16);
        }
    }
}

#[no_mangle]
pub extern "C" fn usermode_entry() {
    write_user_message();

    // Exercise the ring-3 -> ring-0 syscall path before faulting. The kernel
    // switches to the TSS kernel stack and prints the result.
    unsafe {
        asm!(
            "mov eax, 20", // sys_getpid
            "int 0x80",
            out("eax") _,
            options(nostack, preserves_flags),
        );
    }

    unsafe {
        asm!("cli");
    }

    // CLI faults in ring 3, so this loop is unreachable.
    loop {}
}

pub fn enter() {
    println!("  Jumping to ring 3 via IRET...");
    println!(
        "  User CS: {:#06x}, User DS: {:#06x}",
        gdt::USER_CODE_SELECTOR,
        gdt::USER_DATA_SELECTOR
    );

    unsafe {
        enter_user_mode(usermode_entry as *const () as u32);
    }
}
