// Physical frame allocator.
//
// A bitmap tracks every 4 KiB frame in the 32-bit physical address space.
// Frames start reserved, available regions from the multiboot map are opened
// up, and then the low 1 MiB, the kernel image, and boot structures are
// reserved again. Paging is identity-mapped, so a frame address is also a
// usable virtual address.
//
// The allocator manages physical memory only. It does not provide a heap and
// does not track which subsystem owns a frame; callers must free only frames
// returned by `alloc_frame`.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::{multiboot, paging};

const FRAME_SIZE: u64 = 4096;
const MAX_FRAMES: usize = 1_048_576; // 4 GiB of 4 KiB frames
const BITMAP_WORDS: usize = MAX_FRAMES / 64;

static mut BITMAP: [u64; BITMAP_WORDS] = [0; BITMAP_WORDS];

static AVAILABLE_FRAMES: AtomicU32 = AtomicU32::new(0);
static FREE_FRAMES: AtomicU32 = AtomicU32::new(0);
static HINT: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy)]
pub struct Stats {
    pub available_frames: u32,
    pub free_frames: u32,
    pub allocated_frames: u32,
}

extern "C" {
    static _kernel_start: u8;
    static _kernel_end: u8;
}

fn bitmap_pointer() -> *mut u64 {
    core::ptr::addr_of_mut!(BITMAP) as *mut u64
}

fn bitmap_address() -> u64 {
    core::ptr::addr_of!(BITMAP) as *const u8 as u64
}

fn set_used(frame: usize) {
    debug_assert!(frame < MAX_FRAMES);

    unsafe {
        let word = bitmap_pointer().add(frame / 64);
        word.write(word.read() | (1u64 << (frame % 64)));
    }
}

fn set_free(frame: usize) {
    debug_assert!(frame < MAX_FRAMES);

    unsafe {
        let word = bitmap_pointer().add(frame / 64);
        word.write(word.read() & !(1u64 << (frame % 64)));
    }
}

fn is_used(frame: usize) -> bool {
    debug_assert!(frame < MAX_FRAMES);

    unsafe {
        let word = bitmap_pointer().add(frame / 64);
        word.read() & (1u64 << (frame % 64)) != 0
    }
}

fn reserve_range(start: u64, end: u64) {
    if start >= end {
        return;
    }

    let first = (start / FRAME_SIZE) as usize;
    let last = (end.saturating_add(FRAME_SIZE - 1) / FRAME_SIZE) as usize;

    for frame in first..last.min(MAX_FRAMES) {
        set_used(frame);
    }
}

fn open_range(start: u64, end: u64) {
    if start >= end {
        return;
    }

    let first = (start / FRAME_SIZE) as usize;
    let last = (end.saturating_add(FRAME_SIZE - 1) / FRAME_SIZE) as usize;

    for frame in first..last.min(MAX_FRAMES) {
        set_free(frame);
    }
}

fn kernel_range() -> (u64, u64) {
    let start = core::ptr::addr_of!(_kernel_start) as *const u8 as u64;
    let end = core::ptr::addr_of!(_kernel_end) as *const u8 as u64;

    (start, end)
}

/// Build the frame bitmap from the boot memory map.
pub fn init() -> Stats {
    // Start with every frame reserved. Opening available regions later means
    // holes, firmware ranges, and memory beyond the map stay unavailable.
    for index in 0..BITMAP_WORDS {
        unsafe {
            bitmap_pointer().add(index).write(u64::MAX);
        }
    }

    let mut opened_any = false;

    multiboot::for_each_region(|region| {
        if region.kind == 1 {
            let end = region.base.saturating_add(region.length);
            open_range(region.base, end);
            opened_any = true;
        }
    });

    // Fall back to the conventional mem_upper value if no map was supplied.
    if !opened_any {
        if let Some((_, upper)) = multiboot::memory_summary() {
            let start = 0x0010_0000u64;
            let end = start + (upper as u64) * 1024;
            open_range(start, end);
        }
    }

    // Real-mode data, VGA memory, and the multiboot structures normally live
    // here. Never hand out frames below 1 MiB.
    reserve_range(0, 0x0010_0000);

    let (kernel_start, kernel_end) = kernel_range();
    reserve_range(kernel_start, kernel_end);

    // Redundant while these live inside the kernel image, but explicit so the
    // allocations remain safe if their placement changes later.
    let bitmap_start = bitmap_address();
    reserve_range(bitmap_start, bitmap_start + (BITMAP_WORDS * 8) as u64);

    let directory = paging::status().page_directory as u64;
    reserve_range(directory, directory + FRAME_SIZE);

    if let Some(info) = multiboot::info_address() {
        reserve_range(info as u64, info as u64 + FRAME_SIZE);
    }

    if let Some((address, length)) = multiboot::mmap_range() {
        reserve_range(address as u64, address as u64 + length as u64);
    }

    let mut free = 0u32;

    for frame in 0..MAX_FRAMES {
        if !is_used(frame) {
            free += 1;
        }
    }

    AVAILABLE_FRAMES.store(free, Ordering::Relaxed);
    FREE_FRAMES.store(free, Ordering::Relaxed);
    HINT.store(0, Ordering::Relaxed);

    Stats {
        available_frames: free,
        free_frames: free,
        allocated_frames: 0,
    }
}

fn claim(frame: usize) -> bool {
    if is_used(frame) {
        return false;
    }

    set_used(frame);
    HINT.store(frame as u32 + 1, Ordering::Relaxed);
    FREE_FRAMES.fetch_sub(1, Ordering::Relaxed);

    true
}

/// Allocate one 4 KiB frame. Returns its physical (and identity-mapped) address.
pub fn alloc_frame() -> Option<u64> {
    let hint = HINT.load(Ordering::Relaxed) as usize;

    for frame in hint..MAX_FRAMES {
        if claim(frame) {
            return Some(frame as u64 * FRAME_SIZE);
        }
    }

    for frame in 0..hint.min(MAX_FRAMES) {
        if claim(frame) {
            return Some(frame as u64 * FRAME_SIZE);
        }
    }

    None
}

/// Release a frame previously returned by `alloc_frame`.
///
/// Reserved frames are not distinguishable from allocated frames yet, so
/// freeing an address that was never allocated would incorrectly release it.
pub fn free_frame(address: u64) -> bool {
    if address % FRAME_SIZE != 0 {
        return false;
    }

    let frame = (address / FRAME_SIZE) as usize;

    if frame >= MAX_FRAMES || !is_used(frame) {
        return false;
    }

    set_free(frame);
    FREE_FRAMES.fetch_add(1, Ordering::Relaxed);
    HINT.fetch_min(frame as u32, Ordering::Relaxed);

    true
}

pub fn stats() -> Stats {
    let available = AVAILABLE_FRAMES.load(Ordering::Relaxed);
    let free = FREE_FRAMES.load(Ordering::Relaxed);

    Stats {
        available_frames: available,
        free_frames: free,
        allocated_frames: available.saturating_sub(free),
    }
}
