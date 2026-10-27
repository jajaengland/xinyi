// Bounded PS/2 initialization; IRQ code only decodes/enqueues, never draws.
pub use crate::mouse_packet::Event;
use crate::{
    bios::{input, output},
    mouse_packet::Decoder,
    ps2,
};
use core::{
    arch::asm,
    sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
};
const SIZE: usize = 128;
static ENABLED: AtomicBool = AtomicBool::new(false);
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
static LOST: AtomicBool = AtomicBool::new(false);
static BUTTONS: AtomicU8 = AtomicU8::new(0);
static mut QUEUE: [Event; SIZE] = [Event {
    dx: 0,
    dy: 0,
    buttons: 0,
    lost: false,
}; SIZE];
static mut DECODER: Decoder = Decoder::new();
static mut EVENT_ORDER:[u32;SIZE]=[0;SIZE];
static mut LAST_BYTE: u32 = 0;
pub fn bad_byte() {
    unsafe {
        (*core::ptr::addr_of_mut!(DECODER)).reset();
    }
    LOST.store(true, Ordering::Release);
}
pub fn receive(byte: u8) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        let now = crate::interrupts::timer_ticks();
        let decoder = &mut *core::ptr::addr_of_mut!(DECODER);
        if now.wrapping_sub(LAST_BYTE) > 10 {
            decoder.reset();
        }
        LAST_BYTE = now;
        if let Some(event) = decoder.feed(byte) {
            BUTTONS.store(event.buttons, Ordering::Relaxed);
            let head = HEAD.load(Ordering::Relaxed);
            let next = (head + 1) % SIZE;
            if next == TAIL.load(Ordering::Acquire) {
                LOST.store(true, Ordering::Release);
                return;
            }
            core::ptr::addr_of_mut!(QUEUE)
                .cast::<Event>()
                .add(head)
                .write(event);
            core::ptr::addr_of_mut!(EVENT_ORDER).cast::<u32>().add(head).write(crate::input_order::next());
            HEAD.store(next, Ordering::Release);
        }
    }
}
pub fn lost()->bool {LOST.load(Ordering::Acquire)}
pub fn next_order()->Option<u32> {
    let tail=TAIL.load(Ordering::Relaxed);
    if tail==HEAD.load(Ordering::Acquire) {None}else{Some(unsafe {core::ptr::addr_of!(EVENT_ORDER).cast::<u32>().add(tail).read()})}
}
// Only motion with unchanged buttons may be combined; never cross a queued
// keyboard byte, button transition or loss/reset boundary.
pub fn motion_before(buttons:u8,keyboard:Option<u32>)->bool {
    if lost() {return false;}
    let tail=TAIL.load(Ordering::Relaxed);
    if tail==HEAD.load(Ordering::Acquire) {return false;}
    let order=unsafe {core::ptr::addr_of!(EVENT_ORDER).cast::<u32>().add(tail).read()};
    if keyboard.is_some_and(|key|!crate::input_order::before(order,key)) {return false;}
    let event=unsafe {core::ptr::addr_of!(QUEUE).cast::<Event>().add(tail).read()};
    !event.lost&&event.buttons==buttons
}
pub fn pending() -> bool {
    LOST.load(Ordering::Acquire) || HEAD.load(Ordering::Acquire) != TAIL.load(Ordering::Acquire)
}
pub fn poll() -> Option<Event> {
    // IRQs are disabled briefly so overflow recovery can discard old transitions
    // and snapshot the latest buttons without racing the producer.
    let flags: u32;
    unsafe {
        asm!("pushfd","pop {0}","cli",out(reg)flags);
    }
    let event = if LOST.swap(false, Ordering::AcqRel) {
        TAIL.store(HEAD.load(Ordering::Acquire), Ordering::Release);
        Some(Event {
            buttons: BUTTONS.load(Ordering::Relaxed),
            lost: true,
            ..Event::default()
        })
    } else {
        let tail = TAIL.load(Ordering::Relaxed);
        if tail == HEAD.load(Ordering::Acquire) {
            None
        } else {
            let event = unsafe { core::ptr::addr_of!(QUEUE).cast::<Event>().add(tail).read() };
            TAIL.store((tail + 1) % SIZE, Ordering::Release);
            Some(event)
        }
    };
    if flags & 0x200 != 0 {
        unsafe {
            asm!("sti", options(nostack));
        }
    }
    event
}
pub fn discard() -> u8 {
    let flags: u32;
    unsafe {
        asm!("pushfd","pop {0}","cli",out(reg)flags);
    }
    TAIL.store(HEAD.load(Ordering::Acquire), Ordering::Release);
    LOST.store(false, Ordering::Release);
    let buttons = BUTTONS.load(Ordering::Acquire);
    if flags & 0x200 != 0 {
        unsafe {
            asm!("sti", options(nostack));
        }
    }
    buttons
}
unsafe fn device_command(byte: u8) -> bool {
    for _ in 0..3 {
        if !ps2::command(0xd4) || !ps2::data(byte) {
            return false;
        }
        match ps2::response(true) {
            Some(0xfa) => return true,
            Some(0xfe) => continue,
            _ => return false,
        }
    }
    false
}
pub fn init() -> bool {
    if ENABLED.load(Ordering::Acquire) {
        return true;
    }
    let flags: u32;
    unsafe {
        asm!("pushfd","pop {0}","cli",out(reg)flags);
    }
    let mut config = None;
    let ok = unsafe {
        (|| {
            if !ps2::command(0xad) || !ps2::command(0xa7) {
                return false;
            }
            ps2::drain();
            if !ps2::command(0x20) {
                return false;
            }
            let Some(value) = ps2::response(false) else {
                return false;
            };
            config = Some(value);
            if !ps2::command(0xa8) || !device_command(0xf5) || !device_command(0xf6) {
                return false;
            }
            // Preserve translation and unrelated controller settings; enable both
            // clocks and AUX IRQ. Keyboard IRQ remains in its original state.
            if !ps2::command(0x60) || !ps2::data((value | 2) & !0x30) {
                return false;
            }
            if !device_command(0xf4) {
                return false;
            }
            (*core::ptr::addr_of_mut!(DECODER)).reset();
            ENABLED.store(true, Ordering::Release);
            // Unmask slave IRQ12 and the master IRQ2 cascade, preserving other IRQs.
            output(0xa1, input(0xa1) & !0x10);
            output(0x21, input(0x21) & !4);
            true
        })()
    };
    unsafe {
        if !ok {
            let _ = ps2::command(0xa7);
            if let Some(value) = config {
                if ps2::command(0x60) {
                    let _ = ps2::data((value | 0x20) & !0x12);
                }
            }
        }
        let _ = ps2::command(0xae);
        ps2::drain();
        if flags & 0x200 != 0 {
            asm!("sti", options(nostack));
        }
    }
    crate::debug::write_str(if ok {
        "Mouse: PS/2 enabled\n"
    } else {
        "Mouse: PS/2 unavailable\n"
    });
    ok
}
