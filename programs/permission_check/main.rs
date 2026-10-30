#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::{directory, files};
fn main() -> u32 {
    let mut args = [0; 128];
    let n = xinyi::arguments(&mut args);
    assert!(n > 0);
    let args = &args[..n as usize];
    let expected = match args {
        b"ro" => 1,
        b"none" => 0,
        b"trusted" => 63,
        _ => return 2,
    };
    assert_eq!(xinyi::syscall(95, 0, 0, 0), expected);
    assert_eq!(xinyi::syscall(95, 31, 0, 0), -22); // Query cannot grant permissions.
    assert_eq!(xinyi::syscall(95, 0, 0, 0), expected);
    if expected != 63 {
        // Malformed pointers prove denial precedes parsing, queuing and allocation.
        for number in [
            58, 67, 68, 71, 72, 73, 74, 75, 76, 77, 80, 81, 83, 89, 92, 94, 96, 101,
        ] {
            assert_eq!(
                xinyi::syscall(number, 0xffff_ffff, 0xffff_ffff, 0xffff_ffff),
                -13
            );
        }
        for op in [1, 2] {
            assert_eq!(xinyi::syscall(84, op, 0, 8), -13);
        }
        // Real requests try destructive operations through every supported file ABI.
        assert_eq!(xinyi::save_file(b"/KEEP.TXT", b"bad"), -13);
        assert_eq!(files::save_on(b"/KEEP.TXT", b"bad", 1), -13);
        assert_eq!(files::write_at_on(b"/KEEP.TXT", 0, b"bad", 1), -13);
        assert_eq!(files::resize_on(b"/KEEP.TXT", 0, 1), -13);
        assert_eq!(directory::action(b"/KEEP.TXT", 1), -13);
        assert_eq!(directory::transfer(b"/KEEP.TXT", b"/MOVED.TXT", 0), -13);
        assert_eq!(xinyi::launch(b"terminal"), -13);
    } else {
        assert_eq!(files::save(b"/ALLOW.TXT", b"trusted write"), 13);
    }
    if expected == 0 {
        for number in [57, 78, 79, 82, 88, 90, 91, 93] {
            assert_eq!(xinyi::syscall(number, 0, 0, 0), -13);
        }
    } else {
        let volume = directory::volume();
        let mut small = [0; 1024];
        assert_eq!(xinyi::read_file(b"/KEEP.TXT", &mut small), 9);
        assert_eq!(&small[..9], b"immutable");
        assert_eq!(files::read_on(b"/KEEP.TXT", &mut small, volume), 9);
        assert_eq!(files::stream_size_on(b"/KEEP.TXT", volume), 9);
        assert_eq!(
            files::read_at_on(b"/KEEP.TXT", 2, &mut small[..4], volume),
            4
        );
        assert_eq!(&small[..4], b"muta");
        let mut listing = directory::Listing::new();
        assert_eq!(directory::list(b"/", 0, &mut listing), 0);
        assert!(listing.count > 0);
        assert_eq!(directory::cd(b"/"), 0);
    }
    if expected == 1 {
        xinyi::print("Read-only permission checks passed!\n");
    } else if expected == 0 {
        xinyi::print("Isolated permission checks passed!\n");
    } else {
        xinyi::print("Trusted permission checks passed!\n");
    }
    0
}
