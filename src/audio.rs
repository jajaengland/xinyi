//! Intel 82801AA AC'97 PCM output. Fixed 48 kHz, stereo, signed little-endian i16.
//! DMA uses only permanently owned kernel buffers. No recording or device IRQs.
use crate::{
    audio_model::{Ring, BYTES, FRAMES, SLOTS},
    loader::Frames,
};
use core::arch::asm;
unsafe fn in8(p: u16) -> u8 {
    let v;
    asm!("in al,dx",in("dx")p,out("al")v,options(nomem,nostack,preserves_flags));
    v
}
unsafe fn in16(p: u16) -> u16 {
    let v;
    asm!("in ax,dx",in("dx")p,out("ax")v,options(nomem,nostack,preserves_flags));
    v
}
unsafe fn in32(p: u16) -> u32 {
    let v;
    asm!("in eax,dx",in("dx")p,out("eax")v,options(nomem,nostack,preserves_flags));
    v
}
unsafe fn out8(p: u16, v: u8) {
    asm!("out dx,al",in("dx")p,in("al")v,options(nomem,nostack,preserves_flags));
}
unsafe fn out16(p: u16, v: u16) {
    asm!("out dx,ax",in("dx")p,in("ax")v,options(nomem,nostack,preserves_flags));
}
unsafe fn out32(p: u16, v: u32) {
    asm!("out dx,eax",in("dx")p,in("eax")v,options(nomem,nostack,preserves_flags));
}
struct Guard(u32);
impl Guard {
    fn new() -> Self {
        let flags;
        unsafe {
            asm!("pushfd","pop {0:e}","cli",out(reg)flags);
        }
        Self(flags)
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if self.0 & 0x200 != 0 {
            unsafe {
                asm!("sti", options(nostack));
            }
        }
    }
}
struct Device {
    nam: u16,
    bm: u16,
    buffer: Frames,
    ring: Ring,
    owner: u32,
    handle: u32,
    next: u32,
    started: bool,
    fault: bool,
    volume: u32,
    voice: Option<Voice>,
}
struct Voice {
    data: &'static [u8],
    position: usize,
}
static mut SOUNDS: crate::sound_model::Policy = crate::sound_model::Policy::new();
static mut DEVICE: Option<Device> = None;
impl Device {
    fn reset(&mut self) -> bool {
        unsafe {
            out8(self.bm + 0x1b, 0);
            out8(self.bm + 0x1b, 2);
            for _ in 0..100_000 {
                if in8(self.bm + 0x1b) & 2 == 0 {
                    break;
                }
                core::hint::spin_loop();
            }
            if in8(self.bm + 0x1b) & 2 != 0 {
                return false;
            }
            out32(self.bm + 0x10, self.buffer.base());
            out16(self.bm + 0x16, 0x1c);
        }
        self.ring = Ring::new();
        self.started = false;
        self.fault = false;
        self.voice = None;
        true
    }
    fn volume(&mut self, value: u32) {
        self.volume = value;
        let attenuation = 31 - (value * 31 / 100) as u16;
        unsafe {
            out16(
                self.nam + 2,
                if value == 0 {
                    0x8000
                } else {
                    attenuation | (attenuation << 8)
                },
            );
            out16(self.nam + 0x18, 0);
        }
    }
    fn service(&mut self) {
        if !self.started || self.fault || self.ring.count == 0 {
            return;
        }
        unsafe {
            // A stable CIV/PICB pair avoids reclaiming a descriptor on a torn hardware snapshot.
            for _ in 0..3 {
                let state = in32(self.bm + 0x14);
                let civ = (state & 31) as usize;
                let remaining = in16(self.bm + 0x18);
                if in8(self.bm + 0x14) as usize != civ {
                    continue;
                }
                let status = (state >> 16) as u16;
                if status & 16 != 0 || !self.ring.retire(civ, status & 1 != 0 && remaining == 0) {
                    out8(self.bm + 0x1b, 0);
                    self.fault = true;
                    return;
                }
                out16(self.bm + 0x16, status & 0x1c);
                return;
            }
        }
    }
    fn owns(&self, pid: u32, handle: u32) -> bool {
        self.owner == pid && pid != 0 && self.handle == handle
    }
    fn publish(&mut self, frames: usize) {
        let index = self.ring.tail;
        let address = self.buffer.base() + 4096 + (index * BYTES) as u32;
        unsafe {
            let desc = (self.buffer.base() + index as u32 * 8) as *mut u32;
            desc.write_volatile(address);
            desc.add(1).write_volatile(frames as u32 * 2);
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::Release);
            self.ring.push(frames as u16).unwrap();
            out8(self.bm + 0x15, index as u8);
            if !self.started {
                out8(self.bm + 0x1b, 1);
                self.started = true;
            }
        }
    }
    fn voice_tick(&mut self) {
        let Some(voice) = self.voice.as_ref() else {
            return;
        };
        if self.fault || voice.position == voice.data.len() / 2 * 3 && self.ring.count == 0 {
            self.reset();
            crate::debug::write_str("Sound: finished\n");
            return;
        }
        // At most two descriptors per tick; no allocations or filesystem access in IRQ0.
        for _ in 0..2 {
            if !self.ring.available() {
                break;
            }
            let voice = self.voice.as_mut().unwrap();
            let count = voice.data.len() / 2;
            let frames = (count * 3 - voice.position).min(FRAMES);
            if frames == 0 {
                break;
            }
            let address = self.buffer.base() + 4096 + (self.ring.tail * BYTES) as u32;
            for frame in 0..frames {
                let pos = voice.position + frame;
                let source = pos / 3;
                let a =
                    i16::from_le_bytes([voice.data[source * 2], voice.data[source * 2 + 1]]) as i32;
                let next = (source + 1).min(count - 1);
                let b = i16::from_le_bytes([voice.data[next * 2], voice.data[next * 2 + 1]]) as i32;
                let value = (a + (b - a) * (pos % 3) as i32 / 3) as i16;
                unsafe {
                    let out = (address as *mut i16).add(frame * 2);
                    out.write(value);
                    out.add(1).write(value);
                }
            }
            voice.position += frames;
            self.publish(frames);
        }
    }
}
pub fn init() {
    let _guard = Guard::new();
    if unsafe { (*core::ptr::addr_of!(DEVICE)).is_some() } {
        return;
    }
    let mut found = None;
    for bus in 0u16..256 {
        for slot in 0..32 {
            if crate::pci::read(bus as u8, slot, 0, 0) as u16 == 0xffff {
                continue;
            }
            let functions = if crate::pci::read(bus as u8, slot, 0, 0x0c) & 0x00800000 != 0 {
                8
            } else {
                1
            };
            for function in 0..functions {
                if crate::pci::read(bus as u8, slot, function, 0) == 0x24158086
                    && crate::pci::read(bus as u8, slot, function, 8) >> 16 == 0x0401
                {
                    found = Some((bus as u8, slot, function));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let Some((bus, slot, function)) = found else {
        crate::debug::write_str("Audio: no supported AC97 device\n");
        return;
    };
    let a = crate::pci::read(bus, slot, function, 0x10);
    let b = crate::pci::read(bus, slot, function, 0x14);
    if a & 1 == 0 || b & 1 == 0 || a & !3 == 0 || b & !3 == 0 || a & !3 > 0xff00 || b & !3 > 0xffc0
    {
        return;
    }
    let nam = (a & !3) as u16;
    let bm = (b & !3) as u16;
    let command = crate::pci::read(bus, slot, function, 4) as u16;
    crate::pci::write_command(bus, slot, function, (command & !4) | 1);
    unsafe {
        out8(bm + 0x0b, 0);
        out8(bm + 0x1b, 0);
        out8(bm + 0x2b, 0);
        out32(bm + 0x2c, 2);
        let mut ready = false;
        for _ in 0..100_000 {
            if in32(bm + 0x30) & 0x100 != 0 {
                ready = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !ready {
            return;
        }
        out16(nam, 0);
        out16(nam + 0x2a, 0);
        out16(nam + 0x2c, 48000);
        if in16(nam + 0x2c) != 48000 {
            return;
        }
    }
    let Some(buffer) = Frames::new(1 + SLOTS * BYTES / 4096) else {
        return;
    };
    let mut d = Device {
        nam,
        bm,
        buffer,
        ring: Ring::new(),
        owner: 0,
        handle: 0,
        next: 1,
        started: false,
        fault: false,
        volume: 70,
        voice: None,
    };
    if !d.reset() {
        return;
    }
    d.volume(70);
    crate::pci::write_command(bus, slot, function, command | 5);
    crate::debug::write_fmt(format_args!(
        "Audio: AC97 PCM ready nam={:04x} bm={:04x} dma={:08x}\n",
        nam,
        bm,
        d.buffer.base()
    ));
    unsafe {
        core::ptr::addr_of_mut!(DEVICE).write(Some(d));
    }
}
pub fn tick() {
    unsafe {
        if let Some(d) = (&mut *core::ptr::addr_of_mut!(DEVICE)).as_mut() {
            d.service();
            d.voice_tick();
        }
    }
}
pub fn system_sound(event: crate::system_sounds::Event) {
    let _g = Guard::new();
    unsafe {
        let Some(d) = (&mut *core::ptr::addr_of_mut!(DEVICE)).as_mut() else {
            return;
        };
        let policy = &mut *core::ptr::addr_of_mut!(SOUNDS);
        if !policy.accept(
            crate::interrupts::timer_ticks(),
            d.owner != 0 || d.voice.is_some(),
        ) {
            return;
        }
        if !d.reset() {
            return;
        }
        d.volume(policy.volume);
        d.voice = Some(Voice {
            data: crate::system_sounds::clip(event),
            position: 0,
        });
        d.voice_tick();
        crate::debug::write_fmt(format_args!("Sound: play {}\n", event.name()));
    }
}
pub fn system_volume(value: Option<u32>) -> u32 {
    let _g = Guard::new();
    unsafe {
        let policy = &mut *core::ptr::addr_of_mut!(SOUNDS);
        if let Some(n) = value.filter(|n| *n <= 100) {
            policy.volume = n;
            if let Some(d) = (&mut *core::ptr::addr_of_mut!(DEVICE)).as_mut() {
                if d.voice.is_some() {
                    if n == 0 {
                        d.reset();
                    } else {
                        d.volume(n);
                    }
                }
            }
        }
        policy.volume
    }
}
/// A shutdown clip replaces app audio, then drains for a bounded time with IRQs enabled.
pub fn farewell() {
    shutdown();
    // The last event may have just occurred. Shutdown deliberately bypasses its cooldown.
    {
        let _g = Guard::new();
        unsafe {
            let volume = (*core::ptr::addr_of!(SOUNDS)).volume;
            if volume == 0 {
                return;
            }
            let Some(d) = (&mut *core::ptr::addr_of_mut!(DEVICE)).as_mut() else {
                return;
            };
            if !d.reset() {
                return;
            }
            d.volume(volume);
            d.voice = Some(Voice {
                data: crate::system_sounds::clip(crate::system_sounds::Event::Shutdown),
                position: 0,
            });
            d.voice_tick();
            crate::debug::write_str("Sound: play shutdown\n");
        }
    }
    let flags: u32;
    unsafe {
        asm!("pushfd","pop {0:e}","sti",out(reg)flags);
    }
    let start = crate::interrupts::timer_ticks();
    for _ in 0..50_000_000 {
        let done = {
            let _g = Guard::new();
            unsafe {
                (&*core::ptr::addr_of!(DEVICE))
                    .as_ref()
                    .is_none_or(|d| d.voice.is_none())
            }
        };
        if done || crate::interrupts::timer_ticks().wrapping_sub(start) > 150 {
            break;
        }
        core::hint::spin_loop();
    }
    shutdown();
    if flags & 0x200 == 0 {
        unsafe {
            asm!("cli", options(nomem, nostack));
        }
    }
}
pub fn close_owner(pid: u32) {
    let _g = Guard::new();
    unsafe {
        if let Some(d) = (&mut *core::ptr::addr_of_mut!(DEVICE)).as_mut() {
            if d.owner == pid && pid != 0 {
                d.reset();
                d.owner = 0;
                d.handle = 0;
                crate::debug::write_fmt(format_args!("Audio: owner {} closed\n", pid));
            }
        }
    }
}
pub fn shutdown() {
    let _g = Guard::new();
    unsafe {
        if let Some(d) = (&mut *core::ptr::addr_of_mut!(DEVICE)).as_mut() {
            out8(d.bm + 0x1b, 0);
            d.owner = 0;
            d.handle = 0;
            d.voice = None;
        }
    }
}
pub fn syscall(cs: u32, number: u32, a: u32, b: u32, c: u32) -> u32 {
    let Some(image) = crate::usermode::active_image(cs) else {
        return u32::MAX;
    };
    let _guard = Guard::new();
    let pid = crate::usermode::getpid();
    if number == 96 {
        if a | b | c != 0 {
            return (-22i32) as u32;
        }
        return if unsafe { (*core::ptr::addr_of!(DEVICE)).is_some() } {
            48000
        } else {
            0
        };
    }
    if number == 97 && a | b | c != 0 {
        return (-22i32) as u32;
    }
    if number == 98 {
        if c == 0 || c > FRAMES as u32 {
            return (-22i32) as u32;
        }
        if !crate::user_memory::readable_range(b, c * 4, image) {
            return (-14i32) as u32;
        }
    }
    if number == 99 && (b > 2 || b != 2 && c != 0 || b == 2 && c > 100) {
        return (-22i32) as u32;
    }
    let Some(d) = (unsafe { (&mut *core::ptr::addr_of_mut!(DEVICE)).as_mut() }) else {
        return (-19i32) as u32;
    };
    d.service();
    if number == 97 {
        if d.owner != 0 {
            return (-16i32) as u32;
        }
        if d.next > i32::MAX as u32 {
            return (-24i32) as u32;
        }
        if !d.reset() {
            return (-5i32) as u32;
        }
        d.volume(70);
        d.owner = pid;
        d.handle = d.next;
        d.next += 1;
        crate::debug::write_fmt(format_args!(
            "Audio: open pid={} stream={}\n",
            pid, d.handle
        ));
        return d.handle;
    }
    if !d.owns(pid, a) {
        return (-9i32) as u32;
    }
    if number == 99 {
        return match b {
            0 => {
                crate::debug::write_fmt(format_args!("Audio: closed pid={} stream={}\n", pid, a));
                d.reset();
                d.owner = 0;
                d.handle = 0;
                0
            }
            1 => {
                if d.fault {
                    (-5i32) as u32
                } else {
                    d.ring.frames
                }
            }
            _ => {
                d.volume(c);
                0
            }
        };
    }
    if d.fault {
        return (-5i32) as u32;
    }
    if !d.ring.available() {
        return (-11i32) as u32;
    }
    let index = d.ring.tail;
    let address = d.buffer.base() + 4096 + (index * BYTES) as u32;
    unsafe {
        core::ptr::copy_nonoverlapping(b as *const u8, address as *mut u8, c as usize * 4);
    }
    d.publish(c as usize);
    crate::debug::write_fmt(format_args!(
        "Audio: queued pid={} frames={} pending={}\n",
        pid, c, d.ring.frames
    ));
    c
}
