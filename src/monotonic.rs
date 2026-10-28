use core::arch::asm;
use crate::clock_model::Clock;

struct Source { port: u16, clock: Clock }
static mut SOURCE: Option<Source> = None;

fn read(port: u16) -> u32 {
    let value: u32;
    unsafe { asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack, preserves_flags)); }
    value
}

// Called once, with interrupts disabled and the boot memory map available.
pub fn init() {
    let source = crate::power::fadt().and_then(crate::clock_model::pm_timer);
    if let Some((port, mask)) = source {
        unsafe { SOURCE = Some(Source { port, clock: Clock::new(read(port), mask) }); }
        crate::debug::write_fmt(format_args!("Clock: ACPI PM timer port={:04x} mask={:08x}\n", port, mask));
    } else {
        crate::debug::write_str("Clock: PIT interrupt fallback\n");
    }
}

pub fn ticks() -> Option<u32> {
    let flags: u32;
    unsafe {
        asm!("pushfd", "pop {0}", "cli", out(reg) flags);
        let result = (&mut *core::ptr::addr_of_mut!(SOURCE)).as_mut()
            .map(|source| source.clock.sample(read(source.port)));
        if flags & 0x200 != 0 { asm!("sti", options(nostack)); }
        result
    }
}
