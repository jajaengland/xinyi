#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::{directory, files};
fn pattern(offset: usize) -> u8 {
    ((offset * 17 + offset / 512) % 251) as u8
}
fn main() -> u32 {
    let volume = directory::volume();
    let path = b"/STREAM.BIN";
    let size = 98317;
    let mut args = [0; 128];
    let again = xinyi::arguments(&mut args) > 0;
    let mut data = [0u8; 4096];
    if !again {
        assert_eq!(files::resize_on(path, 0, volume), 0);
        // Chunk writes cross the old 64 KiB boundary without a whole-file user buffer.
        let mut offset = 0;
        while offset < size {
            let n = (size - offset).min(data.len());
            for i in 0..n {
                data[i] = pattern(offset + i);
            }
            assert_eq!(
                files::write_at_on(path, offset as u32, &data[..n], volume),
                n as i32
            );
            offset += n;
        }
    }
    assert_eq!(files::stream_size_on(path, volume), size as i32);
    assert_eq!(files::size_on(path, volume), -27); // Legacy ABI remains bounded.
    for offset in [0, 509, 65530, 70000, 98300] {
        data.fill(0xa5);
        let n = (size - offset).min(data.len());
        assert_eq!(
            files::read_at_on(path, offset as u32, &mut data, volume),
            n as i32
        );
        for i in 0..n {
            assert_eq!(data[i], pattern(offset + i));
        }
        assert!(data[n..].iter().all(|&b| b == 0xa5));
    }
    assert_eq!(files::read_at_on(path, size as u32, &mut data, volume), 0);
    assert_eq!(
        files::read_at_on(path, 0, &mut data, volume.wrapping_add(1)),
        -116
    );
    #[repr(C)]
    struct R {
        path: u32,
        path_len: u32,
        data: u32,
        len: u32,
        volume: u32,
        offset: u32,
    }
    let mut r = R {
        path: path.as_ptr() as u32,
        path_len: path.len() as u32,
        data: 1,
        len: 8,
        volume,
        offset: 0,
    };
    let call = |r: &R| xinyi::syscall(91, r as *const _ as u32, 24, 0);
    assert_eq!(call(&r), -14);
    r.data = data.as_mut_ptr() as u32;
    r.offset = u32::MAX;
    assert_eq!(call(&r), -22);
    r.offset = 0;
    r.len = 16385;
    assert_eq!(call(&r), -22);
    r.len = 8;
    r.path = 1;
    assert_eq!(call(&r), -14);
    assert_eq!(xinyi::syscall(91, 1, 24, 0), -14);
    assert_eq!(xinyi::syscall(91, 0, 20, 0), -22);
    assert_eq!(files::write_at_on(path, 1048576, b"x", volume), -22);
    assert_eq!(files::resize_on(path, 1048577, volume), -22);
    if !again {
        assert_eq!(files::write_at_on(b"/GAP.BIN", 1000, b"abc", volume), 3);
        assert_eq!(files::resize_on(b"/GAP.BIN", 1001, volume), 0);
        assert_eq!(files::resize_on(b"/GAP.BIN", 1500, volume), 0);
        assert_eq!(files::read_at_on(b"/GAP.BIN", 0, &mut data, volume), 1500);
        assert!(data[..1000].iter().all(|&b| b == 0));
        assert_eq!(data[1000], b'a');
        assert!(data[1001..1500].iter().all(|&b| b == 0));
    }
    xinyi::print("Streaming file checks passed!\n");
    0
}
