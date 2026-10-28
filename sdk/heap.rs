//! Single-threaded per-process allocator over the bounded kernel break.
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr,
};
pub const BASE: usize = 0x40100000;
pub const LIMIT: usize = 0x40200000;
pub fn program_break(end: usize) -> Result<usize, i32> {
    let n = super::syscall(87, end as u32, 0, 0);
    if n < 0 {
        Err(n)
    } else {
        Ok(n as usize)
    }
}
#[repr(C)]
struct Free {
    size: usize,
    next: *mut Free,
}
#[repr(C)]
struct Header {
    start: usize,
    size: usize,
}
pub struct Allocator {
    head: UnsafeCell<*mut Free>,
}
unsafe impl Sync for Allocator {}
impl Allocator {
    pub const fn new() -> Self {
        Self {
            head: UnsafeCell::new(ptr::null_mut()),
        }
    }
    unsafe fn release(&self, start: usize, size: usize) {
        let head = &mut *self.head.get();
        let mut link = head as *mut *mut Free;
        while !(*link).is_null() && (*link as usize) < start {
            link = &mut (**link).next;
        }
        let node = start as *mut Free;
        node.write(Free { size, next: *link });
        *link = node;
        let mut current = *head;
        while !current.is_null() {
            let next = (*current).next;
            if !next.is_null() && current as usize + (*current).size == next as usize {
                (*current).size += (*next).size;
                (*current).next = (*next).next;
            } else {
                current = next;
            }
        }
    }
}
fn aligned(value: usize, align: usize) -> Option<usize> {
    value.checked_add(align - 1).map(|n| n & !(align - 1))
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() > LIMIT - BASE || layout.align() > LIMIT - BASE {
            return ptr::null_mut();
        }
        for _ in 0..2 {
            let mut link = self.head.get();
            while !(*link).is_null() {
                let node = *link;
                let start = node as usize;
                let Some(pointer) = aligned(
                    start + core::mem::size_of::<Header>(),
                    layout.align().max(8),
                ) else {
                    return ptr::null_mut();
                };
                let Some(end) = pointer
                    .checked_add(layout.size().max(1))
                    .and_then(|n| aligned(n, 8))
                else {
                    return ptr::null_mut();
                };
                let node_end = start + (*node).size;
                if end <= node_end {
                    let size = if node_end - end >= 16 {
                        let next = end as *mut Free;
                        next.write(Free {
                            size: node_end - end,
                            next: (*node).next,
                        });
                        *link = next;
                        end - start
                    } else {
                        *link = (*node).next;
                        node_end - start
                    };
                    ((pointer - core::mem::size_of::<Header>()) as *mut Header)
                        .write(Header { start, size });
                    return pointer as *mut u8;
                }
                link = &mut (*node).next;
            }
            let Ok(start) = program_break(0) else {
                return ptr::null_mut();
            };
            let Some(bytes) = layout
                .size()
                .max(1)
                .checked_add(layout.align().max(8) + 16)
                .and_then(|n| aligned(n, 4096))
            else {
                return ptr::null_mut();
            };
            let Some(end) = start.checked_add(bytes).filter(|&n| n <= LIMIT) else {
                return ptr::null_mut();
            };
            if program_break(end).is_err() {
                return ptr::null_mut();
            }
            self.release(start, bytes);
        }
        ptr::null_mut()
    }
    unsafe fn dealloc(&self, pointer: *mut u8, _layout: Layout) {
        let h = (pointer as usize - core::mem::size_of::<Header>()) as *const Header;
        let Header { start, size } = h.read();
        self.release(start, size);
    }
}
