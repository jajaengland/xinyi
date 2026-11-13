#![no_std]
#![no_main]
#[path = "../../src/hardware_model.rs"]
mod hardware;
#[path = "../../sdk/xinyi.rs"]
mod xinyi;

use core::fmt::{self, Write};
use hardware::{Device, Snapshot};
use xinyi::{
    gui, key,
    widgets::{Button, Rect},
    window::{self, Draw},
};

const GROUPS: [&[u8]; 7] = [
    b"Audio",
    b"Display",
    b"Input",
    b"Storage",
    b"USB",
    b"Network",
    b"Other PCI devices",
];
const VISIBLE: usize = 11;
const TOP: i32 = 52;
const STEP: i32 = 24;

#[derive(Clone, Copy)]
enum Row {
    Group(usize),
    Summary(usize),
    Pci(usize),
}

fn group(device: Device) -> usize {
    match (device.class, device.subclass) {
        (4, 1 | 3) => 0,
        (3, _) => 1,
        (9, _) => 2,
        (1, _) => 3,
        (12, 3) => 4,
        (2, _) => 5,
        _ => 6,
    }
}

fn device_name(device: Device) -> &'static [u8] {
    match (device.class, device.subclass, device.interface) {
        (4, 3, _) => b"HD Audio controller",
        (4, 1, _) => b"Multimedia audio controller",
        (3, _, _) => b"Display controller",
        (9, _, _) => b"Input controller",
        (1, 1, _) => b"IDE controller",
        (1, 6, 1) => b"AHCI controller",
        (1, 8, 2) => b"NVMe controller",
        (1, _, _) => b"Storage controller",
        (12, 3, 0) => b"UHCI USB controller",
        (12, 3, 0x10) => b"OHCI USB controller",
        (12, 3, 0x20) => b"EHCI USB controller",
        (12, 3, 0x30) => b"xHCI USB controller",
        (12, 3, _) => b"USB controller",
        (2, 0, _) => b"Ethernet controller",
        (2, _, _) => b"Network controller",
        (6, _, _) => b"PCI bridge",
        _ => b"PCI device",
    }
}

struct Text {
    bytes: [u8; 112],
    len: usize,
}
impl Text {
    fn new() -> Self {
        Self {
            bytes: [0; 112],
            len: 0,
        }
    }
    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
impl Write for Text {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len + s.len();
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

struct Browser {
    inventory: Snapshot,
    expanded: u8,
    selected: usize,
    offset: usize,
    rows: [Row; hardware::CAPACITY + GROUPS.len() * 2],
    len: usize,
    save: Button,
    extended: Button,
}
impl Browser {
    fn rebuild(&mut self) {
        self.len = 0;
        for category in 0..GROUPS.len() {
            self.rows[self.len] = Row::Group(category);
            self.len += 1;
            if self.expanded & (1 << category) == 0 {
                continue;
            }
            self.rows[self.len] = Row::Summary(category);
            self.len += 1;
            for (index, device) in self.inventory.devices[..self.inventory.count as usize]
                .iter()
                .enumerate()
            {
                if group(*device) == category {
                    self.rows[self.len] = Row::Pci(index);
                    self.len += 1;
                }
            }
        }
        self.reveal();
    }

    fn reveal(&mut self) {
        self.selected = self.selected.min(self.len - 1);
        self.offset = self.offset.min(self.len.saturating_sub(VISIBLE));
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset + VISIBLE {
            self.offset = self.selected + 1 - VISIBLE;
        }
    }

    fn toggle(&mut self, expand: Option<bool>) {
        match self.rows[self.selected] {
            Row::Group(category) => {
                let bit = 1 << category;
                if expand.unwrap_or(self.expanded & bit == 0) {
                    self.expanded |= bit;
                } else {
                    self.expanded &= !bit;
                }
                self.rebuild();
            }
            _ if expand == Some(false) => {
                while !matches!(self.rows[self.selected], Row::Group(_)) {
                    self.selected -= 1;
                }
                self.reveal();
            }
            _ => (),
        }
    }

