// Capture ordering shared by the serialized PS/2 IRQ producers. Compare with
// wrapping subtraction; bounded queues keep live sequence distances small.
use core::sync::atomic::{AtomicU32, Ordering};
static NEXT: AtomicU32 = AtomicU32::new(0);
pub fn next() -> u32 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
pub fn before(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}
