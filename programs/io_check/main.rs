#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
use alloc::vec::Vec;
#[path = "../../sdk/xinyi.rs"] mod xinyi;
use xinyi::{directory, files, window::{self, Draw}};
#[global_allocator] static HEAP: xinyi::heap::Allocator = xinyi::heap::Allocator::new();
#[alloc_error_handler] fn oom(_: core::alloc::Layout) -> ! { xinyi::exit(12) }

fn status(window: u32, color: u32) {
    window::draw(window, &[Draw::rect(8, 48, 16, 16, color)]);
    window::present(window);
}

fn main() -> u32 {
    let handle = window::open_sized(b"Disk I/O check", 320, 144);
    if handle < 0 { return 1; }
    let handle = handle as u32;
    window::draw(handle, &[
        Draw::rect(0, 0, 320, 144, 0xffffff),
        Draw::text(8, 8, 0, b"W: write   R: check   C: copy"),
        Draw::text(8, 24, 0, b"K: final copy   L: stale   Q: quit"),
    ]);
    status(handle, 0x777777);
    let mut data = Vec::new();
    data.resize(65536, 0u8);
    for (index, byte) in data.iter_mut().enumerate() { *byte = (index % 95 + 32) as u8; }
    loop {
        let mut event = window::Event::default();
        if window::event(handle, &mut event, true) < 0 { return 2; }
        if event.kind == window::CLOSE { return 0; }
        if event.kind != window::KEY { continue; }
        match event.code as u16 {
            86 | 118 | 87 | 119 => {
                status(handle, 0xaa0000);
                assert_eq!(directory::action(b"/SOURCE", 0), 0);
                assert_eq!(directory::action(b"/SOURCE/NEST", 0), 0);
                let count = if matches!(event.code as u16, 86 | 118) { 2 } else { 8 };
                for path in [b"/SOURCE/NEST/A.BIN", b"/SOURCE/NEST/B.BIN", b"/SOURCE/NEST/C.BIN", b"/SOURCE/NEST/D.BIN", b"/SOURCE/NEST/E.BIN", b"/SOURCE/NEST/F.BIN", b"/SOURCE/NEST/G.BIN", b"/SOURCE/NEST/H.BIN"].into_iter().take(count) {
                    assert_eq!(files::save(path, &data), 65536);
                }
                status(handle, 0x00aa00);
            }
            82 | 114 => {
                let mut tiny = [0x55; 16];
                assert_eq!(files::read(b"/SOURCE/NEST/A.BIN", &mut tiny), -28);
                assert_eq!(tiny, [0x55; 16]);
                let volume = directory::volume();
                assert_eq!(files::save_on(b"/SOURCE/NEST/A.BIN", b"wrong", volume.wrapping_sub(1)), -116);
                assert_eq!(files::read(b"/MISSING.TXT", &mut tiny), -2);
                assert_eq!(xinyi::syscall(88, 0x100000, 20, 0), -14);
                let mut output = Vec::new();output.resize(65536, 0);
                assert_eq!(files::read(b"/SOURCE/NEST/A.BIN", &mut output), 65536);
                assert_eq!(output, data);
                status(handle, 0x7755aa);
            }
            67 | 99 => {
                status(handle, 0x0000aa);
                assert_eq!(directory::transfer(b"/SOURCE", b"/DEST", 1), 0);
                let mut output = Vec::new();output.resize(65536, 0);
                assert_eq!(files::read(b"/DEST/NEST/A.BIN", &mut output), 65536);
                assert_eq!(output, data);
                status(handle, 0x00aaaa);
            }
            75 | 107 => {
                status(handle, 0xaa5500);
                assert_eq!(directory::transfer(b"/SOURCE", b"/FINISH", 1), 0);
                status(handle, 0x00aa00);
            }
            76 | 108 => {
                assert_eq!(xinyi::save_file(b"/STALE.TXT", b"wrong volume"), -116);
                status(handle, 0xaa00aa);
            }
            81 | 113 => return 0,
            _ => (),
        }
    }
}