    fn summary(&self, category: usize, text: &mut Text) {
        let description = match category {
            0 if self.inventory.audio != hardware::NO_AUDIO => {
                b"Sampled playback active" as &[u8]
            }
            0 => b"No active sampled playback",
            1 => {
                let backend = match self.inventory.display {
                    1 => "Bochs framebuffer",
                    2 => "BIOS VESA framebuffer",
                    _ => "Display mode unavailable",
                };
                let _ = write!(
                    text,
                    "{}  {}x{}",
                    backend, self.inventory.width, self.inventory.height
                );
                return;
            }
            2 if self.inventory.mouse != 0 => b"PS/2 mouse active",
            2 => b"PS/2 mouse unavailable",
            3 if self.inventory.storage == 1 => b"Mounted through BIOS boot drive services",
            3 if self.inventory.storage == 2 => b"Mounted through the native IDE driver",
            3 if self.inventory.storage == 3 => b"Mounted through the native AHCI driver",
            3 => b"No disk volume mounted",
            4 => b"Native USB drivers unavailable",
            5 if self.inventory.network != 0 => b"RTL8139 network driver active",
            5 => b"Networking inactive",
            _ => b"Bridges and other detected devices",
        };
        let _ = text.write_str(core::str::from_utf8(description).unwrap_or(""));
    }

    fn save_report(&self, extended: bool) {
        let volume = xinyi::directory::volume();
        let result = xinyi::syscall(107, if extended { 2 } else { 1 }, volume, 0);
        match result {
            0..=99 => {
                let path = hardware::report_path(result as u32);
                let mut message = *b"Saved /HARDWARE.TXT on the mounted drive.";
                message[6..19].copy_from_slice(&path);
                xinyi::syscall(105, 0, message.as_ptr() as u32, message.len() as u32);
            }
            -17 => {
                gui::report(1, b"All hardware report filenames are in use.");
            }
            -19 => {
                gui::report(2, b"Mount a drive before saving the hardware report.");
            }
            -28 => {
                gui::report(2, b"Not enough free space for the hardware report.");
            }
            -116 => {
                gui::report(2, b"The mounted drive changed. Try saving again.");
            }
            _ => {
                gui::report(
                    2,
                    b"Could not save the hardware report to the mounted drive.",
                );
            }
        }
    }

