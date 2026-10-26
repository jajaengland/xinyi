#![no_std]
#![no_main]

mod debug;
mod gdt;
mod interrupts;
mod syscall_frame;
mod keyboard;
mod multiboot;
mod paging;
mod phys;
mod rtc;
mod task;
mod usermode;
mod user_buffer;
mod vfs;
mod loader;
mod blockdev;
mod fat16;
mod fs_path;
mod ata;
mod ata_poll;
mod ata_capacity;
mod idle;
mod pci;
mod disk_target;
mod diskfs;
mod installer;
mod bootdisk;
mod vga;

use core::panic::PanicInfo;

const INPUT_LIMIT: usize = 128;
const HISTORY_LIMIT: usize = 8;
fn print_prompt() {
    let mut path = [0; fs_path::MAX_PATH];
    let length = diskfs::pwd(&mut path);
    // Paths are canonical ASCII 8.3 components supplied by the filesystem.
    let directory = core::str::from_utf8(&path[..length]).unwrap_or("/");
    print!("xinyi:{}> ", directory);
}

// Single source of truth for help output and Tab completion.
// Add new commands here as well as in the run_command dispatch below.
// Entries are (command name, usage text, description).
const COMMANDS: &[(&str, &str, &str)] = &[
    ("help", "help", "Show available commands"),
    ("about", "about", "About Xinyi"),
    ("clear", "clear", "Clear the screen"),
    ("echo", "echo TEXT", "Print text"),
    ("version", "version", "Show version"),
    ("date", "date", "Show the current date and time"),
    ("uname", "uname", "Show system information"),
    ("uptime", "uptime", "Show system uptime"),
    (
        "keyboard",
        "keyboard",
        "Show keyboard state and diagnostics",
    ),
    ("memory", "memory", "Show bootloader memory information"),
    ("paging", "paging", "Show paging configuration"),
    ("frames", "frames", "Show and test physical frame allocation"),
    ("usermode", "usermode [probe]", "Test protected ring 3 execution"),
    ("ps", "ps", "Show running processes"),
    ("run", "run [demo|FILE]", "Run a user program and wait for its exit"),
    ("spawn", "spawn [demo]", "Create a paused demo task record (diagnostic)"),
    ("kill", "kill PID", "Remove a task record and free its memory"),
    ("ls", "ls [PATH]", "List files and folders"),
    ("touch", "touch FILE", "Create a file"),
    ("delete", "delete FILE", "Delete a file from the active filesystem"),
    ("mkdir", "mkdir PATH", "Create a folder"),
    ("rmdir", "rmdir PATH", "Remove an empty folder"),
    ("cd", "cd [PATH]", "Change directory; no path returns to root"),
    ("pwd", "pwd", "Show the current directory"),
    (
        "write",
        "write FILE [TEXT]",
        "Replace or clear file contents (up to 1024 bytes)",
    ),
    ("cat", "cat FILE", "Display file contents"),
    ("mount", "mount", "Mount FAT16 on the selected IDE disk"),
    ("unmount", "unmount", "Return to the in-memory filesystem"),
    ("format", "format", "Preview destructive primary-disk FAT16 format"),
    (
        "breakpoint",
        "breakpoint",
        "Trigger the CPU breakpoint exception",
    ),
    ("disk", "disk [scan|select]", "Test, scan, or explicitly select an IDE disk"),
    ("install", "install", "Preview bootable selected-disk installation"),
    ("reboot", "reboot", "Reboot the machine"),
];

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

// Compare an ASCII command name case-insensitively without allocating.
fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.as_bytes()[..prefix.len()]
            .eq_ignore_ascii_case(prefix.as_bytes())
}

// Offer a suggestion only when exactly one command matches by prefix.
// Both directions are checked so truncated and overlong names are covered.
fn suggest_command(command: &str) -> Option<&'static str> {
    let mut matches = COMMANDS
        .iter()
        .map(|(name, _, _)| *name)
        .filter(|name| {
            starts_with_ignore_ascii_case(name, command)
                || starts_with_ignore_ascii_case(command, name)
        });

    let suggestion = matches.next()?;

    if matches.next().is_none() {
        Some(suggestion)
    } else {
        None
    }
}

