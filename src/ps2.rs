// IRQ1 and IRQ12 share the 8042 output FIFO. Always route by AUX status,
// never by which IRQ happened to run first. Also used after firmware calls.
use crate::bios::{input, output};
pub fn drain() {
    for _ in 0..32 {
        unsafe {
            let status = input(0x64);
            if status & 1 == 0 {
                break;
            }
            let byte = input(0x60);
            if status & 0xc0 != 0 {
                if status & 0x20 != 0 {
                    crate::mouse::bad_byte();
                } else {
                    crate::keyboard::bad_byte();
                }
                continue;
            }
            if status & 0x20 != 0 {
                crate::mouse::receive(byte);
            } else {
                crate::keyboard::push_scancode(byte);
            }
        }
    }
}
pub unsafe fn ready() -> bool {
    for _ in 0..100_000 {
        if input(0x64) & 2 == 0 {
            return true;
        }
        output(0x80, 0);
    }
    false
}
pub unsafe fn command(byte: u8) -> bool {
    if !ready() {
        return false;
    }
    output(0x64, byte);
    true
}
pub unsafe fn data(byte: u8) -> bool {
    if !ready() {
        return false;
    }
    output(0x60, byte);
    true
}
pub unsafe fn response(aux: bool) -> Option<u8> {
    for _ in 0..100_000 {
        let status = input(0x64);
        if status & 1 != 0 {
            let byte = input(0x60);
            if status & 0xc0 != 0 {
                continue;
            }
            if (status & 0x20 != 0) == aux {
                return Some(byte);
            }
            if status & 0x20 == 0 {
                crate::keyboard::push_scancode(byte);
            }
        } else {
            output(0x80, 0);
        }
    }
    None
}
