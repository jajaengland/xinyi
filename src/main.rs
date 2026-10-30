#![no_std]
#![no_main]

mod debug;
mod power;
mod acpi_sleep;
mod gdt;
mod interrupts;
mod syscall_frame;
mod keyboard;
mod multiboot;
mod paging;
mod phys;
mod rtc;
mod monotonic;
mod clock_model;
mod task;
mod usermode;
mod user_buffer;
mod user_memory;
mod elf;
mod terminal;
mod speaker;
mod audio;
mod audio_model;
mod sound_model;
mod system_sounds;
mod stream_protocol;
mod permissions;
mod program_image;
mod load_model;
mod files_api;
mod clipboard;
mod file_clipboard;
mod theme;
mod theme_model;
mod terminal_service;
mod directory_api;
mod directory_protocol;
mod shell;
mod pty;
mod pty_model;
mod bios;
mod bios_disk;
mod vbe;
mod vbe_mode;
mod canvas;
mod graphics;
mod ps2;
mod mouse;
mod mouse_packet;
mod mouse_ui;
mod desktop_ui;
mod desktop_shell;
mod desktop_paint;
mod app_catalog;
mod app_manifest;
mod app_metadata;
mod app_install;
mod app_tree;
mod cpu_context;
mod scheduler_model;
mod multitask;
mod multi_window;
mod ui_font;
mod window_protocol;
mod window_queue;
mod app_window_ui;
mod window_api;
mod input_order;
mod vfs;
mod loader;
mod blockdev;
mod block_cache;
mod fat16;
mod fs_path;
mod ata;
mod ata_poll;
mod ata_capacity;
mod idle;
mod pci;
mod rtl8139;
mod net_model;
mod network;
mod disk_target;
mod diskfs;
mod disk_io;
mod installer;
mod bootdisk;
mod vga;

use core::panic::PanicInfo;
use blockdev::BlockDevice;

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
    ("multitask", "multitask [hog|fault|check|wait]", "Run two isolated counter programs concurrently"),
    ("files", "files", "Open the file manager"),
    ("settings", "settings", "Open appearance settings"),
    ("sounds", "sounds [list|test NAME|volume N|mute|unmute|save]", "System voice sounds and volume"),
    ("terminal", "terminal", "Open a graphical terminal window"),
    ("desktop", "desktop", "Open the multitasking desktop and Apps menu"),
    ("windowtest", "windowtest", "Run the legacy window/launcher diagnostic"),
    ("mouse", "mouse", "Test PS/2 mouse, buttons and right-click menu; Escape exits"),
    ("graphics", "graphics", "Test framebuffer graphics; Escape returns to text"),
    ("bios", "bios", "Show boot-firmware disk and VESA diagnostics"),
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
    ("apps", "apps", "List programs installed in /APPS"),
    ("app", "app install FOLDER | app remove /APPS/ID", "Install or remove an app folder"),
    ("notepad", "notepad [FILE]", "Open a file in graphical Notepad"),
    ("edit", "edit [FILE]", "Open a text file in the editor"),
    ("disks", "disks", "List available numbered drives"),
    ("use", "use NUMBER", "Select a numbered drive and mount its filesystem"),
    ("run", "run [--read-only|--isolated|--media] APP [ARGS]", "Run an application with a selected access profile"),
    ("spawn", "spawn [demo]", "Create a paused demo task record (diagnostic)"),
    ("kill", "kill PID", "Stop a desktop process or remove a diagnostic task record"),
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
    ("mount", "mount [NUMBER]", "Open a drive or mount the selected filesystem"),
    ("unmount", "unmount", "Return to the in-memory filesystem"),
    ("fsck", "fsck [repair-orphans|confirm]", "Check the selected FAT16 volume; preview orphan cleanup"),
    ("format", "format", "Preview destructive primary-disk FAT16 format"),
    (
        "breakpoint",
        "breakpoint",
        "Trigger the CPU breakpoint exception",
    ),
    ("disk", "disk [scan|select]", "Test, scan, or explicitly select an IDE disk"),
    ("install", "install [NUMBER]", "Preview installation and copying /APPS to a drive"),
    ("reboot", "reboot", "Reboot the machine"),
    ("shutdown", "shutdown [confirm]", "Flush disks and power off or halt"),
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

