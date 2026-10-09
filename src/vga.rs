use core::fmt;
use core::fmt::Write;
use core::ptr::{read_volatile, write_volatile};

const VGA_BUFFER: *mut u8 = 0xB8000 as *mut u8;

const VGA_CONTROL: u16 = 0x3D4;
const VGA_DATA: u16 = 0x3D5;

const WIDTH: usize = 80;
const HEIGHT: usize = 25;

const DEFAULT_COLOR: u8 = 0x0A;

pub struct Writer {
    column: usize,
    row: usize,
    color: u8,
}

impl Writer {
    pub const fn new() -> Self {
        Self {
            column: 0,
            row: 0,
            color: DEFAULT_COLOR,
        }
    }

    fn update_cursor(&self) {
        let position = (self.row * WIDTH + self.column) as u16;

        unsafe {
            // Select the VGA cursor's low position byte.
            core::arch::asm!(
                "out dx, al",
                in("dx") VGA_CONTROL,
                in("al") 0x0Fu8,
                options(nomem, nostack, preserves_flags)
            );

            // Write the low position byte.
            core::arch::asm!(
                "out dx, al",
                in("dx") VGA_DATA,
                in("al") position as u8,
                options(nomem, nostack, preserves_flags)
            );

            // Select the VGA cursor's high position byte.
            core::arch::asm!(
                "out dx, al",
                in("dx") VGA_CONTROL,
                in("al") 0x0Eu8,
                options(nomem, nostack, preserves_flags)
            );

            // Write the high position byte.
            core::arch::asm!(
                "out dx, al",
                in("dx") VGA_DATA,
                in("al") (position >> 8) as u8,
                options(nomem, nostack, preserves_flags)
            );
        }
    }

    fn put_cell(&mut self, column: usize, row: usize, character: u8) {
        let offset = (row * WIDTH + column) * 2;

        unsafe {
            write_volatile(VGA_BUFFER.add(offset), character);
            write_volatile(VGA_BUFFER.add(offset + 1), self.color);
        }
    }

    fn clear_row(&mut self, row: usize) {
        for column in 0..WIDTH {
            self.put_cell(column, row, b' ');
        }
    }

    pub fn clear_screen(&mut self) {
        for row in 0..HEIGHT {
            self.clear_row(row);
        }

        self.column = 0;
        self.row = 0;
        self.update_cursor();
    }

    pub fn set_color(&mut self, color: u8) {
        self.color = color;
    }

    fn scroll(&mut self) {
        for row in 1..HEIGHT {
            for column in 0..WIDTH {
                let source = (row * WIDTH + column) * 2;
                let destination = ((row - 1) * WIDTH + column) * 2;

                unsafe {
                    let character =
                        read_volatile(VGA_BUFFER.add(source));
                    let color =
                        read_volatile(VGA_BUFFER.add(source + 1));

                    write_volatile(VGA_BUFFER.add(destination), character);
                    write_volatile(VGA_BUFFER.add(destination + 1), color);
                }
            }
        }

        self.clear_row(HEIGHT - 1);
        self.row = HEIGHT - 1;
    }

    fn new_line(&mut self) {
        self.column = 0;
        self.row += 1;

        if self.row >= HEIGHT {
            self.scroll();
        }
    }

    fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),

            b'\r' => self.column = 0,

            b'\t' => {
                for _ in 0..4 {
                    self.write_byte(b' ');
                }
                return;
            }

            byte => {
                if self.column >= WIDTH {
                    self.new_line();
                }

                self.put_cell(self.column, self.row, byte);
                self.column += 1;

                if self.column >= WIDTH {
                    self.new_line();
                }
            }
        }

        self.update_cursor();
    }

    pub fn backspace(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.column = WIDTH - 1;
        } else {
            return;
        }

        self.put_cell(self.column, self.row, b' ');
        self.update_cursor();
    }
}

impl Write for Writer {
    fn write_str(&mut self, string: &str) -> fmt::Result {
        for byte in string.bytes() {
            self.write_byte(byte);
        }

        Ok(())
    }
}

static mut WRITER: Writer = Writer::new();

pub fn clear_screen() {
    unsafe {
        let writer = core::ptr::addr_of_mut!(WRITER);
        (*writer).clear_screen();
    }
}

pub fn set_color(color: u8) {
    unsafe {
        let writer = core::ptr::addr_of_mut!(WRITER);
        (*writer).set_color(color);
    }
}

pub fn backspace() {
    unsafe {
        let writer = core::ptr::addr_of_mut!(WRITER);
        (*writer).backspace();
    }
}

pub fn _print(arguments: fmt::Arguments) {
    unsafe {
        let writer = core::ptr::addr_of_mut!(WRITER);
        let _ = (*writer).write_fmt(arguments);
    }
}