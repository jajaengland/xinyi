use core::sync::atomic::{AtomicUsize, Ordering};

const SCANCODE_BUFFER_SIZE: usize = 128;

const EXTENDED_PREFIX: u8 = 0xE0;
const LEFT_SHIFT: u8 = 0x2A;
const RIGHT_SHIFT: u8 = 0x36;
const CAPS_LOCK: u8 = 0x3A;

// Serialized IRQ1/IRQ12 FIFO routing, single-consumer (shell) ring buffer.
static SCANCODE_HEAD: AtomicUsize = AtomicUsize::new(0);
static SCANCODE_TAIL: AtomicUsize = AtomicUsize::new(0);
static DROPPED_SCANCODES: AtomicUsize = AtomicUsize::new(0);
static mut SCANCODE_BUFFER: [u8; SCANCODE_BUFFER_SIZE] =
    [0; SCANCODE_BUFFER_SIZE];

static mut SCANCODE_ORDER:[u32;SCANCODE_BUFFER_SIZE]=[0;SCANCODE_BUFFER_SIZE];

static mut LEFT_SHIFT_DOWN: bool = false;
static mut RIGHT_SHIFT_DOWN: bool = false;
static mut CAPS_LOCK_ON: bool = false;
static mut CAPS_LOCK_DOWN: bool = false;
static mut EXTENDED: bool = false;
static mut CTRL_DOWN: [bool; 2] = [false; 2];
static mut ALT_DOWN: [bool; 2] = [false; 2];
static mut ARROWS_DOWN: [bool;4] = [false;4]; // Up, Down, Left, Right.

// Consumer-side copy of the drop counter, used to notice ring-buffer
// overflow without adding work to normal scancode processing.
static mut OBSERVED_DROPS: usize = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Special(u16),
}

pub(crate) fn bad_byte() {DROPPED_SCANCODES.fetch_add(1,Ordering::Relaxed);}

/// Append one byte to the ring buffer. Called by the IRQ producer.
pub(crate) fn push_scancode(code: u8) {
    let head = SCANCODE_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % SCANCODE_BUFFER_SIZE;
    let tail = SCANCODE_TAIL.load(Ordering::Acquire);

    // Leave one slot empty to distinguish a full buffer from an empty one.
    if next == tail {
        // A dropped byte may have been a modifier release. Record the loss
        // so the consumer can discard its now-untrustworthy modifier state.
        DROPPED_SCANCODES.fetch_add(1, Ordering::Relaxed);
        return;
    }

    unsafe {
        core::ptr::addr_of_mut!(SCANCODE_BUFFER)
            .cast::<u8>()
            .add(head)
            .write(code);
    }

    unsafe {core::ptr::addr_of_mut!(SCANCODE_ORDER).cast::<u32>().add(head).write(crate::input_order::next());}
    SCANCODE_HEAD.store(next, Ordering::Release);
}

/// Called by IRQ1/IRQ12. Route shared FIFO bytes without printing or drawing.
///
/// The controller is drained completely rather than reading a single byte.
/// Its hardware queue is small, so leaving bytes behind while handling other
/// interrupts can make it overflow and silently lose key releases.
pub fn handle_irq() {crate::ps2::drain();}

/// Gets the next scancode collected by IRQ1.
fn read_scancode() -> Option<u8> {
    let tail = SCANCODE_TAIL.load(Ordering::Relaxed);
    let head = SCANCODE_HEAD.load(Ordering::Acquire);

    if tail == head {
        return None;
    }

    let code = unsafe {
        core::ptr::addr_of!(SCANCODE_BUFFER)
            .cast::<u8>()
            .add(tail)
            .read()
    };

    SCANCODE_TAIL.store(
        (tail + 1) % SCANCODE_BUFFER_SIZE,
        Ordering::Release,
    );

    Some(code)
}

/// Discard modifier state if the producer had to drop scancodes.
///
/// Without this, losing a release event can leave a Shift key logically held
/// until that same key is pressed and released again. Treating overflow as a
/// state reset is conservative: a key may appear unshifted, but it cannot
/// remain stuck.
fn resynchronize_after_drop() {
    let dropped = DROPPED_SCANCODES.load(Ordering::Acquire);

    unsafe {
        if dropped != OBSERVED_DROPS {
            OBSERVED_DROPS = dropped;
            LEFT_SHIFT_DOWN = false;
            RIGHT_SHIFT_DOWN = false;
            CAPS_LOCK_DOWN = false;
            EXTENDED = false;
            CTRL_DOWN = [false; 2];
            ALT_DOWN = [false; 2];
            ARROWS_DOWN = [false;4];
        }
    }
}