fn launch_program(name:&str,arguments:&str)->bool {
    if pty::current().is_some() {return pty::launch(name,arguments);}
    launch_with_profile(name,arguments,permissions::Profile::for_name(name))
}
fn launch_with_profile(name:&str,arguments:&str,profile:permissions::Profile)->bool {
    if pty::current().is_some() {return pty::launch_with_profile(name,arguments,profile);}
    if profile==permissions::Profile::Trusted && (name.eq_ignore_ascii_case("terminal")||name.eq_ignore_ascii_case("terminal.bin")) {multitask::terminal_desktop();return true;}
    if arguments.len()>128 {println!("Program arguments exceed 128 bytes.");return false;}
    let mut program=match loader::from_disk(name) {Ok(p)=>p,Err(e)=>{println!("Program: {}",e);return false;}};
    program.profile=program.profile.restrict(profile);
    println!("Application access: {}",profile.label());
    program.arguments[..arguments.len()].copy_from_slice(arguments.as_bytes());program.argument_length=arguments.len();
    if let Some(pid)=task::create_task(program) {println!("Running process {}...",pid);task::execute(pid);true}
    else {println!("Failed to create task record");false}
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
    if command != "fsck" || argument != "confirm" {
        diskfs::cancel_repair();
    }
    if command != "install" || argument != "confirm" {
        installer::cancel();
    }
    if pty::current().is_some()&&matches!(command,"usermode"|"spawn"|"multitask") {println!("Run this kernel diagnostic from the physical console.");return;}
    match command {
        "net"=>network::command(argument),
        "ping"=>network::ping(argument),
        "exit"=>{if let Some(id)=pty::current() {pty::exit_shell(id);}else{println!("The physical console has no parent shell.");}},
        "help" => {
            println!("Xinyi commands:");

            for (_, usage, description) in COMMANDS {
                println!("  {:<10} - {}", usage, description);
            }
        }

        "about" => {
            system_sounds::play(system_sounds::Event::Info);
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
            if multitask::active() {
                multitask::list_processes();
                return;
            }
            println!("Prototype task records (not scheduled):");
            if let Some(pid) = task::current_task_id() {
                println!("Current record: {}", pid);
            } else {
                println!("(no tasks)");
            }
            println!("Total tasks: {}", task::task_count());
        }

        "bios"=>{
            println!("BIOS bridge: {}",if bios::available() {"available"}else{"unavailable (boot the disk/USB image)"});
            if let Some(drive)=multiboot::bios_drive() {println!("Boot drive: {:02x}; sectors: {}; last disk status: {:02x}",drive,bios_disk::BOOT.block_count(),bios_disk::BOOT.status());}
            match vbe::find_mode() {Ok(mode)=>println!("VBE mode {:04x}: 800x600x{} at {:08x}, pitch {}",mode.number,mode.bpp,mode.base,mode.pitch),Err(message)=>println!("{}",message)}
        },
        "multitask"=>multitask::demo(argument),
        "desktop"=>multitask::desktop(),
        "windowtest"=>graphics::desktop_demo(),
        "mouse"=>graphics::mouse_demo(),
        "graphics"=>graphics::demo(),
        "apps" => {
            let mut catalog=app_catalog::Catalog::new();
            match multi_window::collect_catalog(&mut catalog) {
                Ok(())=>for entry in &catalog.entries[..catalog.len] {
                    let path=core::str::from_utf8(&entry.path[..entry.path_len]).unwrap();
                    println!("  {} - {}",path.rsplit('/').next().unwrap(),core::str::from_utf8(entry.title()).unwrap_or("Application"));
                },
                Err(error)=>println!("Applications: {}. Use disks and use NUMBER to choose a drive.",error.message()),
            }
        }
        "app"=>{
            let (operation,path)=argument.split_once(' ').unwrap_or((argument,""));
            let result=match operation {
                "install" if !path.is_empty()=>app_install::install(path.trim()),
                "remove" if !path.is_empty()=>app_install::remove(path.trim()),
                _=>{println!("Usage: app install FOLDER | app remove /APPS/ID");return;},
            };
            match result {Ok(())=>println!("Application {} complete.",operation),Err(e)=>println!("Application: {}",e.message())}
        }
        "sounds" => system_sounds::command(argument),
        "edit" => {launch_program("edit", argument);},
        "notepad" => {launch_program("notepad", argument);},
        "files" => {launch_program("files", argument);},
        "settings" => {launch_program("settings", argument);},
        "terminal" => {launch_program("terminal", argument);},
        "disks" => diskfs::list_drives(),
        "use" => match argument.parse::<usize>() {
            Ok(number) => if let Err(error)=diskfs::use_drive(number,true) {println!("Drive: {}",error.message());},
            Err(_) => println!("Usage: use NUMBER (see disks)"),
        },

        "run" | "spawn" => {
            if command == "run" && !argument.is_empty() && argument != "demo" {
                match permissions::parse(argument) {
                    Ok(request)=>{launch_with_profile(request.name,request.arguments,request.profile);},
                    Err(message)=>println!("{}",message),
                }
                return;
            }
            if command == "spawn" && !argument.is_empty() && argument != "demo" {
                println!("Only the built-in demo task record is supported.");
                return;
            }
            let program = if argument.is_empty() || argument == "demo" {
                loader::load_binary(loader::DEMO_BINARY)
            } else {
                // The staging buffer lives in owned frames, not the 16 KiB
                // kernel stack. Drop it before creating/running the task.
                let Some(staging) = loader::Frames::new(loader::MAX_BINARY_BYTES / 4096) else {
                    println!("Out of memory"); return;
                };
                let file = unsafe { core::slice::from_raw_parts_mut(staging.base() as *mut u8, loader::MAX_BINARY_BYTES) };
                match diskfs::read_program(argument, file) {
                    Ok(0) => { println!("Program is empty."); return; }
                    Ok(size) => loader::load_binary(&file[..size]),
                    Err(error) => { println!("Storage: {}", error.message()); return; }
                }
            };
            if let Some(program) = program {
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
            if multitask::active() {
                match argument.parse::<u32>() {
                    Ok(pid) => match multitask::request_kill(pid) {
                        Ok(()) => println!("Termination requested for process {}", pid),
                        Err(message) => println!("{}", message),
                    },
                    Err(_) => println!("Usage: kill PID"),
                }
                return;
            }
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
        "mount" if !argument.is_empty()=>match argument.parse::<usize>() {
            Ok(number)=>if let Err(error)=diskfs::use_drive(number,true) {println!("Mount failed: {}",error.message());},
            Err(_)=>println!("Usage: mount [NUMBER]"),
        },
        "mount" => match diskfs::mount() {
            Ok(start) => {println!("Mounted FAT16 at sector {}. File commands now use disk.", start);system_sounds::play(system_sounds::Event::Connect);},
            Err(error) => println!("Mount failed: {}", error.message()),
        },
        "fsck" => diskfs::check_command(argument),
        "unmount" => match diskfs::unmount() {
            Ok(()) => {println!("Unmounted. File commands now use RAM (not persistent).");system_sounds::play(system_sounds::Event::Disconnect);},
            Err(error) => println!("Unmount failed: {}", error.message()),
        },
        "disk" => match argument {
            "list" => diskfs::list_drives(),
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
                    system_sounds::play(system_sounds::Event::Warning);
                }
                Err(error) => println!("Format refused: {}", error.message()),
            },
            "confirm" => match diskfs::confirm_format() {
                Ok(()) => {println!("Format complete. Run mount to use the empty FAT16 disk.");system_sounds::play(system_sounds::Event::Complete);},
                Err(error) => println!("Format failed: {}", error.message()),
            },
            _ => println!("Usage: format, then format confirm. Unmount first."),
        },

        "install" => {
            match argument {
                "" => match installer::prepare_friendly(None) {
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
                        system_sounds::play(system_sounds::Event::Warning);
                    }
                    Err(error) => println!("Install refused: {}",error.message()),
                },
                "confirm" => match installer::confirm() {
                    Ok(()) => {
                        println!("Installation complete: bootloader, kernel, and FAT16 data are ready.");
                        system_sounds::play(system_sounds::Event::Complete);
                        if diskfs::mount().is_ok() {println!("Installed drive ready. Type apps or edit FILE.");}
                    }
                    Err(error) => println!("Install failed: {}",error.message()),
                },
                _ => match argument.parse::<usize>() {
                    Ok(number)=>match installer::prepare_friendly(Some(number)) {
                        Ok(plan)=>{
                            println!("Install on drive {}: {} MiB. {}",number,plan.disk_sectors/2048,
                                if plan.preserve_data {"Existing FAT16 files are preserved; matching apps replaced."}else{"WARNING: all partitions and files on this drive will be replaced."});
                            println!("To install, enter: install confirm");
                        }
                        Err(error)=>println!("Install refused: {}",error.message()),
                    },Err(_)=>println!("Usage: install [NUMBER], then install confirm."),
                },
            }
        }

        "shutdown" => {if !argument.is_empty()&&argument!="confirm"{println!("Usage: shutdown [confirm]");}else if multitask::active()&&argument!="confirm"{println!("Shutdown closes running applications. Save your work, then shutdown confirm.");}else{power::shutdown();}}
        "reboot" => {
            audio::farewell();
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
            system_sounds::play(system_sounds::Event::Error);
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
    bios::init();
    monotonic::init();
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
    diskfs::startup_mount();
    theme::load();
    power::init();
    audio::init();
    network::init();
    system_sounds::load();
    system_sounds::play(system_sounds::Event::Startup);
    println!();

    shell::Shell::physical()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    pty::set_current(None);
    graphics::restore();
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
