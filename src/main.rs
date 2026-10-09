#![no_std]
#![no_main]

mod keyboard;
mod vga;

use core::panic::PanicInfo;

const INPUT_LIMIT: usize = 128;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::vga::_print(core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::vga::_print(core::format_args!("\n"))
    };

    ($($arg:tt)*) => {{
        $crate::vga::_print(core::format_args!($($arg)*));
        $crate::vga::_print(core::format_args!("\n"));
    }};
}

fn run_command(input: &str) {
    let input = input.trim();

    if input.is_empty() {
        return;
    }

    let (command, argument) = match input.split_once(' ') {
        Some((command, argument)) => (command, argument.trim()),
        None => (input, ""),
    };

    match command {
        "help" => {
            println!("Xinyi commands:");
            println!("  help       - Show available commands");
            println!("  about      - About Xinyi");
            println!("  clear      - Clear the screen");
            println!("  echo TEXT  - Print text");
            println!("  version    - Show version");
            println!("  uname      - Show system information");
            println!("  reboot     - Reboot the machine");
        }

        "about" => {
            println!("Xinyi");
            println!("A small 32-bit operating system kernel");
            println!("Written in Rust and x86 assembly.");
        }

        "clear" => {
            vga::clear_screen();
        }

        "echo" => {
            println!("{}", argument);
        }

        "version" => {
            println!("Xinyi kernel version 0.1.0");
        }

        "uname" => {
            println!("Xinyi x86 32-bit");
            println!("Architecture: i686");
            println!("Environment: bare metal / QEMU");
        }

        "reboot" => {
            println!("Rebooting...");

            unsafe {
                // Ask the legacy keyboard controller to reset the CPU.
                for _ in 0..100_000 {
                    let status: u8;

                    core::arch::asm!(
                        "in al, dx",
                        in("dx") 0x64u16,
                        out("al") status,
                        options(nomem, nostack, preserves_flags)
                    );

                    if status & 0x02 == 0 {
                        core::arch::asm!(
                            "out dx, al",
                            in("dx") 0x64u16,
                            in("al") 0xFEu8,
                            options(nomem, nostack, preserves_flags)
                        );

                        break;
                    }
                }
            }

            println!("Reset was not confirmed.");
        }

        _ => {
            println!("Unknown command: {}", command);
            println!("Type 'help' to see available commands.");
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    vga::clear_screen();

    println!("Welcome to Xinyi.");
    println!("32-bit Rust kernel");
    println!("Type 'help' to see commands.");
    println!();

    let mut input = [0u8; INPUT_LIMIT];
    let mut length = 0usize;

    print!("xinyi> ");

    loop {
        if let Some(character) = keyboard::poll_char() {
            match character {
                '\n' => {
                    println!();

                    let command = core::str::from_utf8(&input[..length])
                        .unwrap_or("");

                    run_command(command);

                    length = 0;
                    input.fill(0);

                    println!();
                    print!("xinyi> ");
                }

                '\u{8}' => {
                    if length > 0 {
                        length -= 1;
                        input[length] = 0;
                        vga::backspace();
                    }
                }

                '\t' => {
                    for _ in 0..4 {
                        if length < INPUT_LIMIT {
                            input[length] = b' ';
                            length += 1;
                            print!(" ");
                        }
                    }
                }

                character if character.is_ascii() && !character.is_control() => {
                    if length < INPUT_LIMIT {
                        input[length] = character as u8;
                        length += 1;
                        print!("{}", character);
                    }
                }

                _ => {}
            }
        } else {
            core::hint::spin_loop();
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            core::arch::asm!("cli; hlt", options(nomem, nostack));
        }
    }
}