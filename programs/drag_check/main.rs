#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
fn main() -> u32 {
    if xinyi::permissions::current() != 63 {
        for op in 0..2 {
            assert_eq!(xinyi::syscall(103, op, 0, 0), -13);
        }
        xinyi::print("Restricted drag API checks passed!\n");
        return 0;
    }
    let w = xinyi::window::open_sized(b"Drag API check", 160, 80);
    assert!(w > 0);
    assert_eq!(xinyi::drag_drop::register(), 0);
    assert_eq!(xinyi::syscall(103, 0, 1, 0), -22);
    assert_eq!(xinyi::syscall(103, 1, 0, 219), -22);
    assert_eq!(xinyi::syscall(103, 1, 0, 3468), -16); // No mouse gesture owns capture.
    assert_eq!(xinyi::syscall(103, 2, 1, 3468), -14);
    let mut payload = xinyi::file_clipboard::Selection::new();
    assert_eq!(xinyi::drag_drop::take(&mut payload), -9);
    assert_eq!(xinyi::drag_drop::finish(), -9);
    0
}
