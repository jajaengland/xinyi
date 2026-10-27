// Shell context only: return with interrupts enabled.
pub fn wait_for_input() {
    wait(false);
}
pub fn wait_for_pointer() {wait(true);}
fn wait(pointer:bool) {
    unsafe {
        // These blocks deliberately keep compiler memory barriers. Recheck the
        // IRQ-owned queue with interrupts disabled to avoid a lost wakeup.
        core::arch::asm!("cli", options(nostack));
        if crate::keyboard::queued_scancodes() == 0 && (!pointer||!crate::mouse::pending()) {
            // STI's interrupt shadow makes enable-and-halt one safe sequence.
            core::arch::asm!("sti; hlt", options(nostack));
        } else {
            core::arch::asm!("sti", options(nostack));
        }
    }
}
