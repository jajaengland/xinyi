
#[derive(Clone, Copy, PartialEq, Eq)]
struct RtcSnapshot {
    second: u8,
    minute: u8,
    hour: u8,
    day: u8,
    month: u8,
    year: u8,
}

pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

// Read a CMOS RTC register using x86 I/O ports.
unsafe fn read_register(register: u8) -> u8 {
    use core::arch::asm;

    asm!(
        "out dx, al",
        in("dx") 0x70u16,
        in("al") register,
        options(nomem, nostack, preserves_flags)
    );

    let value: u8;

    asm!(
        "in al, dx",
        in("dx") 0x71u16,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );

    value
}

fn wait_for_update() {
    // Wait until the RTC finishes updating its registers.
    while unsafe { read_register(0x0A) } & 0x80 != 0 {
        core::hint::spin_loop();
    }
}

fn read_snapshot() -> RtcSnapshot {
    wait_for_update();

    unsafe {
        RtcSnapshot {
            second: read_register(0x00),
            minute: read_register(0x02),
            hour: read_register(0x04),
            day: read_register(0x07),
            month: read_register(0x08),
            year: read_register(0x09),
        }
    }
}

fn from_bcd(value: u8) -> u8 {
    (value & 0x0F) + ((value >> 4) * 10)
}

pub fn read() -> DateTime {
    // Read twice to avoid using values from different clock updates.
    let snapshot = loop {
        let first = read_snapshot();
        let second = read_snapshot();

        if first == second {
            break second;
        }
    };

    let status_b = unsafe { read_register(0x0B) };
    let binary_mode = status_b & 0x04 != 0;
    let twenty_four_hour = status_b & 0x02 != 0;

    let convert = |value: u8| {
        if binary_mode {
            value
        } else {
            from_bcd(value)
        }
    };

    let raw_hour = snapshot.hour;
    let is_pm = raw_hour & 0x80 != 0;
    let mut hour = convert(raw_hour & 0x7F);

    if !twenty_four_hour {
        hour %= 12;

        if is_pm {
            hour += 12;
        }
    }

    DateTime {
        year: 2000 + convert(snapshot.year) as u16,
        month: convert(snapshot.month),
        day: convert(snapshot.day),
        hour,
        minute: convert(snapshot.minute),
        second: convert(snapshot.second),
    }
}
