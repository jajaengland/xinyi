//! RTL8139 legacy I/O driver. Device IRQs stay masked; DMA owns five kernel pages.
//! Register reference: Realtek legacy interface, also implemented by QEMU hw/net/rtl8139.c.
use crate::{loader::Frames, net_model::FRAME};
use core::{
    arch::asm,
    ptr,
    sync::atomic::{compiler_fence, Ordering},
};
unsafe fn in8(p: u16) -> u8 {
    let v;
    asm!("in al,dx",in("dx")p,out("al")v,options(nomem,nostack));
    v
}
unsafe fn in16(p: u16) -> u16 {
    let v;
    asm!("in ax,dx",in("dx")p,out("ax")v,options(nomem,nostack));
    v
}
unsafe fn in32(p: u16) -> u32 {
    let v;
    asm!("in eax,dx",in("dx")p,out("eax")v,options(nomem,nostack));
    v
}
unsafe fn out8(p: u16, v: u8) {
    asm!("out dx,al",in("dx")p,in("al")v,options(nomem,nostack));
}
unsafe fn out16(p: u16, v: u16) {
    asm!("out dx,ax",in("dx")p,in("ax")v,options(nomem,nostack));
}
unsafe fn out32(p: u16, v: u32) {
    asm!("out dx,eax",in("dx")p,in("eax")v,options(nomem,nostack));
}
pub struct Device {
    io: u16,
    pci: (u8, u8, u8),
    dma: Frames,
    rx: usize,
    tx: usize,
    fault: bool,
    pub mac: [u8; 6],
    pub received: u32,
    pub transmitted: u32,
    pub errors: u32,
}
impl Device {
    pub fn init() -> Option<Self> {
        let mut found = None;
        for bus in 0u16..256 {
            for slot in 0..32 {
                if crate::pci::read(bus as u8, slot, 0, 0) as u16 == 0xffff {
                    continue;
                }
                let count = if crate::pci::read(bus as u8, slot, 0, 0x0c) & 0x800000 != 0 {
                    8
                } else {
                    1
                };
                for function in 0..count {
                    if crate::pci::read(bus as u8, slot, function, 0) != 0x813910ec
                        || crate::pci::read(bus as u8, slot, function, 8) >> 16 != 0x0200
                    {
                        continue;
                    }
                    let bar = crate::pci::read(bus as u8, slot, function, 0x10);
                    let io = bar & !3;
                    if bar & 1 != 0 && io != 0 && io <= 0xff00 && io & 0xff == 0 {
                        found = Some((bus as u8, slot, function, io as u16));
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
        let (bus, slot, function, io) = found?;
        let dma = Frames::new(5)?;
        let command = crate::pci::read(bus, slot, function, 4) as u16;
        crate::pci::write_command(bus, slot, function, command | 1);
        unsafe {
            out8(io + 0x50, 0xc0);
            out8(io + 0x52, 0);
            out8(io + 0x50, 0);
            out16(io + 0x3c, 0);
            out8(io + 0x37, 0x10);
            let mut ready = false;
            for _ in 0..100_000 {
                if in8(io + 0x37) & 0x10 == 0 {
                    ready = true;
                    break;
                }
                core::hint::spin_loop();
            }
            if !ready {
                crate::pci::write_command(bus, slot, function, command & !4);
                return None;
            }
            let mut mac = [0; 6];
            for (i, b) in mac.iter_mut().enumerate() {
                *b = in8(io + i as u16);
            }
            if mac == [0; 6] || mac[0] & 1 != 0 {
                crate::pci::write_command(bus, slot, function, command & !4);
                return None;
            }
            out32(io + 0x30, dma.base());
            for i in 0..4 {
                out32(io + 0x20 + i * 4, dma.base() + 12288 + i as u32 * 2048);
            }
            out16(io + 0x3c, 0);
            out16(io + 0x3e, 0xffff);
            crate::pci::write_command(bus, slot, function, command | 5);
            out8(io + 0x37, 0x0c);
            out32(io + 0x44, 0x0000e78a); // 8 KiB, no wrap, max DMA/FIFO; own unicast + broadcast.
            out32(io + 0x40, 0x03000700); // Standard IFG, 2048-byte DMA burst.
            out16(io + 0x38, 0xfff0);
            crate::debug::write_fmt(format_args!(
                "Net: RTL8139 ready {:02x}:{:02x}.{} io={:04x} dma={:08x}\n",
                bus,
                slot,
                function,
                io,
                dma.base()
            ));
            Some(Self {
                io,
                pci: (bus, slot, function),
                dma,
                rx: 0,
                tx: 0,
                fault: false,
                mac,
                received: 0,
                transmitted: 0,
                errors: 0,
            })
        }
    }
    pub fn link(&self) -> bool {
        !self.fault && unsafe { in8(self.io + 0x58) & 4 == 0 }
    }
    pub fn transmit(&mut self, frame: &[u8]) -> bool {
        if frame.len() < 14 || frame.len() > FRAME || !self.link() {
            return false;
        }
        unsafe {
            let status = in32(self.io + 0x10 + self.tx as u16 * 4);
            if status & 0x2000 == 0 {
                return false;
            }
            if status & 0xc0004000 != 0 {
                self.errors = self.errors.saturating_add(1);
                out32(self.io + 0x40, 0x03000701);
            }
            let pointer = (self.dma.base() + 12288 + self.tx as u32 * 2048) as *mut u8;
            let size = frame.len().max(60);
            for i in 0..size {
                ptr::write_volatile(pointer.add(i), frame.get(i).copied().unwrap_or(0));
            }
            compiler_fence(Ordering::Release);
            out32(self.io + 0x10 + self.tx as u16 * 4, size as u32);
            self.tx = (self.tx + 1) % 4;
            self.transmitted = self.transmitted.saturating_add(1);
            true
        }
    }
    pub fn receive(&mut self, buffer: &mut [u8; FRAME]) -> Option<usize> {
        unsafe {
            let status = in16(self.io + 0x3e);
            out16(self.io + 0x3e, status);
            if status & 0x0050 != 0 {
                self.recover();
                return None;
            }
            if in8(self.io + 0x37) & 1 != 0 {
                return None;
            }
            compiler_fence(Ordering::Acquire);
            let start = (self.dma.base() as *const u8).add(self.rx);
            let flags =
                u16::from_le_bytes([ptr::read_volatile(start), ptr::read_volatile(start.add(1))]);
            let size = u16::from_le_bytes([
                ptr::read_volatile(start.add(2)),
                ptr::read_volatile(start.add(3)),
            ]) as usize;
            if size == 0xfff0 {
                return None;
            } // Hardware is still writing this frame.
            if !(18..=1522).contains(&size) || flags & 0x3f != 1 {
                self.recover();
                return None;
            }
            let length = size - 4;
            for (i, b) in buffer[..length].iter_mut().enumerate() {
                *b = ptr::read_volatile(start.add(4 + i));
            }
            self.rx = ((self.rx + size + 7) & !3) % 8192;
            out16(self.io + 0x38, (self.rx as u16).wrapping_sub(16));
            self.received = self.received.saturating_add(1);
            Some(length)
        }
    }
    unsafe fn recover(&mut self) {
        self.errors = self.errors.saturating_add(1);
        out8(self.io + 0x37, 0x10);
        let mut ready = false;
        for _ in 0..100_000 {
            if in8(self.io + 0x37) & 0x10 == 0 {
                ready = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !ready {
            self.fault = true;
            out8(self.io + 0x37, 0);
            return;
        }
        out16(self.io + 0x3c, 0);
        out32(self.io + 0x30, self.dma.base());
        self.rx = 0;
        self.tx = 0;
        for i in 0..4 {
            out32(
                self.io + 0x20 + i * 4,
                self.dma.base() + 12288 + i as u32 * 2048,
            );
        }
        out8(self.io + 0x37, 0x0c);
        out32(self.io + 0x44, 0x0000e78a);
        out32(self.io + 0x40, 0x03000700);
        out16(self.io + 0x38, 0xfff0);
        out16(self.io + 0x3e, 0xffff);
    }
    pub fn stop(&mut self) {
        let (bus, slot, function) = self.pci;
        let command = crate::pci::read(bus, slot, function, 4) as u16;
        crate::pci::write_command(bus, slot, function, command & !4);
        unsafe {
            out16(self.io + 0x3c, 0);
            out8(self.io + 0x37, 0);
        }
    }
}
// A published Device lives until halt. DMA backing is never borrowed from an app.

/// Quiesce supported adapters at boot without allocating DMA or starting a stack.
pub fn disable_all() {
    for bus in 0u16..256 {
        for slot in 0..32 {
            if crate::pci::read(bus as u8, slot, 0, 0) as u16 == 0xffff {
                continue;
            }
            let functions = if crate::pci::read(bus as u8, slot, 0, 0x0c) & 0x800000 != 0 {
                8
            } else {
                1
            };
            for function in 0..functions {
                if crate::pci::read(bus as u8, slot, function, 0) != 0x813910ec {
                    continue;
                }
                let command = crate::pci::read(bus as u8, slot, function, 4) as u16;
                crate::pci::write_command(bus as u8, slot, function, command & !4);
                let bar = crate::pci::read(bus as u8, slot, function, 0x10);
                let io = bar & !3;
                if bar & 1 != 0 && io != 0 && io <= 0xff00 && io & 255 == 0 {
                    crate::pci::write_command(bus as u8, slot, function, (command | 1) & !4);
                    unsafe {
                        out16(io as u16 + 0x3c, 0);
                        out8(io as u16 + 0x37, 0);
                    }
                }
                crate::pci::write_command(bus as u8, slot, function, command & !5);
            }
        }
    }
}
