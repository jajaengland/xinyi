#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;

fn main() -> u32 {
    xinyi::print("Counting inside a Rust program:\n");
    for number in 1u8..=10 {
        let line = [b'0' + number / 10, b'0' + number % 10, b'\n'];
        if xinyi::write(&line) != line.len() as i32 {
            return 1;
        }
    }
    xinyi::print("Done! Back to the shell.\n");
    0
}
