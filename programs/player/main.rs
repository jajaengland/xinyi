#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;
use alloc::vec::Vec;
#[path = "../../src/wav.rs"]
mod wav;
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::{
    audio, file_dialog, files, gui, key,
    widgets::{Button, Rect},
    window::{self, Draw},
};
#[global_allocator]
static HEAP: xinyi::heap::Allocator = xinyi::heap::Allocator::new();
#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! {
    xinyi::exit(12)
}
const MAX_WAV: usize = 512 * 1024;
struct Loading {
    path: [u8; 105],
    length: usize,
    volume: u32,
    size: usize,
    read: usize,
}
struct Player {
    data: Vec<u8>,
    output: Vec<i16>,
    format: Option<wav::Format>,
    source: u8,
    handle: Option<u32>,
    position: u32,
    total: u32,
    volume: u32,
    queued: u32,
    name: [u8; 105],
    name_len: usize,
    message: &'static [u8],
    loading: Option<Loading>,
    dialog: Option<file_dialog::Dialog>,
    buttons: [Button; 6],
}
fn number(out: &mut [u8], mut value: u32) {
    for b in out.iter_mut().rev() {
        *b = b'0' + (value % 10) as u8;
        value /= 10;
    }
}
fn triangle(frame: u32, hz: u32) -> i32 {
    let p = (frame * hz) % 48000;
    (if p < 24000 {
        p as i32
    } else {
        48000 - p as i32
    }) * 2
        - 24000
}
impl Player {
    fn new() -> Self {
        let mut name = [0; 105];
        let label = b"Built-in test sound";
        name[..label.len()].copy_from_slice(label);
        let mut output = Vec::new();
        output.resize(audio::FRAMES * 2, 0);
        Self {
            data: Vec::new(),
            output,
            format: None,
            source: 1,
            handle: None,
            position: 0,
            total: 192000,
            volume: 70,
            queued: 0,
            name,
            name_len: label.len(),
            message: b"T plays a test sound. Ctrl+O opens a PCM WAV (up to 512 KiB).",
            loading: None,
            dialog: None,
            buttons: [
                Button::new(Rect::new(8, 48, 96, 28)),
                Button::new(Rect::new(112, 48, 136, 28)),
                Button::new(Rect::new(256, 48, 96, 28)),
                Button::new(Rect::new(360, 48, 96, 28)),
                Button::new(Rect::new(496, 48, 48, 28)),
                Button::new(Rect::new(552, 48, 48, 28)),
            ],
        }
    }
    fn stop(&mut self) {
        if let Some(h) = self.handle.take() {
            audio::close(h);
        }
        self.queued = 0;
        self.loading = None;
        self.message = b"Stopped. Space plays from the beginning.";
    }
    fn start(&mut self) {
        self.stop();
        if self.source == 0 {
            self.message = b"Choose a WAV or test sound first.";
            return;
        }
        let h = audio::open();
        if h < 0 {
            self.message = if h == -16 {
                b"Another application is using audio."
            } else if h == -13 {
                b"Audio permission denied for this launch."
            } else {
                b"Audio unavailable. Launch QEMU with an AC97 sound device."
            };
            return;
        }
        self.handle = Some(h as u32);
        audio::volume(h as u32, self.volume);
        self.position = 0;
        self.queued = 0;
        self.message = b"Playing. S stops; +/- changes volume.";
    }
    fn tone(&mut self) {
        self.stop();
        self.source = 1;
        self.format = None;
        self.total = 192000;
        let label = b"Built-in test sound";
        self.name[..label.len()].copy_from_slice(label);
        self.name_len = label.len();
        self.start();
    }
    fn load(&mut self, path: [u8; 105], length: usize, volume: u32) {
        self.stop();
        self.source = 0;
        self.format = None;
        let size = files::stream_size_on(&path[..length], volume);
        if size <= 0 || size as usize > MAX_WAV {
            self.message =
                b"Cannot open WAV: missing, empty, drive changed or larger than 512 KiB.";
            return;
        }
        self.data.clear();
        if self.data.try_reserve(size as usize).is_err() {
            self.message = b"Not enough memory for WAV.";
            return;
        }
        self.data.resize(size as usize, 0);
        self.name = path;
        self.name_len = length;
        self.loading = Some(Loading {
            path,
            length,
            volume,
            size: size as usize,
            read: 0,
        });
        self.message = b"Loading WAV... S cancels. Other apps keep running.";
    }
    fn update(&mut self) {
        if let Some(load) = self.loading.as_mut() {
            let count = (load.size - load.read).min(files::MAX_CHUNK);
            let n = files::read_at_on(
                &load.path[..load.length],
                load.read as u32,
                &mut self.data[load.read..load.read + count],
                load.volume,
            );
            if n != count as i32 {
                self.loading = None;
                self.message = b"Read failed or drive changed. No audio was started.";
                return;
            }
            load.read += count;
            if load.read < load.size {
                return;
            }
            self.loading = None;
            match wav::parse(self.data.len() as u32, |offset, out| {
                let p = offset as usize;
                if p + out.len() > self.data.len() {
                    return false;
                }
                out.copy_from_slice(&self.data[p..p + out.len()]);
                true
            }) {
                Ok(format) => {
                    self.format = Some(format);
                    self.source = 2;
                    self.total = ((format.bytes as u64 / format.align as u64 * 48000
                        + format.rate as u64
                        - 1)
                        / format.rate as u64) as u32;
                    self.start();
                }
                Err(_) => {
                    self.message =
                        b"Unsupported WAV. Use uncompressed 8/16-bit mono/stereo, 8-48 kHz.";
                    return;
                }
            }
        }
        let Some(h) = self.handle else {
            return;
        };
        for _ in 0..4 {
            if self.position == self.total {
                break;
            }
            let n = (self.total - self.position).min(audio::FRAMES as u32) as usize;
            for i in 0..n {
                let at = self.position + i as u32;
                let (l, r) = if self.source == 1 {
                    let hz = [262, 330, 392, 523][(at / 48000).min(3) as usize];
                    let fade = at.min(self.total - at).min(2400) as i32;
                    let s =
                        ((triangle(at, hz) + triangle(at, hz * 3 / 2)) / 6 * fade / 2400) as i16;
                    (s, s)
                } else {
                    let f = self.format.unwrap();
                    let source = (at as u64 * f.rate as u64 / 48000)
                        .min((f.bytes / f.align as u32 - 1) as u64)
                        as usize;
                    let p = f.data as usize + source * f.align as usize;
                    let frame = &self.data[p..p + f.align as usize];
                    (wav::sample(f, frame, 0), wav::sample(f, frame, 1))
                };
                self.output[i * 2] = l;
                self.output[i * 2 + 1] = r;
            }
            let result = audio::write(h, &self.output[..n * 2]);
            if result == -11 {
                break;
            }
            if result != n as i32 {
                self.stop();
                self.message = b"Audio playback failed. Try opening the stream again.";
                return;
            }
            self.position += n as u32;
        }
        let queued = audio::queued(h);
        if queued < 0 {
            self.stop();
            self.message = b"Audio device error. Playback stopped.";
            return;
        }
        self.queued = queued as u32;
        if self.position == self.total && queued == 0 {
            self.stop();
            self.message = b"Finished. Space plays again.";
        }
    }
    fn paint(&self, w: u32) {
        window::draw(
            w,
            &[
                Draw::rect(0, 0, 640, 400, 0xeeeeee),
                Draw::text(16, 16, 0, b"Audio player - sampled sound"),
                Draw::text(
                    16,
                    104,
                    0,
                    b"48,000 Hz stereo output | PCM WAV or built-in test sound",
                ),
            ],
        );
        for (b, label) in self.buttons.iter().zip([
            b"Open WAV" as &[u8],
            b"Test sound",
            b"Play",
            b"Stop",
            b"-",
            b"+",
        ]) {
            gui::button(w, b, label, false);
        }
        window::row(w, 16, 136, 0xeeeeee, 0, &self.name[..self.name_len.min(76)]);
        let progress = if let Some(l) = self.loading.as_ref() {
            l.read as u32 * 592 / l.size as u32
        } else if self.total > 0 {
            self.position.saturating_sub(self.queued).min(self.total) * 592 / self.total
        } else {
            0
        };
        window::draw(
            w,
            &[
                Draw::rect(16, 176, 592, 24, 0x999999),
                Draw::rect(16, 176, progress, 24, 0x247b96),
            ],
        );
        let mut status = *b"Volume: 000%    Buffered: 00000 frames";
        number(&mut status[8..11], self.volume);
        number(&mut status[26..31], self.queued);
        window::row(w, 16, 224, 0xeeeeee, 0, &status);
        window::row(
            w,
            16,
            272,
            0xeeeeee,
            0,
            b"Ctrl+O: open | T: test sound | Space: play | S: stop",
        );
        window::row(
            w,
            16,
            304,
            0xeeeeee,
            0,
            b"PCM device is shared exclusively; closing releases it.",
        );
        window::row(w, 16, 368, 0xeeeeee, 0, self.message);
        if let Some(d) = self.dialog.as_ref() {
            d.paint(w);
        }
        window::present(w);
    }
    fn action(&mut self, index: usize) {
        match index {
            0 => {
                self.stop();
                self.dialog = Some(file_dialog::Dialog::new(false));
            }
            1 => self.tone(),
            2 => self.start(),
            3 => self.stop(),
            _ => {
                self.volume = if index == 4 {
                    self.volume.saturating_sub(10)
                } else {
                    (self.volume + 10).min(100)
                };
                if let Some(h) = self.handle {
                    audio::volume(h, self.volume);
                }
            }
        }
    }
}
fn main() -> u32 {
    let w = window::open(b"Audio player");
    if w < 0 {
        return 1;
    }
    let w = w as u32;
    let mut player = Player::new();
    let mut args = [0; 128];
    let n = xinyi::arguments(&mut args);
    if n > 0 && n <= 105 {
        let mut path = [0; 105];
        path[..n as usize].copy_from_slice(&args[..n as usize]);
        player.load(path, n as usize, xinyi::directory::volume());
    }
    xinyi::drag_drop::register_open();
    loop {
        player.update();
        player.paint(w);
        let active = player.handle.is_some() || player.loading.is_some();
        let mut e = window::Event::default();
        let result = window::event(w, &mut e, !active);
        if result < 0 || e.kind == window::CLOSE {
            player.stop();
            return 0;
        }
        if e.kind == 0 {
            xinyi::sleep_ms(10);
            continue;
        }
        if e.kind==window::DROP {
            if player.dialog.is_some() {xinyi::drag_drop::finish();player.message=b"Close the file dialog before dropping a WAV";}
            else {match xinyi::drag_drop::open_file(e) {Ok((path,len,volume))=>{if path[..len].ends_with(b".WAV")||path[..len].ends_with(b".wav") {player.load(path,len,volume);}else{player.message=b"Drop one WAV audio file";}},Err(_)=>player.message=b"Drop one WAV file from the current drive"}}
            continue;
        }
        if let Some(d) = player.dialog.as_mut() {
            match d.event(e) {
                file_dialog::Result::Accept { path, len, volume } => {
                    player.dialog = None;
                    player.load(path, len, volume);
                }
                file_dialog::Result::Cancel => player.dialog = None,
                _ => (),
            }
            continue;
        }
        if e.kind == window::RESET {
            for b in &mut player.buttons {
                b.reset(e.buttons);
            }
            continue;
        }
        if e.kind == window::POINTER {
            for i in 0..player.buttons.len() {
                if player.buttons[i].pointer(e.x, e.y, e.buttons) {
                    player.action(i);
                }
            }
        }
        if e.kind == window::KEY {
            let code = e.code as u16;
            if e.code & key::CTRL != 0 && matches!(code, 79 | 111) {
                player.action(0);
            } else {
                match code {
                    27 => {
                        player.stop();
                        return 0;
                    }
                    84 | 116 => player.action(1),
                    32 => player.action(2),
                    83 | 115 => player.action(3),
                    45 => player.action(4),
                    43 | 61 => player.action(5),
                    _ => (),
                }
            }
        }
    }
}
