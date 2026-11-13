use crate::{bootdisk::InstallPlan, canvas::Canvas, diskfs, graphics, installer, keyboard::Key};
use core::fmt::{self, Write};

const INK: u32 = 0x172e43;
const BLUE: u32 = 0x245f83;
const AMBER: u32 = 0x8a400d;
const ROWS: usize = 8;

struct Line { bytes: [u8; 88], len: usize }
impl Write for Line {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let n = text.len().min(self.bytes.len() - self.len);
        self.bytes[self.len..self.len + n].copy_from_slice(&text.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}
fn label(c: &mut Canvas<'_>, y: i32, color: u32, text: fmt::Arguments<'_>) {
    let mut line = Line { bytes: [0; 88], len: 0 };
    let _ = line.write_fmt(text);
    c.label(84, y, 632, color, &line.bytes[..line.len]);
}

#[derive(Clone, Copy)]
enum Page { Drives, Preparing, Review(InstallPlan), Installing(usize, usize), Done, Failed(&'static str) }

struct Wizard {
    drives: [Option<([u8; 40], u32)>; 16],
    numbers: [usize; 16],
    count: usize,
    selected: usize,
    page: Page,
    cursor: (i32, i32),
}

impl Wizard {
    fn render(&self) {
        graphics::paint(|c| {
            c.rect(0, 0, 800, 600, 0x20374d);
            c.rect(56, 55, 688, 490, 0xf1f5f9);
            c.rect(56, 55, 688, 68, BLUE);
            label(c, 80, 0xffffff, format_args!("Install Xinyi"));
            let (heading, back, next) = match self.page {
                Page::Drives => ("Choose a destination drive", "Cancel", if self.count > 0 { "Review" } else { "" }),
                Page::Preparing => ("Reading the installation source and destination...", "", ""),
                Page::Review(_) => ("Review the changes before installing", "Back", "Install now"),
                Page::Installing(..) => ("Installing Xinyi. Keep the computer powered on.", "", ""),
                Page::Done => ("Installation complete", "Exit", "Reboot"),
                Page::Failed(_) => ("Installation could not continue", "Back", ""),
            };
            label(c, 146, INK, format_args!("{}", heading));
            match self.page {
                Page::Drives if self.count == 0 => label(c, 191, INK, format_args!("No supported destination drive found.")),
                Page::Drives => {
                    let first = self.selected / ROWS * ROWS;
                    for (row, &index) in self.numbers[first..self.count.min(first + ROWS)].iter().enumerate() {
                        let y = 181 + row as i32 * 33;
                        let active = first + row == self.selected;
                        let (model, sectors) = self.drives[index].unwrap();
                        c.rect(80, y, 640, 30, if active { BLUE } else { 0xe2ebf2 });
                        label(c, y+7, if active { 0xffffff } else { INK }, format_args!("Drive {}   {}   {} MiB", index+1, model_name(&model), sectors/2048));
                    }
                    label(c, 462, INK, format_args!("Use Up/Down or click a drive. Then choose Review."));
                }
                Page::Review(plan) => {
                    let index = self.numbers[self.selected];
                    let (model, _) = self.drives[index].unwrap();
                    label(c, 186, INK, format_args!("Destination: Drive {}   {}   {} MiB", index+1, model_name(&model), plan.disk_sectors/2048));
                    if plan.preserve_data {
                        label(c, 226, AMBER, format_args!("The bootloader and matching bundled app files will be replaced."));
                        label(c, 251, INK, format_args!("The existing FAT16 volume and partition table will be kept."));
                    } else {
                        label(c, 226, AMBER, format_args!("All existing partitions, files and operating systems will be lost."));
                        label(c, 251, INK, format_args!("Xinyi will create a {} MiB FAT16 partition.", plan.volume_sectors/2048));
                    }
                    let outside = plan.disk_sectors.saturating_sub(plan.volume_sectors + crate::fat16::FormatPlan::START);
                    if plan.disk_sectors > crate::fat16::FormatPlan::START + crate::fat16::FormatPlan::MAX_VOLUME_SECTORS {
                        label(c, 302, AMBER, format_args!("Capacity warning: at least {} MiB will be unusable by Xinyi.", outside/2048));
                        label(c, 327, AMBER, format_args!("{}", if plan.preserve_data {
                            "Xinyi only uses the existing FAT16 volume."
                        } else { "Space beyond the FAT16 partition will be left unallocated." }));
                    }
                    label(c, 460, INK, format_args!("Choose Install now or press I to write to this drive."));
                }
                Page::Installing(done, total) => {
                    c.rect(84, 240, 632, 16, 0xd8e4ec);
                    c.rect(84, 240, (632 * done / total) as i32, 16, BLUE);
                    label(c, 270, INK, format_args!("{} of {} steps completed", done, total));
                }
                Page::Done => label(c, 190, INK, format_args!("Remove the installation media before booting the installed drive.")),
                Page::Failed(error) => label(c, 190, AMBER, format_args!("{}", error)),
                Page::Preparing => {}
            }
            for (x, caption) in [(84, back), (576, next)] {
                if !caption.is_empty() {
                    c.rect(x, 500, 140, 32, if x == 84 { INK } else { BLUE });
                    c.label(x+12, 508, 116, 0xffffff, caption.as_bytes());
                }
            }
        });
        graphics::present_rect(0, 0, 800, 600);
        graphics::pointer(self.cursor.0, self.cursor.1);
    }
}

fn model_name(bytes: &[u8; 40]) -> &str {
    core::str::from_utf8(bytes).unwrap_or("Disk").trim()
}

pub(crate) fn run() {
    if graphics::active() || !diskfs::boot_selected() || !diskfs::is_mounted() {
        crate::println!("Start setup from the physical console with the boot media mounted as drive 0.");
        return;
    }
    if diskfs::directory_exists("/APPS").is_err() {
        crate::println!("Setup requires installation media with bundled apps.");
        return;
    }
    if let Err(message) = graphics::start() {
        crate::println!("{}", message);
        return;
    }
    let drives = diskfs::installer_drives();
    let mut numbers = [0; 16];
    let mut count = 0;
    for (index, drive) in drives.iter().enumerate() {
        if drive.is_some() {
            numbers[count] = index;
            count += 1;
        }
    }
    let mut wizard = Wizard { drives, numbers, count, selected: 0, page: Page::Drives, cursor: (400, 300) };
    if !crate::mouse::enabled() { crate::mouse::init(); }
    let mut buttons = crate::mouse::discard();
    wizard.render();
    crate::debug::write_str("Setup: drive selection ready\n");
    loop {
        let mut key = crate::keyboard::poll_key();
        if let Some(event) = crate::mouse::poll() {
            let old = wizard.cursor;
            wizard.cursor = ((old.0 + i32::from(event.dx)).clamp(0, 799), (old.1 + i32::from(event.dy)).clamp(0, 599));
            graphics::move_pointer(old, wizard.cursor);
            let (x, y) = wizard.cursor;
            if !event.lost && event.buttons & 1 != 0 && buttons & 1 == 0 {
                if (500..532).contains(&y) {
                    if (84..224).contains(&x) { key = Some(Key::Char('\x1b')); }
                    if (576..716).contains(&x) { key = Some(Key::Char(if matches!(wizard.page, Page::Review(_)) { 'i' } else { '\n' })); }
                } else if matches!(wizard.page, Page::Drives) && (80..720).contains(&x) && (181..445).contains(&y) {
                    let row = wizard.selected / ROWS * ROWS + ((y-181)/33) as usize;
                    if row < wizard.count {
                        wizard.selected = row;
                        wizard.render();
                    }
                }
            }
            buttons = event.buttons;
        }
        match (wizard.page, key) {
            (Page::Drives | Page::Done, Some(Key::Char('\x1b'))) => break,
            (Page::Review(_) | Page::Failed(_), Some(Key::Char('\x1b'))) => {
                installer::cancel();
                wizard.page = match diskfs::use_drive(0, true) {
                    Ok(()) => Page::Drives,
                    Err(_) => break,
                };
            }
            (Page::Drives, Some(Key::ArrowUp)) => wizard.selected = wizard.selected.saturating_sub(1),
            (Page::Drives, Some(Key::ArrowDown)) if wizard.selected + 1 < wizard.count => wizard.selected += 1,
            (Page::Drives, Some(Key::Char('\n'))) if wizard.count > 0 => {
                wizard.page = Page::Preparing;
                wizard.render();
                wizard.page = match installer::prepare_friendly(Some(wizard.numbers[wizard.selected]+1)) {
                    Ok(plan) => {
                        crate::debug::write_str("Setup: review ready\n");
                        Page::Review(plan)
                    }
                    Err(error) => Page::Failed(error.message()),
                };
                drain_input(&mut buttons);
            }
            // A different key or the explicit button confirms the destructive step.
            (Page::Review(_), Some(Key::Char('i' | 'I'))) => {
                wizard.page = match installer::confirm_with_progress(|done, total| {
                    wizard.page = Page::Installing(done, total);
                    wizard.render();
                }) {
                    Ok(()) => {
                        crate::debug::write_str("Setup: installation complete\n");
                        Page::Done
                    }
                    Err(error) => Page::Failed(error.message()),
                };
                drain_input(&mut buttons);
            }
            (Page::Done, Some(Key::Char('\n'))) => {
                if !crate::power::restart() { wizard.page = Page::Failed("Reset failed. Remove the installation media and restart manually."); }
            }
            _ => (),
        }
        if key.is_some() { wizard.render(); }
        crate::idle::wait_for_pointer();
    }
    installer::cancel();
    graphics::restore();
    let _ = diskfs::use_drive(0, true);
}

fn drain_input(buttons: &mut u8) {
    while crate::keyboard::queued_scancodes() != 0 { let _ = crate::keyboard::poll_key(); }
    *buttons = crate::mouse::discard();
}
