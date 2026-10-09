#![no_std]
#![no_main]

mod keyboard;
mod vga;

use core::panic::PanicInfo;

const INPUT_LIMIT: usize = 128;
const HISTORY_LIMIT: usize = 8;

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

    let mut history = [[0u8; INPUT_LIMIT]; HISTORY_LIMIT];
    let mut history_lengths = [0usize; HISTORY_LIMIT];
    let mut history_count = 0usize;
    let mut history_position = 0usize;

    print!("xinyi> ");

    loop {
        if let Some(key) = keyboard::poll_key() {
            match key {
                keyboard::Key::ArrowUp => {
                    if history_position > 0 {
                        history_position -= 1;

                        // Remove the current line from the display.
                        while length > 0 {
                            vga::backspace();
                            length -= 1;
                        }

                        input.fill(0);

                        // Restore the selected command.
                        length = history_lengths[history_position];

                        input[..length].copy_from_slice(
                            &history[history_position][..length],
                        );

                        if let Ok(command) =
                            core::str::from_utf8(&input[..length])
                        {
                            print!("{}", command);
                        }
                    }
                }

                keyboard::Key::ArrowDown => {
                    if history_position < history_count {
                        history_position += 1;

                        // Clear the currently displayed command.
                        while length > 0 {
                            vga::backspace();
                            length -= 1;
                        }

                        input.fill(0);

                        // A position after the newest command is blank.
                        if history_position < history_count {
                            length = history_lengths[history_position];

                            input[..length].copy_from_slice(
                                &history[history_position][..length],
                            );

                            if let Ok(command) =
                                core::str::from_utf8(&input[..length])
                            {
                                print!("{}", command);
                            }
                        }
                    }
                }

                keyboard::Key::Char(character) => {
                    match character {
                        '\n' => {
                            println!();

                            let command =
                                core::str::from_utf8(&input[..length])
                                    .unwrap_or("");

                            // Save non-empty commands, avoiding consecutive
                            // duplicates.
                            if length > 0 {
                                let duplicate = history_count > 0
                                    && history_lengths[history_count - 1]
                                        == length
                                    && history[history_count - 1][..length]
                                        == input[..length];

                                if !duplicate {
                                    if history_count == HISTORY_LIMIT {
                                        // Discard the oldest command.
                                        for index in 1..HISTORY_LIMIT {
                                            history[index - 1] =
                                                history[index];
                                            history_lengths[index - 1] =
                                                history_lengths[index];
                                        }

                                        history_count -= 1;
                                    }

                                    history[history_count][..length]
                                        .copy_from_slice(&input[..length]);

                                    history_lengths[history_count] = length;
                                    history_count += 1;
                                }
                            }

                            run_command(command);

                            length = 0;
                            input.fill(0);
                            history_position = history_count;

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

                        ch if ch.is_ascii() && !ch.is_control() => {
                            if length < INPUT_LIMIT {
                                input[length] = ch as u8;
                                length += 1;
                                print!("{}", ch);
                            }
                        }

                        _ => {}
                    }
                }
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