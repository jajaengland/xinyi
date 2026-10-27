#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::directory::{self, Listing};
fn main() -> u32 {
    let path = b".";
    let mut listing = Listing::new();
    let out = &mut listing as *mut _ as u32;
    assert_eq!(xinyi::syscall(78, 0x100000, 1, out), -14);
    assert_eq!(xinyi::syscall(78, path.as_ptr() as u32, 1, 0x40000000), -14);
    assert_eq!(xinyi::syscall(78, path.as_ptr() as u32, 106, out), -22);
    assert_eq!(xinyi::syscall(79, path.as_ptr() as u32, 1, 1), -22);
    assert_eq!(xinyi::syscall(80, path.as_ptr() as u32, 1, 3), -22);
    assert_eq!(directory::cd(&[255]), -22);
    assert_eq!(directory::action(b"bad long name", 0), -22);
    assert_eq!(directory::action(b"/dircheck", 0), 0);
    assert_eq!(directory::cd(b"/dircheck"), 0);
    for i in 0..35 {
        let name = [b'F', b'0' + i / 10, b'0' + i % 10, b'.', b'T', b'X', b'T'];
        assert_eq!(xinyi::save_file(&name, b"x"), 1);
    }
    assert_eq!(directory::list(b".", 0, &mut listing), 0);
    assert_eq!(listing.count, 32);
    assert_eq!(listing.total, 35);
    assert_eq!(&listing.entries[0].name[..7], b"F00.TXT");
    assert_eq!(directory::list(b".", 32, &mut listing), 0);
    assert_eq!(listing.count, 3);
    assert_eq!(listing.total, 35);
    assert_eq!(&listing.entries[0].name[..7], b"F32.TXT");
    assert_eq!(listing.entries[3].length, 0);
    assert_eq!(directory::list(b".", 65535, &mut listing), 0);
    assert_eq!(listing.count, 0);
    assert_eq!(listing.total, 35);
    assert_eq!(directory::cd(b"F00.TXT"), -20);
    assert_eq!(directory::cd(b"/"), 0);
    assert_eq!(directory::action(b"/dircheck", 2), -39);
    assert_eq!(xinyi::pty::open(), -16);
    assert_eq!(xinyi::pty::control(9999, 3), -9);
    for i in 0..35 {
        let name = [
            b'/',
            b'd',
            b'i',
            b'r',
            b'c',
            b'h',
            b'e',
            b'c',
            b'k',
            b'/',
            b'F',
            b'0' + i / 10,
            b'0' + i % 10,
            b'.',
            b'T',
            b'X',
            b'T',
        ];
        assert_eq!(directory::action(&name, 1), 0);
    }
    assert_eq!(directory::action(b"/dircheck", 2), 0);
    xinyi::print("Directory API checks passed!\n");
    0
}
