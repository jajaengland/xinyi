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
    assert_eq!(xinyi::syscall(81, 0x100000, 220, 0), -14);
    assert_eq!(xinyi::syscall(81, 0, 215, 0), -22);
    assert_eq!(xinyi::syscall(81, 0, 220, 1), -22);
    let mut request = [0u8; 220];
    request[216..220].copy_from_slice(&directory::volume().to_le_bytes());
    request[210..212].copy_from_slice(&106u16.to_le_bytes());
    assert_eq!(xinyi::syscall(81, request.as_ptr() as u32, 220, 0), -22);
    assert_eq!(
        directory::transfer_on(
            b"absent.txt",
            b"other.txt",
            1,
            directory::volume().wrapping_sub(1)
        ),
        -116
    );
    assert_eq!(xinyi::save_file(b"tx.txt", b"transfer"), 8);
    assert_eq!(directory::transfer(b"tx.txt", b"ty.txt", 0), 0);
    assert_eq!(directory::transfer(b"ty.txt", b"tz.txt", 1), 0);
    assert_eq!(directory::transfer(b"ty.txt", b"tz.txt", 1), -17);
    assert_eq!(directory::action(b"target", 0), 0);
    assert_eq!(directory::transfer(b"tz.txt", b"target/tz.txt", 2), 0);
    let mut data = [0; 1024];
    assert_eq!(xinyi::read_file(b"target/tz.txt", &mut data), 8);
    assert_eq!(&data[..8], b"transfer");
    assert_eq!(directory::transfer(b"target", b"renamed", 0), 0);
    assert_eq!(directory::cd(b"renamed"), 0);
    assert_eq!(directory::transfer(b"/renamed", b"/busy", 0), -16);
    assert_eq!(directory::cd(b"/"), 0);
    assert_eq!(directory::action(b"renamed/tz.txt", 1), 0);
    assert_eq!(directory::action(b"renamed", 2), 0);
    assert_eq!(directory::action(b"ty.txt", 1), 0);
    assert_eq!(xinyi::syscall(83,1,0x40000000,3468),-14);
    assert_eq!(xinyi::syscall(83,0,0x100000,3468),-14);
    assert_eq!(xinyi::syscall(83,0,0,3467),-22);
    let mut clip=xinyi::file_clipboard::Selection::new();clip.volume=directory::volume();clip.count=1;clip.paths[0][..15].copy_from_slice(b"/APPS/HELLO.BIN");clip.lengths[0]=15;
    let version=xinyi::file_clipboard::set(&clip);assert!(version>0);assert_eq!(xinyi::file_clipboard::get(&mut clip),0);assert_eq!(clip.count,1);clip.version=0;assert_eq!(xinyi::file_clipboard::update(&clip),-116);
    clip.count=0;assert!(xinyi::file_clipboard::set(&clip)>0);
    let mut theme=xinyi::appearance::Theme::DEFAULT;assert_eq!(xinyi::appearance::get(&mut theme),0);theme.title=0x1000000;assert_eq!(xinyi::appearance::apply(&theme,false),-22);assert_eq!(xinyi::syscall(84,0,0x40000000,8),-14);
    xinyi::print("Directory API checks passed!\n");
    0
}
