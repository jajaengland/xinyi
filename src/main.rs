#![no_std]
#![no_main]

mod interrupts;
mod keyboard;
mod rtc;
mod vga;

use core::panic::PanicInfo;

const INPUT_LIMIT: usize = 128;
const HISTORY_LIMIT: usize = 8;
const PROMPT: &str = "xinyi> ";

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

// Position the cursor at a character index within the input line.
// VGA positions are represented as (column, row).
fn place_input_cursor(
    start_column: usize,
    start_row: usize,
    position: usize,
) {
    let absolute = start_row * 80 + start_column + position;
    let row = absolute / 80;
    let column = absolute % 80;

    vga::move_cursor(column, row);
}

// Keep the remembered prompt row aligned when VGA scrolling moves the screen.
fn sync_prompt_row(prompt_row: &mut usize, last_scroll_count: &mut usize) {
    let current = vga::scroll_count();
    let scrolled = current.wrapping_sub(*last_scroll_count);
    *prompt_row = prompt_row.saturating_sub(scrolled);
    *last_scroll_count = current;
}

// Redraw an edited command and remove any leftover characters.
fn redraw_input(
    input: &[u8; INPUT_LIMIT],
    length: usize,
    cursor_position: usize,
    old_length: usize,
    start_column: usize,
    start_row: &mut usize,
    last_scroll_count: &mut usize,
) {
    sync_prompt_row(start_row, last_scroll_count);
    vga::move_cursor(start_column, *start_row);

    if let Ok(text) = core::str::from_utf8(&input[..length]) {
        print!("{}", text);
    }

    // Erase characters left behind when the command becomes shorter.
    for _ in length..old_length {
        print!(" ");
    }

    // Printing the input may itself have scrolled the display, so update
    // the saved prompt row before placing the hardware cursor.
    sync_prompt_row(start_row, last_scroll_count);
    place_input_cursor(
        start_column,
        *start_row,
        cursor_position,
    );
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
            println!("  date       - Show the current date and time");
            println!("  uname      - Show system information");
            println!("  reboot     - Reboot the machine");
        }

        "about" => {
            println!("Xinyi");
            println!("A small 32-bit operating system kernel");
            println!("Written in Rust and x86 assembly.");
        }

        "breakpoint" => {
    	    println!("Triggering CPU breakpoint exception...");
    	    unsafe {
                core::arch::asm!("int3", options(nomem, nostack));
            }
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

        "date" => {
            let now = rtc::read();

            println!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                now.year,
                now.month,
                now.day,
                now.hour,
                now.minute,
                now.second
            );
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
            println!("Type 'help' for available commands.");
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    interrupts::init();
    vga::clear_screen();

    println!("Welcome to Xinyi.");
    println!("32-bit Rust kernel");
    println!("Type 'help' to see commands.");
    println!();

    let mut input = [0u8; INPUT_LIMIT];
    let mut length = 0usize;
    let mut cursor_position = 0usize;

    let mut history = [[0u8; INPUT_LIMIT]; HISTORY_LIMIT];
    let mut history_lengths = [0usize; HISTORY_LIMIT];
    let mut history_count = 0usize;
    let mut history_position = 0usize;

    print!("{}", PROMPT);
    // The current VGA cursor is immediately after the prompt, which is
    // where command input should begin.
    let (prompt_end_column, mut prompt_row) = vga::cursor_position();
    let mut prompt_column = prompt_end_column;
    let mut last_scroll_count = vga::scroll_count();

    loop {
        if let Some(key) = keyboard::poll_key() {
            match key {
                keyboard::Key::ArrowLeft => {
                    if cursor_position > 0 {
                        cursor_position -= 1;
                        sync_prompt_row(&mut prompt_row, &mut last_scroll_count);

                        place_input_cursor(
                            prompt_column,
                            prompt_row,
                            cursor_position,
                        );
                    }
                }

                keyboard::Key::ArrowRight => {
                    if cursor_position < length {
                        cursor_position += 1;
                        sync_prompt_row(&mut prompt_row, &mut last_scroll_count);

                        place_input_cursor(
                            prompt_column,
                            prompt_row,
                            cursor_position,
                        );
                    }
                }

                keyboard::Key::ArrowUp => {
                    if history_position > 0 {
                        let old_length = length;
                        history_position -= 1;

                        input.fill(0);
                        length = history_lengths[history_position];

                        input[..length].copy_from_slice(
                            &history[history_position][..length],
                        );

                        cursor_position = length;

                        redraw_input(
                            &input,
                            length,
                            cursor_position,
                            old_length,
                            prompt_column,
                            &mut prompt_row,
                            &mut last_scroll_count,
                        );
                    }
                }

                keyboard::Key::ArrowDown => {
                    if history_position < history_count {
                        let old_length = length;
                        history_position += 1;

                        input.fill(0);
                        length = 0;

                        // Select the next command, or a blank line.
                        if history_position < history_count {
                            length = history_lengths[history_position];

                            input[..length].copy_from_slice(
                                &history[history_position][..length],
                            );
                        }

                        cursor_position = length;

                        redraw_input(
                            &input,
                            length,
                            cursor_position,
                            old_length,
                            prompt_column,
                            &mut prompt_row,
                            &mut last_scroll_count,
                        );
                    }
                }

                keyboard::Key::Char(character) => {
                    match character {
                        '\n' => {
                            sync_prompt_row(&mut prompt_row, &mut last_scroll_count);
                            // Move to the next line before running a command.
                            vga::move_cursor(
                                (prompt_column + length) % 80,
                                prompt_row + (prompt_column + length) / 80,
                            );
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
                                            history[index - 1] = history[index];
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
                            cursor_position = 0;
                            input.fill(0);
                            history_position = history_count;

                            println!();

                            // Print the next prompt, then remember the
                            // cursor position immediately after it. Input
                            // must begin here, not at the start of the prompt.
                            print!("{}", PROMPT);
                            let (prompt_end_column, new_prompt_row) = vga::cursor_position();
                            prompt_column = prompt_end_column;
                            prompt_row = new_prompt_row;
                            last_scroll_count = vga::scroll_count();
                        }

                        '\u{8}' => {
                            // Delete the character immediately before the
                            // cursor, shifting the remaining text left.
                            if cursor_position > 0 {
                                let old_length = length;

                                for index in cursor_position..length {
                                    input[index - 1] = input[index];
                                }

                                length -= 1;
                                cursor_position -= 1;
                                input[length] = 0;

                                redraw_input(
                                    &input,
                                    length,
                                    cursor_position,
                                    old_length,
                                    prompt_column,
                                    &mut prompt_row,
                                    &mut last_scroll_count,
                                );
                            }
                        }

                        '\t' => {
                            // Complete commands only when the cursor is at
                            // the end of the command line.
                            if cursor_position == length {
                                let current =
                                    core::str::from_utf8(&input[..length])
                                        .unwrap_or("");

                                let commands = [
                                    "help",
                                    "about",
                                    "clear",
                                    "echo",
                                    "version",
                                    "date",
                                    "uname",
				    "breakpoint",
                                    "reboot",
                                ];

                                let mut matches = commands
                                    .iter()
                                    .filter(|command| {
                                        command.starts_with(current)
                                    });

                                // Complete only if exactly one command matches.
                                if let Some(command) = matches.next() {
                                    if matches.next().is_none() {
                                        let remaining = &command[length..];

                                        for byte in remaining.bytes() {
                                            if length < INPUT_LIMIT {
                                                input[length] = byte;
                                                length += 1;
                                                cursor_position += 1;
                                            }
                                        }

                                        redraw_input(
                                            &input,
                                            length,
                                            cursor_position,
                                            length,
                                            prompt_column,
                                            &mut prompt_row,
                                            &mut last_scroll_count,
                                        );
                                    }
                                }
                            }
                        }

                        ch if ch.is_ascii() && !ch.is_control() => {
                            if length < INPUT_LIMIT {
                                // Shift the text right to make room for the
                                // new character at the cursor.
                                for index in (cursor_position..length).rev() {
                                    input[index + 1] = input[index];
                                }

                                input[cursor_position] = ch as u8;
                                length += 1;
                                cursor_position += 1;

                                redraw_input(
                                    &input,
                                    length,
                                    cursor_position,
                                    length - 1,
                                    prompt_column,
                                    &mut prompt_row,
                                    &mut last_scroll_count,
                                );
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