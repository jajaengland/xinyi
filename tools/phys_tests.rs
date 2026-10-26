#![allow(dead_code)]
#[path = "../src/phys.rs"] mod phys;
#[no_mangle] static _kernel_start: u8 = 0;
#[no_mangle] static _kernel_end: u8 = 0;
mod paging {
    pub const USER_BASE: u32 = 0x40000000;
    pub const USER_WINDOW_BYTES: u32 = 0x400000;
    pub struct Status { pub page_directory: u32 }
    pub fn status() -> Status { Status { page_directory: 0 } }
}
mod multiboot {
    pub struct Region { pub base: u64, pub length: u64, pub kind: u32 }
    pub fn for_each_region(mut visit: impl FnMut(Region)) {
        visit(Region { base: 0x100000, length: 32 * 4096, kind: 1 });
    }
    pub fn memory_summary() -> Option<(u32, u32)> { None }
    pub fn info_address() -> Option<u32> { None }
    pub fn mmap_range() -> Option<(u32, u32)> { None }
}

#[test]
fn contiguous_claims_fragmentation_exhaustion_and_release_balance() {
    assert_eq!(phys::init().free_frames, 32);
    assert_eq!(phys::alloc_frames(0), None);
    assert_eq!(phys::alloc_frames(33), None);
    let base = phys::alloc_frames(32).unwrap();
    assert_eq!(base, 0x100000);
    assert_eq!(phys::stats().free_frames, 0);
    assert_eq!(phys::alloc_frame(), None);
    for page in [10, 12] { assert!(phys::free_frame(base + page * 4096)); }
    assert_eq!(phys::alloc_frames(2), None); // fragmented, not contiguous
    assert_eq!(phys::stats().free_frames, 2);
    assert!(phys::free_frame(base + 11 * 4096));
    assert_eq!(phys::alloc_frames(3), Some(base + 10 * 4096));
    assert_eq!(phys::stats().free_frames, 0);
    for page in 0..32 { assert!(phys::free_frame(base + page * 4096)); }
    assert!(!phys::free_frame(base)); // double free
    assert_eq!(phys::stats().free_frames, 32);
    assert_eq!(phys::stats().allocated_frames, 0);
}