pub fn poll_key() -> Option<Key> {
    resynchronize_after_drop();

    let code = read_scancode()?;

    unsafe {
        // Extended PS/2 keys start with 0xE0.
        if code == EXTENDED_PREFIX {
            EXTENDED = true;
            return None;
        }

        let extended = EXTENDED;
        EXTENDED = false;

        // Track both left and right modifiers, including their release bytes.
        let pressed = code & 0x80 == 0;
        match code & 0x7f {
            0x1d => { CTRL_DOWN[extended as usize] = pressed; return None; }
            0x38 => { ALT_DOWN[extended as usize] = pressed; return None; }
            _ => (),
        }

        if extended {
            let arrow=match code&0x7f {0x48=>Some(0),0x50=>Some(1),0x4b=>Some(2),0x4d=>Some(3),_=>None};
            if let Some(index)=arrow {ARROWS_DOWN[index]=pressed;}
            // Ignore extended key-release events. The embedded Shift codes
            // used by Print Screen are also ignored here; they are fake
            // modifiers and do not represent the physical Shift keys.
            if code & 0x80 != 0 {
                return None;
            }

            return match code {
                0x48 => Some(Key::ArrowUp),
                0x50 => Some(Key::ArrowDown),
                0x4B => Some(Key::ArrowLeft),
                0x4D => Some(Key::ArrowRight),
                0x47 => Some(Key::Special(0x104)), // Home
                0x4f => Some(Key::Special(0x105)), // End
                0x53 => Some(Key::Special(0x106)), // Delete
                0x49 => Some(Key::Special(0x107)), // Page Up
                0x51 => Some(Key::Special(0x108)), // Page Down
                0x52 => Some(Key::Special(0x109)), // Insert
                _ => None,
            };
        }

        if code == LEFT_SHIFT {
            LEFT_SHIFT_DOWN = true;
            return None;
        }

        if code == RIGHT_SHIFT {
            RIGHT_SHIFT_DOWN = true;
            return None;
        }

        if code == (LEFT_SHIFT | 0x80) {
            LEFT_SHIFT_DOWN = false;
            return None;
        }

        if code == (RIGHT_SHIFT | 0x80) {
            RIGHT_SHIFT_DOWN = false;
            return None;
        }

        if code == (CAPS_LOCK | 0x80) {
            CAPS_LOCK_DOWN = false;
            return None;
        }

        // Ignore ordinary key-release events.
        if code & 0x80 != 0 {
            return None;
        }

        // Toggle only on the press edge. Holding Caps Lock generates repeated
        // make codes, which must not flip the state back and forth.
        if code == CAPS_LOCK {
            if !CAPS_LOCK_DOWN {
                CAPS_LOCK_DOWN = true;
                CAPS_LOCK_ON = !CAPS_LOCK_ON;
            }

            return None;
        }

        let shift = LEFT_SHIFT_DOWN || RIGHT_SHIFT_DOWN;
        let caps = CAPS_LOCK_ON;

        let ch = match code {
            0x01 => '\u{1b}', // Escape
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

            0x0E => '\u{8}', // Backspace
            0x0F => '\t',    // Tab
            0x1C => '\n',    // Enter
            0x39 => ' ',     // Space

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

            0x3b..=0x44 => return Some(Key::Special(0x120 + (code - 0x3b) as u16)),
            _ => return None,
        };

        Some(Key::Char(ch))
    }
}

/// Keep the character-only interface available.
pub fn poll_char() -> Option<char> {
    match poll_key()? {
        Key::Char(ch) => Some(ch),
        _ => None,
    }
}

/// One key press: code in bits 0..15, Shift/Ctrl/Alt/Caps in bits 16..19.
/// Modifier/release/prefix bytes are consumed but do not produce events.
pub fn poll_event() -> Option<u32> {
    let code = match poll_key()? {
        Key::Char(ch) => ch as u32,
        Key::ArrowUp => 0x100,
        Key::ArrowDown => 0x101,
        Key::ArrowLeft => 0x102,
        Key::ArrowRight => 0x103,
        Key::Special(code) => code as u32,
    };
    let modifiers = modifiers();
    Some(code | modifiers)
}

pub fn left_shift_down() -> bool {
    unsafe { LEFT_SHIFT_DOWN }
}
pub fn arrows_down()->[bool;4] {unsafe {*core::ptr::addr_of!(ARROWS_DOWN)}}
pub fn clear_arrows() {unsafe {*core::ptr::addr_of_mut!(ARROWS_DOWN)=[false;4];}}

pub fn right_shift_down() -> bool {
    unsafe { RIGHT_SHIFT_DOWN }
}

pub fn caps_lock_on() -> bool {
    unsafe { CAPS_LOCK_ON }
}

pub fn next_order()->Option<u32> {
    let tail=SCANCODE_TAIL.load(Ordering::Relaxed);
    if tail==SCANCODE_HEAD.load(Ordering::Acquire) {None}else{Some(unsafe {core::ptr::addr_of!(SCANCODE_ORDER).cast::<u32>().add(tail).read()})}
}

pub fn queued_scancodes() -> usize {
    let head = SCANCODE_HEAD.load(Ordering::Acquire);
    let tail = SCANCODE_TAIL.load(Ordering::Acquire);

    (head + SCANCODE_BUFFER_SIZE - tail) % SCANCODE_BUFFER_SIZE
}

pub fn dropped_scancodes() -> usize {
    DROPPED_SCANCODES.load(Ordering::Relaxed)
}

fn letter(lower: char, shift: bool, caps: bool) -> char {
    if shift ^ caps {
        lower.to_ascii_uppercase()
    } else {
        lower
    }
}

pub fn modifiers()->u32 {
    unsafe {
        ((LEFT_SHIFT_DOWN || RIGHT_SHIFT_DOWN) as u32) << 16
        | ((CTRL_DOWN[0] || CTRL_DOWN[1]) as u32) << 17
        | ((ALT_DOWN[0] || ALT_DOWN[1]) as u32) << 18
        | (CAPS_LOCK_ON as u32) << 19
    }
}
