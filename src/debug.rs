// Minimal diagnostic output through QEMU's debug console (I/O port 0xE9).
// Writes are intentionally cheap so this can be used while investigating
// hardware behaviour. It is not a replacement for the VGA console.

use core::arch::asm;

pub fn write_byte(byte: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") 0xE9u16,
            in("al") byte,
            options(nomem, nostack, preserves_flags)
        );
    }
}

pub fn write_str(text: &str) {
    for byte in text.bytes() {
        write_byte(byte);
    }
}

/// Write formatted diagnostic text without requiring a heap.
pub fn write_fmt(args: core::fmt::Arguments) {
    use core::fmt::Write;

    struct DebugWriter;

    impl Write for DebugWriter {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            write_str(text);
            Ok(())
        }
    }

    let _ = DebugWriter.write_fmt(args);
}

pub fn write_hex8(value: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    write_byte(HEX[(value >> 4) as usize]);
    write_byte(HEX[(value & 0x0F) as usize]);
}

pub fn write_u32(mut value: u32) {
    let mut buffer = [b'0'; 10];
    let mut index = buffer.len();

    if value == 0 {
        write_byte(b'0');
        return;
    }

    while value > 0 {
        index -= 1;
        buffer[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }

    for byte in &buffer[index..] {
        write_byte(*byte);
    }
}

pub fn write_hex32(value: u32) {
    for shift in (0..32).step_by(8).rev() {
        write_hex8((value >> shift) as u8);
    }
}
