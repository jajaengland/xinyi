#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::{
    file_dialog, files, key,
    window::{self, Draw},
};
fn hex(mut value: u32, out: &mut [u8]) {
    for b in out.iter_mut().rev() {
        *b = b"0123456789ABCDEF"[(value & 15) as usize];
        value >>= 4;
    }
}
fn main() -> u32 {
    let w = window::open(b"File viewer");
    if w < 0 {
        return 1;
    }
    let w = w as u32;
    let mut path = [0; 105];
    let mut len = 0;
    let mut volume = xinyi::directory::volume();
    let mut size = 0u32;
    let mut offset = 0u32;
    let mut data = [0; 256];
    let mut count = 0;
    let mut message: &[u8] = b"Choose a file with Ctrl+O. Viewer never writes file data.";
    let mut dialog = Some(file_dialog::Dialog::new(false));
    let mut args = [0; 128];
    let n = xinyi::arguments(&mut args);
    if n > 0 && n <= 105 {
        path[..n as usize].copy_from_slice(&args[..n as usize]);
        len = n as usize;
        dialog = None;
    }
    let mut load = true;
    loop {
        if load && len > 0 {
            let result = files::stream_size_on(&path[..len], volume);
            if result < 0 {
                count = 0;
                message = b"Cannot read file or drive changed. Ctrl+O opens another file.";
            } else {
                size = result as u32;
                offset = offset.min(size.saturating_sub(1) / 256 * 256);
                let result = files::read_at_on(&path[..len], offset, &mut data, volume);
                if result < 0 {
                    count = 0;
                    message = b"Read failed. File may have changed; Ctrl+O reopens it.";
                } else {
                    count = result as usize;
                    message = b"Read only | Arrows / PgUp / PgDn / Home / End | Ctrl+O open";
                }
            }
        }
        window::draw(w, &[Draw::rect(0, 0, 640, 400, 0xffffff)]);
        window::row(w, 8, 8, 0xffffff, 0, &path[..len.min(78)]);
        let mut status = *b"Size 00000000h bytes     Offset 00000000h";
        hex(size, &mut status[5..13]);
        hex(offset, &mut status[32..40]);
        window::row(w, 8, 32, 0xffffff, 0, &status);
        window::row(
            w,
            8,
            64,
            0xffffff,
            0x555555,
            b"OFFSET    HEXADECIMAL BYTES                                  ASCII",
        );
        for row in 0..16 {
            let mut line = [b' '; 76];
            let begin = row * 16;
            if begin < count {
                hex(offset + begin as u32, &mut line[..8]);
                for j in 0..16 {
                    if begin + j < count {
                        let b = data[begin + j];
                        hex(b as u32, &mut line[10 + j * 3..12 + j * 3]);
                        line[60 + j] = if (32..=126).contains(&b) { b } else { b'.' };
                    }
                }
            }
            window::row(w, 8, 88 + row as u32 * 16, 0xffffff, 0, &line);
        }
        window::row(w, 8, 376, 0xffffff, 0, message);
        if let Some(d) = dialog.as_ref() {
            d.paint(w);
        }
        window::present(w);
        load = false;
        let mut e = window::Event::default();
        if window::event(w, &mut e, true) < 0 || e.kind == window::CLOSE {
            return 0;
        }
        if let Some(d) = dialog.as_mut() {
            match d.event(e) {
                file_dialog::Result::Accept {
                    path: p,
                    len: n,
                    volume: v,
                } => {
                    path = p;
                    len = n;
                    volume = v;
                    offset = 0;
                    load = true;
                    dialog = None;
                }
                file_dialog::Result::Cancel => {
                    dialog = None;
                }
                file_dialog::Result::Pending => (),
            }
        } else if e.kind == window::KEY {
            let code = e.code as u16;
            if e.code & key::CTRL != 0 && matches!(code, 79 | 111) {
                dialog = Some(file_dialog::Dialog::new(false));
            } else {
                match code {
                    27 => return 0,
                    key::UP => {
                        offset = offset.saturating_sub(16);
                        load = true;
                    }
                    key::DOWN => {
                        offset = offset.saturating_add(16);
                        load = true;
                    }
                    key::PAGE_UP => {
                        offset = offset.saturating_sub(256);
                        load = true;
                    }
                    key::PAGE_DOWN => {
                        offset = offset.saturating_add(256);
                        load = true;
                    }
                    key::HOME => {
                        offset = 0;
                        load = true;
                    }
                    key::END => {
                        offset = size.saturating_sub(1) / 256 * 256;
                        load = true;
                    }
                    _ => (),
                }
            }
        }
    }
}
