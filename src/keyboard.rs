const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;

const LEFT_SHIFT: u8 = 0x2A;
const RIGHT_SHIFT: u8 = 0x36;
const CAPS_LOCK: u8 = 0x3A;

static mut SHIFT_DOWN: bool = false;
static mut CAPS_LOCK_ON: bool = false;
static mut EXTENDED: bool = false;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;

    core::arch::asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );

    value
}

fn read_scancode() -> Option<u8> {
    unsafe {
        if inb(STATUS_PORT) & 1 == 0 {
            return None;
        }

        Some(inb(DATA_PORT))
    }
}

pub fn poll_key() -> Option<Key> {
    let code = read_scancode()?;

    unsafe {
        // Extended PS/2 keys start with 0xE0.
        if code == 0xE0 {
            EXTENDED = true;
            return None;
        }

        let extended = EXTENDED;
        EXTENDED = false;

        // Handle extended arrow keys.
        if extended {
            // Ignore extended key-release events.
            if code & 0x80 != 0 {
                return None;
            }

            return match code {
                0x48 => Some(Key::ArrowUp),
                0x50 => Some(Key::ArrowDown),
                0x4B => Some(Key::ArrowLeft),
                0x4D => Some(Key::ArrowRight),
                _ => None,
            };
        }

        if code == LEFT_SHIFT || code == RIGHT_SHIFT {
            SHIFT_DOWN = true;
            return None;
        }

        if code == (LEFT_SHIFT | 0x80)
            || code == (RIGHT_SHIFT | 0x80)
        {
            SHIFT_DOWN = false;
            return None;
        }

        // Ignore ordinary key-release events.
        if code & 0x80 != 0 {
            return None;
        }

        if code == CAPS_LOCK {
            CAPS_LOCK_ON = !CAPS_LOCK_ON;
            return None;
        }

        let shift = SHIFT_DOWN;
        let caps = CAPS_LOCK_ON;

        let ch = match code {
            0x02 => if shift { '!' } else { '1' },
            0x03 => if shift { '@' } else { '2' },
            0x04 => if shift { '#' } else { '3' },
            0x05 => if shift { '$' } else { '4' },
            0x06 => if shift { '%' } else { '5' },
            0x07 => if shift { '^' } else { '6' },
            0x08 => if shift { '&' } else { '7' },
            0x09 => if shift { '*' } else { '8' },
            0x0A => if shift { '(' } else { '9' },
            0x0B => if shift { ')' } else { '0' },
            0x0C => if shift { '_' } else { '-' },
            0x0D => if shift { '+' } else { '=' },

            0x0E => '\u{8}',
            0x0F => '\t',
            0x1C => '\n',
            0x39 => ' ',

            0x10 => letter('q', shift, caps),
            0x11 => letter('w', shift, caps),
            0x12 => letter('e', shift, caps),
            0x13 => letter('r', shift, caps),
            0x14 => letter('t', shift, caps),
            0x15 => letter('y', shift, caps),
            0x16 => letter('u', shift, caps),
            0x17 => letter('i', shift, caps),
            0x18 => letter('o', shift, caps),
            0x19 => letter('p', shift, caps),

            0x1E => letter('a', shift, caps),
            0x1F => letter('s', shift, caps),
            0x20 => letter('d', shift, caps),
            0x21 => letter('f', shift, caps),
            0x22 => letter('g', shift, caps),
            0x23 => letter('h', shift, caps),
            0x24 => letter('j', shift, caps),
            0x25 => letter('k', shift, caps),
            0x26 => letter('l', shift, caps),

            0x2C => letter('z', shift, caps),
            0x2D => letter('x', shift, caps),
            0x2E => letter('c', shift, caps),
            0x2F => letter('v', shift, caps),
            0x30 => letter('b', shift, caps),
            0x31 => letter('n', shift, caps),
            0x32 => letter('m', shift, caps),

            0x27 => if shift { ':' } else { ';' },
            0x28 => if shift { '"' } else { '\'' },
            0x29 => if shift { '~' } else { '`' },
            0x2B => if shift { '|' } else { '\\' },
            0x33 => if shift { '<' } else { ',' },
            0x34 => if shift { '>' } else { '.' },
            0x35 => if shift { '?' } else { '/' },

            _ => return None,
        };

        Some(Key::Char(ch))
    }
}

// Keep the existing character-only interface available.
pub fn poll_char() -> Option<char> {
    match poll_key()? {
        Key::Char(ch) => Some(ch),
        Key::ArrowUp
        | Key::ArrowDown
        | Key::ArrowLeft
        | Key::ArrowRight => None,
    }
}

fn letter(lower: char, shift: bool, caps: bool) -> char {
    if shift ^ caps {
        lower.to_ascii_uppercase()
    } else {
        lower
    }
}