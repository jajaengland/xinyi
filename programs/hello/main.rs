#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;

fn main() -> u32 {
    xinyi::print("Hello from Rust on Xinyi!\n");
    if xinyi::getpid() == 0 {
        return 1;
    }
    0
}
