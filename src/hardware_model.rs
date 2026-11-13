//! Read-only hardware inventory shared by the kernel and the Hardware app.
pub const CAPACITY: usize = 64;
pub const NO_AUDIO: u32 = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Device {
    pub bus: u8,
    pub slot: u8,
    pub function: u8,
    pub class: u8,
    pub subclass: u8,
    pub interface: u8,
    pub vendor: u16,
    pub device: u16,
    pub subsystem_vendor: u16,
    pub subsystem_device: u16,
    pub ahci: u8, // 0 not AHCI, 1 register status unavailable, 2 ports sampled
    pub reserved: u8,
    pub ports: [u8; 32],
}

impl Device {
    pub const EMPTY: Self = Self {
        bus: 0,
        slot: 0,
        function: 0,
        class: 0,
        subclass: 0,
        interface: 0,
        vendor: 0,
        device: 0,
        subsystem_vendor: 0,
        subsystem_device: 0,
        ahci: 0,
        reserved: 0,
        ports: [0; 32],
    };

    pub fn address(self) -> u32 {
        (u32::from(self.bus) << 8) | (u32::from(self.slot) << 3) | u32::from(self.function)
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub count: u32,
    pub truncated: u32,
    pub audio: u32,
    pub width: u16,
    pub height: u16,
    pub display: u8, // 0 inactive, 1 Bochs, 2 VESA
    pub storage: u8,
    pub mouse: u8,
    pub network: u8,
    pub devices: [Device; CAPACITY],
}

impl Snapshot {
    pub const fn new() -> Self {
        Self {
            count: 0,
            truncated: 0,
            audio: NO_AUDIO,
            width: 0,
            height: 0,
            display: 0,
            storage: 0,
            mouse: 0,
            network: 0,
            devices: [Device::EMPTY; CAPACITY],
        }
    }
}

pub fn report_path(index: u32) -> [u8; 13] {
    if index == 0 {
        return *b"/HARDWARE.TXT";
    }
    let mut path = *b"/HARDWR00.TXT";
    path[7] += (index / 10) as u8;
    path[8] += (index % 10) as u8;
    path
}

// Port symbols keep a controller's whole status visible in a single report row.
pub fn port_map(ports: &[u8; 32]) -> [u8; 32] {
    ports.map(|state| match state {
        0 => b'-',
        1 => b'.',
        2 => b'd',
        3 => b'S',
        4 => b'A',
        5 => b'?',
        6 => b'x',
        _ => b'?',
    })
}
