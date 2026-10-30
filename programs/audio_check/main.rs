#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::audio;
fn main() -> u32 {
    let mut args = [0; 128];
    let n = xinyi::arguments(&mut args).max(0) as usize;
    let mode = &args[..n];
    assert_eq!(xinyi::permissions::current(), 33);
    assert_eq!(xinyi::syscall(58, u32::MAX, 0, 0), -13);
    assert_eq!(xinyi::launch(b"terminal"), -13);
    assert_eq!(xinyi::sounds::set_volume(100), -13);
    assert_eq!(xinyi::sounds::volume(), -13);
    assert_eq!(xinyi::syscall(100,0,9,0),-22);
    assert_eq!(xinyi::syscall(100,0,4,1),-22);
    if mode == b"missing" {
        assert_eq!(audio::device_rate(), 0);
        assert_eq!(audio::open(), -19);
        xinyi::print("Missing audio checks passed!\n");
        return 0;
    }
    assert_eq!(audio::device_rate(), 48000);
    if mode == b"busy" {
        assert_eq!(audio::open(), -16);
        assert_eq!(audio::queued(1), -9);
        assert_eq!(audio::volume(1, 100), -9);
        assert_eq!(audio::close(1), -9);
        xinyi::print("Audio ownership checks passed!\n");
        return 0;
    }
    let h = audio::open();
    assert!(h > 0);
    let h = h as u32;
    assert_eq!(audio::volume(h, 100), 0);
    assert_eq!(audio::open(), -16);
    assert_eq!(xinyi::sounds::notification(),0); // Must not steal an active PCM stream.
    assert_eq!(audio::queued(h),0);
    assert_eq!(audio::queued(h + 1), -9);
    assert_eq!(xinyi::syscall(98, h, 1, 16), -14);
    assert_eq!(xinyi::syscall(98, h, 0, u32::MAX), -22);
    assert_eq!(xinyi::syscall(99, h, 2, 101), -22);
    let mut data = [0i16; 4096];
    let mut at = 0u32;
    while at < 120000 {
        let count = (120000 - at).min(2048) as usize;
        for i in 0..count {
            let v = if (at + i as u32) % 100 < 50 {
                12000
            } else {
                -12000
            };
            data[i * 2] = v;
            data[i * 2 + 1] = v;
        }
        let result = audio::write(h, &data[..count * 2]);
        if result == -11 {
            xinyi::sleep_ms(10);
            continue;
        }
        assert_eq!(result, count as i32);
        at += count as u32;
    }
    if mode == b"fault" {
        unsafe {
            core::ptr::write_volatile(0x40000000 as *mut u32, 0);
        }
        return 99;
    }
    while audio::queued(h) > 0 {
        xinyi::sleep_ms(10);
    }
    assert_eq!(audio::queued(h), 0);
    assert_eq!(audio::close(h), 0);
    assert_eq!(audio::queued(h), -9);
    let next = audio::open();
    assert!(next > h as i32);
    assert_eq!(audio::close(next as u32), 0);
    xinyi::print("PCM audio checks passed!\n");
    0
}