    fn paint(&self, w: u32) {
        window::draw(
            w,
            &[
                Draw::rect(0, 0, 600, 400, 0xf3f6f8),
                Draw::rect(0, 0, 600, 36, 0x203e52),
                Draw::label(16, 10, 560, 0xffffff, b"Detected hardware"),
                Draw::rect(8, TOP as u32, 584, (VISIBLE as i32 * STEP) as u32, 0xffffff),
            ],
        );
        for (index, &row) in self.rows[..self.len]
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(VISIBLE)
        {
            let y = (TOP + (index - self.offset) as i32 * STEP) as u32;
            let selected = index == self.selected;
            let ink = if selected { 0xffffff } else { 0x20384d };
            if selected {
                window::draw(w, &[Draw::rect(8, y, 584, 24, 0x2b5268)]);
            }
            match row {
                Row::Group(category) => window::draw(
                    w,
                    &[
                        Draw::text(
                            16,
                            y + 4,
                            ink,
                            if self.expanded & (1 << category) != 0 {
                                b"-"
                            } else {
                                b"+"
                            },
                        ),
                        Draw::label(36, y + 4, 532, ink, GROUPS[category]),
                    ],
                ),
                Row::Summary(category) => {
                    let mut text = Text::new();
                    self.summary(category, &mut text);
                    window::draw(w, &[Draw::label(52, y + 4, 520, ink, text.bytes())])
                }
                Row::Pci(index) => {
                    let device = self.inventory.devices[index];
                    let mut text = Text::new();
                    let _ = write!(
                        text,
                        "{:04x}:{:04x}{}",
                        device.vendor,
                        device.device,
                        if device.address() == self.inventory.audio {
                            "  Active"
                        } else {
                            ""
                        }
                    );
                    window::draw(
                        w,
                        &[
                            Draw::label(52, y + 4, 320, ink, device_name(device)),
                            Draw::text(376, y + 4, ink, text.bytes()),
                        ],
                    )
                }
            };
        }
        let mut detail = Text::new();
        let mut port_detail = Text::new();
        if let Row::Pci(index) = self.rows[self.selected] {
            let d = self.inventory.devices[index];
            if d.ahci == 2 {
                let map = hardware::port_map(&d.ports);
                let _ = write!(
                    port_detail,
                    "Ports 0-31: {} (S SATA, A ATAPI, . empty)",
                    core::str::from_utf8(&map).unwrap()
                );
            } else if d.ahci == 1 {
                let _ = port_detail
                    .write_str("AHCI port status unavailable in the current firmware state.");
            }
            let _ = write!(
                detail,
                "PCI {:02x}:{:02x}.{}  Class {:02x}/{:02x}/{:02x}  Subsystem {:04x}:{:04x}",
                d.bus,
                d.slot,
                d.function,
                d.class,
                d.subclass,
                d.interface,
                d.subsystem_vendor,
                d.subsystem_device
            );
        }
        window::draw(
            w,
            &[
                Draw::label(16, 328, 568, 0x20384d, detail.bytes()),
                Draw::label(16, 344, 568, 0x20384d, port_detail.bytes()),
                Draw::label(
                    16,
                    360,
                    424,
                    0x536c80,
                    if self.inventory.truncated != 0 {
                        b"First 64 PCI devices shown. Use arrows or scroll to browse."
                    } else {
                        b"Click or press Enter to expand. Use arrows or scroll to browse."
                    },
                ),
            ],
        );
        gui::button_flat(w, &self.extended, b"Check SATA & Save", false);
        gui::button_flat(w, &self.save, b"Save Report", true);
        window::present(w);
    }
}

fn main() -> u32 {
    let handle = window::open_sized(b"Hardware", 600, 400);
    if handle < 0 {
        return 1;
    }
    let w = handle as u32;
    let mut inventory = Snapshot::new();
    if xinyi::syscall(
        107,
        0,
        &mut inventory as *mut Snapshot as u32,
        core::mem::size_of::<Snapshot>() as u32,
    ) as i32
        != 0
        || inventory.count as usize > hardware::CAPACITY
    {
        xinyi::gui::report(2, b"Hardware inventory is unavailable.");
        let mut event = window::Event::default();
        while window::event(w, &mut event, true) >= 0 {
            if event.kind == window::CLOSE || event.kind == window::RESET {
                break;
            }
        }
        return 1;
    }
    let mut app = Browser {
        inventory,
        expanded: 0,
        selected: 0,
        offset: 0,
        rows: [Row::Group(0); hardware::CAPACITY + GROUPS.len() * 2],
        len: 0,
        save: Button::new(Rect::new(452, 364, 132, 26)),
        extended: Button::new(Rect::new(420, 5, 164, 26)),
    };
    app.rebuild();
    let mut buttons = 0;
    loop {
        app.paint(w);
        loop {
            let mut e = window::Event::default();
            if window::event(w, &mut e, true) < 0 {
                return 1;
            }
            match e.kind {
                window::CLOSE => return 0,
                window::RESET => {
                    buttons = e.buttons;
                    app.save.reset(e.buttons);
                    app.extended.reset(e.buttons);
                    break;
                }
                window::KEY => match e.code as u16 {
                    27 => return 0,
                    83 | 115 if e.code & key::CTRL != 0 => {
                        app.save_report(e.code & key::SHIFT != 0);
                        break;
                    }
                    10 | 32 => {
                        app.toggle(None);
                        break;
                    }
                    key::LEFT => {
                        app.toggle(Some(false));
                        break;
                    }
                    key::RIGHT => {
                        app.toggle(Some(true));
                        break;
                    }
                    key::UP | key::DOWN | key::HOME | key::END | key::PAGE_UP | key::PAGE_DOWN => {
                        app.selected = match e.code as u16 {
                            key::UP => app.selected.saturating_sub(1),
                            key::DOWN => app.selected + 1,
                            key::HOME => 0,
                            key::END => app.len - 1,
                            key::PAGE_UP => app.selected.saturating_sub(VISIBLE),
                            _ => app.selected + VISIBLE,
                        };
                        app.reveal();
                        break;
                    }
                    _ => (),
                },
                window::SCROLL => {
                    let delta = (e.code as i32).clamp(-8, 8);
                    app.selected = (app.selected as i32 + delta * 3).max(0) as usize;
                    app.reveal();
                    break;
                }
                window::POINTER => {
                    let clicked = e.buttons & 1 != 0 && buttons & 1 == 0;
                    buttons = e.buttons;
                    let before = (
                        app.save.hover,
                        app.save.pressed,
                        app.extended.hover,
                        app.extended.pressed,
                    );
                    let save = app.save.pointer(e.x, e.y, e.buttons);
                    let extended = app.extended.pointer(e.x, e.y, e.buttons);
                    if save || extended {
                        app.save_report(extended);
                        break;
                    }
                    if clicked
                        && (8..592).contains(&e.x)
                        && (TOP..TOP + VISIBLE as i32 * STEP).contains(&e.y)
                    {
                        let index = app.offset + ((e.y - TOP) / STEP) as usize;
                        if index < app.len {
                            app.selected = index;
                            if matches!(app.rows[index], Row::Group(_)) {
                                app.toggle(None);
                            }
                            break;
                        }
                    }
                    if before
                        != (
                            app.save.hover,
                            app.save.pressed,
                            app.extended.hover,
                            app.extended.pressed,
                        )
                    {
                        break;
                    }
                }
                _ => (),
            }
        }
    }
}