fn run_command(input: &str) {
    let input = input.trim();

    if input.is_empty() {
        return;
    }

    let (command, argument) = match input.find(char::is_whitespace) {
        Some(index) => (&input[..index], input[index..].trim()),
        None => (input, ""),
    };

    // Formatting requires a fresh preview immediately followed by confirmation.
    if command != "format" || argument != "confirm" {
        diskfs::cancel_format();
    }
    if command != "install" || argument != "confirm" {
        installer::cancel();
    }
    match command {
        "help" => {
            println!("Xinyi commands:");

            for (_, usage, description) in COMMANDS {
                println!("  {:<10} - {}", usage, description);
            }
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

        "panic" => {
            println!("Triggering kernel panic...");
            panic!("panic command requested");
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

        "keyboard" => {
            println!(
                "Left shift:  {}",
                if keyboard::left_shift_down() { "down" } else { "up" }
            );
            println!(
                "Right shift: {}",
                if keyboard::right_shift_down() { "down" } else { "up" }
            );
            println!(
                "Caps lock:   {}",
                if keyboard::caps_lock_on() { "on" } else { "off" }
            );
            println!(
                "Buffered scancodes: {}",
                keyboard::queued_scancodes()
            );
            println!(
                "Dropped scancodes:  {}",
                keyboard::dropped_scancodes()
            );
        }

        "memory" => {
            if let Some((lower, upper)) = multiboot::memory_summary() {
                println!("Multiboot memory:");
                println!("  Lower: {} KiB", lower);
                println!("  Upper: {} KiB", upper);
            } else {
                println!("Multiboot memory information unavailable.");
            }

            let mut regions = 0usize;

            multiboot::for_each_region(|region| {
                regions += 1;

                if regions <= 16 {
                    println!(
                        "  {:#010x}-{:#010x} {}",
                        region.base,
                        region.base.wrapping_add(region.length),
                        multiboot::region_kind(region.kind)
                    );
                }
            });

            if regions == 0 {
                println!("  Memory map unavailable.");
            } else if regions > 16 {
                println!("  ... {} more regions", regions - 16);
            }
        }

        "paging" => {
            let status = paging::status();

            println!(
                "Paging: {}",
                if status.enabled { "enabled" } else { "disabled" }
            );

            if status.enabled {
                println!("  Kernel: supervisor 4 MiB pages (PSE)");
                println!("  User: private 4 KiB code/stack mappings");
                println!(
                    "  Identity-mapped: 0x00000000-{:#010x}",
                    paging::MAPPED_BYTES
                );
            }

            println!("  CR3: {:#010x}", status.page_directory);
            println!("  CR0: {:#010x}", status.cr0);
            println!("  CR4: {:#010x}", status.cr4);
        }

        "frames" => {
            let stats = phys::stats();

            println!("Physical frames (4 KiB):");
            println!("  Available: {}", stats.available_frames);
            println!("  Allocated: {}", stats.allocated_frames);
            println!("  Free:      {}", stats.free_frames);

            if let Some(frame) = phys::alloc_frame() {
                println!("  Allocated: {:#010x}", frame);
                if !phys::free_frame(frame) {
                    println!("  Failed to free {:#010x}", frame);
                } else {
                    println!("  Freed:     {:#010x}", frame);
                }
            } else {
                println!("  Allocation failed.");
            }

            let stats = phys::stats();
            println!("  Free now:  {}", stats.free_frames);
        }

        "usermode" => {
            println!("Entering user mode (ring 3)...");
            usermode::enter(argument);
            println!("Returned from user mode.");
        }

        "ps" => {
            println!("Prototype task records (not scheduled):");
            if let Some(pid) = task::current_task_id() {
                println!("Current record: {}", pid);
            } else {
                println!("(no tasks)");
            }
            println!("Total tasks: {}", task::task_count());
        }

        "run" | "spawn" => {
            if command == "spawn" && !argument.is_empty() && argument != "demo" {
                println!("Only the built-in demo task record is supported.");
                return;
            }
            let mut file = [0; fat16::MAX_FILE_SIZE];
            let bytes = if argument.is_empty() || argument == "demo" {
                loader::DEMO_BINARY
            } else {
                match diskfs::read(argument, &mut file) {
                    Ok(0) => { println!("Program is empty."); return; }
                    Ok(size) => &file[..size],
                    Err(error) => { println!("Storage: {}", error.message()); return; }
                }
            };
            if let Some(program) = loader::load_binary(bytes) {
                if let Some(pid) = task::create_task(program) {
                    if command == "run" {
                        println!("Running process {}...", pid);
                        task::execute(pid);
                        return;
                    }
                    let (entry, size, user_stack, kernel_stack, directory) = task::describe(pid).unwrap();
                    println!("Created task record {} (not executing)", pid);
                    println!("Program: {:#010x} ({} bytes)", entry, size);
                    println!("User stack backing top: {:#010x}", user_stack);
                    println!("Kernel stack top: {:#010x}", kernel_stack);
                    println!("Page directory: {:#010x}", directory);
                } else {
                    println!("Failed to create task record");
                }
            } else {
                println!("Out of memory");
            }
        }

        "kill" => {
            match argument.parse::<u32>() {
                Ok(pid) if task::remove(pid) => println!("Removed task record {}", pid),
                Ok(_) => println!("Task record not found"),
                Err(_) => println!("Usage: kill PID"),
            }
        }

        "pwd" => {
            let mut path = [0; fs_path::MAX_PATH];
            let length = diskfs::pwd(&mut path);
            println!("{}", core::str::from_utf8(&path[..length]).unwrap());
        }
        "cd" => {
            if let Err(error) = diskfs::cd(if argument.is_empty() { "/" } else { argument }) {
                println!("Storage: {}", error.message());
            }
        }
        "mkdir" | "rmdir" => {
            if argument.is_empty() { println!("Usage: {} PATH", command); }
            else {
                let result = if command == "mkdir" { diskfs::mkdir(argument) } else { diskfs::rmdir(argument) };
                match result {
                    Ok(()) => println!("{}: {}", if command == "mkdir" { "Created directory" } else { "Removed directory" }, argument),
                    Err(error) => println!("Storage: {}", error.message()),
                }
            }
        }
        "ls" => {
            println!(
                "Files ({}):",
                if diskfs::is_mounted() { "FAT16 disk" } else { "RAM, not persistent" }
            );
            let mut count = 0;
            let mut directories = 0;
            if let Err(error) = diskfs::list(argument, |name, size, directory| {
                let mut text = [0u8; 12];
                let len = fat16::display_name(name, &mut text);
                if directory {
                    println!("  {}/  <DIR>", core::str::from_utf8(&text[..len]).unwrap_or("(non-ASCII)"));
                    directories += 1;
                    return;
                }
                println!(
                    "  {:<12} {} bytes",
                    core::str::from_utf8(&text[..len]).unwrap_or("(non-ASCII)"),
                    size
                );
                count += 1;
            }) {
                println!("Storage: {}", error.message());
            }
            println!("{} file(s)", count);
            if directories != 0 { println!("{} folder(s)", directories); }
        }

        "touch" => {
            if argument.is_empty() {
                println!("Usage: touch FILE");
            } else {
                match diskfs::create(argument) {
                    Ok(()) => println!("Created: {}", argument),
                    Err(error) => println!("Storage: {}", error.message()),
                }
            }
        }

        "delete" => {
            if argument.is_empty() {
                println!("Usage: delete FILE");
            } else {
                match diskfs::delete(argument) {
                    Ok(()) => println!("Deleted: {}", argument),
                    Err(error) => println!("Storage: {}", error.message()),
                }
            }
        }

        "cat" => {
            if argument.is_empty() {
                println!("Usage: cat FILE");
            } else {
                let mut data = [0; fat16::MAX_FILE_SIZE];
                match diskfs::read(argument, &mut data) {
                    Ok(size) => println!(
                        "{}",
                        core::str::from_utf8(&data[..size]).unwrap_or("(binary)")
                    ),
                    Err(error) => println!("Storage: {}", error.message()),
                }
            }
        }

        "write" => {
            if !argument.is_empty() {
                let (name, text) = match argument.find(char::is_whitespace) {
                    Some(index) => (&argument[..index], argument[index..].trim_start()),
                    None => (argument, ""),
                };
                match diskfs::write(name, text.as_bytes()) {
                    Ok(()) => println!("Wrote {} bytes to {}", text.len(), name),
                    Err(error) => println!("Storage: {}", error.message()),
                }
            } else {
                println!("Usage: write FILE [TEXT] (create it with touch first)");
            }
        }
        "mount" => match diskfs::mount() {
            Ok(start) => println!("Mounted FAT16 at sector {}. File commands now use disk.", start),
            Err(error) => println!("Mount failed: {}", error.message()),
        },
        "unmount" => match diskfs::unmount() {
            Ok(()) => println!("Unmounted. File commands now use RAM (not persistent)."),
            Err(error) => println!("Unmount failed: {}", error.message()),
        },
        "disk" => match argument {
            "" => diskfs::test(),
            "scan" => diskfs::scan(),
            _ if argument.starts_with("select ") => diskfs::select_command(&argument[7..]),
            _ => println!("Usage: disk select BB:DD.F primary|secondary master|slave"),
        },
        "format" => match argument {
            "" => match diskfs::prepare_format() {
                Ok(plan) => {
                    println!("Target: selected IDE disk, {} sectors ({} MiB).", plan.disk_sectors, plan.disk_sectors / 2048);
                    diskfs::report_target();
                    println!("WARNING: removes all partitions, files, and bootability on this disk.");
                    println!("This includes any installed OS. Quick format is not a secure erase.");
                    println!("Creates a {} MiB FAT16 data partition at sector 2048.", plan.volume_sectors / 2048);
                    if plan.disk_sectors > fat16::FormatPlan::START + plan.volume_sectors {
                        println!("Remaining disk space will be unallocated (FAT16 size limit).");
                    }
                    println!("Boot source is not detected; verify this is the intended target.");
                    println!("To erase this disk, enter: format confirm");
                    println!("Any other command cancels confirmation.");
                }
                Err(error) => println!("Format refused: {}", error.message()),
            },
            "confirm" => match diskfs::confirm_format() {
                Ok(()) => println!("Format complete. Run mount to use the empty FAT16 disk."),
                Err(error) => println!("Format failed: {}", error.message()),
            },
            _ => println!("Usage: format, then format confirm. Unmount first."),
        },

        "install" => {
            if diskfs::is_mounted() {
                installer::cancel();
                println!("Unmount the filesystem before installing Xinyi.");
                return;
            }
            match argument {
                "" => match installer::prepare() {
                    Ok(plan) => {
                        println!("Target: selected IDE disk, {} MiB.", plan.disk_sectors/2048);
                        diskfs::report_target();
                        println!("Installs the BIOS bootloader and {} kernel bytes.",plan.kernel.bytes);
                        if plan.preserve_data {
                            println!("Existing compatible FAT16 data and partition table will be preserved.");
                            println!("The current bootloader will be replaced.");
                        } else {
                            println!("WARNING: all existing partitions, files, and OS boot loaders will be replaced.");
                            println!("Creates a separate FAT16 data partition (up to 2 GiB).");
                            if plan.disk_sectors > fat16::FormatPlan::START + fat16::FormatPlan::MAX_VOLUME_SECTORS {
                                println!("Remaining disk space will be unallocated.");
                            }
                        }
                        println!("To install, enter: install confirm");
                        println!("Any other command cancels confirmation.");
                    }
                    Err(error) => println!("Install refused: {}",error.message()),
                },
                "confirm" => match installer::confirm() {
                    Ok(()) => {
                        println!("Installation complete: bootloader, kernel, and FAT16 data are ready.");
                        println!("Boot the selected IDE disk in legacy BIOS mode. Run mount for files.");
                    }
                    Err(error) => println!("Install failed: {}",error.message()),
                },
                _ => println!("Usage: install, then install confirm. Unmount first."),
            }
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
		"uptime" => {
			let total_seconds = interrupts::uptime_seconds();
			let hours = total_seconds / 3600;
			let minutes = (total_seconds % 3600) / 60;
			let seconds = total_seconds % 60;

			println!(
				"Uptime: {:02}:{:02}:{:02}",
				hours,
				minutes,
				seconds
			);
		}

        _ => {
            println!("Unknown command: '{}'", command);

            if let Some(suggestion) = suggest_command(command) {
                println!("Did you mean '{}'?", suggestion);
            }

            println!("Type 'help' for available commands.");
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_main(magic: u32, info_addr: u32) -> ! {
    // Preserve bootloader-provided memory information before anything else.
    multiboot::init(magic, info_addr);
    diskfs::restore_boot_target();

    // Install the kernel-owned GDT before relying on any segment selectors.
    gdt::init();

    // Enable identity-mapped paging before installing handlers so every
    // kernel structure is reachable through the final address space.
    let paging_result = paging::init();

    // Build the physical frame bitmap from the boot memory map.
    let phys_stats = phys::init();

    // Initialize task management
    task::init();

    // Initialize virtual filesystem
    vfs::init();

    // Install CPU exception and hardware IRQ handlers.
    interrupts::init();

    // Remap the PIC and configure the PIT while interrupts remain disabled.
    interrupts::remap_pic();
    interrupts::init_pit();

    // Enable CPU interrupts only after the handlers and timer are ready.
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }

    vga::clear_screen();

    if let Err(reason) = paging_result {
        println!("Paging disabled: {}", reason);
        println!();
    }

    if phys_stats.available_frames == 0 {
        println!("Warning: no physical frames are available.");
        println!();
    }

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

    print_prompt();
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
                            print_prompt();
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

                                let mut matches = COMMANDS
                                    .iter()
                                    .map(|(name, _, _)| *name)
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
            idle::wait_for_input();
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!();
    println!("Xinyi: kernel panic!");
    println!("{}", info.message());

    // Mirror the panic to the debug console so a headless or automated run
    // still reports why the kernel stopped.
    debug::write_str("Xinyi panic: ");
    debug::write_fmt(format_args!("{}", info.message()));

    if let Some(location) = info.location() {
        println!(
            "Location: {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
        debug::write_str(" at ");
        debug::write_str(location.file());
        debug::write_str(":");
        debug::write_u32(location.line());
        debug::write_str(":");
        debug::write_u32(location.column());
    }

    debug::write_str("\r\n");
    println!("System halted.");

    loop {
        unsafe {
            core::arch::asm!("cli; hlt", options(nomem, nostack));
        }
    }
}
