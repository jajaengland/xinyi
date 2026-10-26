#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
fn main() -> u32 {
    xinyi::getpid()
}
