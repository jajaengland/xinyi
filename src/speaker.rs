// Legacy PC speaker: PIT channel 2 only. Channel 0 remains the system clock.
use core::arch::asm;

unsafe fn read(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}
unsafe fn write(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}
pub fn stop() {
    unsafe { let control = read(0x61); write(0x61, control & !3); }
}
pub fn beep(frequency: u32, milliseconds: u32) -> u32 {
    if milliseconds > 1000 || (frequency != 0 && !(37..=20_000).contains(&frequency)) {
        return (-22i32) as u32;
    }
    if frequency == 0 || milliseconds == 0 { stop(); return 0; }
    let divisor = (1_193_182 / frequency) as u16;
    unsafe {
        write(0x43, 0xb6); // Channel 2, low/high divisor, square wave.
        write(0x42, divisor as u8);
        write(0x42, (divisor >> 8) as u8);
        let control = read(0x61);
        write(0x61, control | 3);
    }
    crate::debug::write_fmt(format_args!("Speaker: {} Hz for {} ms\n", frequency, milliseconds));
    let started = crate::interrupts::timer_ticks();
    let ticks = (milliseconds + 9) / 10 + 1;
    while crate::interrupts::timer_ticks().wrapping_sub(started) < ticks {
        // Entered through an interrupt-gate syscall with IF=0. Allow PIT and
        // keyboard IRQs while halted; return to Rust with interrupts disabled.
        unsafe { asm!("sti; hlt; cli", options(nostack)); }
    }
    stop();
    0
}
