//! Desktop timing in timestamp-counter ticks on Xinyi's single boot CPU.
use core::{
    arch::{asm, x86::__cpuid},
    sync::atomic::{AtomicBool, Ordering::Relaxed},
};

static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Stats {
    samples: u64,
    compose: u64,
    present: u64,
    compose_peak: u64,
    present_peak: u64,
}
static mut STATS: Stats = Stats {
    samples: 0,
    compose: 0,
    present: 0,
    compose_peak: 0,
    present_peak: 0,
};

fn with_stats<T>(run: impl FnOnce(&mut Stats) -> T) -> T {
    let flags: u32;
    unsafe {
        asm!("pushfd", "pop {0:e}", "cli", out(reg) flags);
        let value = run(&mut *core::ptr::addr_of_mut!(STATS));
        if flags & 0x200 != 0 {
            asm!("sti", options(nostack));
        }
        value
    }
}

pub fn init() {
    let features = __cpuid(1).edx;
    ENABLED.store(
        features & ((1 << 4) | (1 << 26)) == ((1 << 4) | (1 << 26)),
        Relaxed,
    );
}

pub fn stamp() -> Option<u64> {
    if !ENABLED.load(Relaxed) {
        return None;
    }
    let (lo, hi): (u32, u32);
    unsafe {
        asm!("lfence", "rdtsc", "lfence", out("eax") lo, out("edx") hi, options(nostack));
    }
    Some((u64::from(hi) << 32) | u64::from(lo))
}

pub fn record(start: Option<u64>, composed: Option<u64>) {
    if let (Some(start), Some(composed), Some(end)) = (start, composed, stamp()) {
        let compose = composed.saturating_sub(start);
        let present = end.saturating_sub(composed);
        with_stats(|s| {
            s.samples = s.samples.saturating_add(1);
            s.compose = s.compose.saturating_add(compose);
            s.present = s.present.saturating_add(present);
            s.compose_peak = s.compose_peak.max(compose);
            s.present_peak = s.present_peak.max(present);
        });
    }
}

pub fn summary() -> Option<[u64; 5]> {
    let s = with_stats(|stats| *stats);
    if s.samples == 0 {
        return None;
    }
    Some([
        s.samples,
        s.compose / s.samples,
        s.present / s.samples,
        s.compose_peak,
        s.present_peak,
    ])
}
