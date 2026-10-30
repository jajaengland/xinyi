// PCI configuration mechanism #1. Drivers may enable I/O/bus mastering on a matched device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdeChannel {
    pub io: u16,
    pub control: u16,
}
impl IdeChannel {
    pub const PRIMARY: Self = Self {
        io: 0x1f0,
        control: 0x3f6,
    };
    pub const SECONDARY: Self = Self {
        io: 0x170,
        control: 0x376,
    };
}
#[derive(Clone, Copy)]
pub struct StorageController {
    pub bus: u8,
    pub slot: u8,
    pub function: u8,
    pub vendor: u16,
    pub device: u16,
    pub subclass: u8,
    pub interface: u8,
    pub command: u16,
    pub bars: [u32; 4],
    pub mapping: u32,
}
pub fn read(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    let address = 0x80000000u32
        | (bus as u32) << 16
        | (slot as u32) << 11
        | (function as u32) << 8
        | (offset as u32 & 0xfc);
    let value;
    unsafe {
        core::arch::asm!("out dx, eax",in("dx") 0xcf8u16,in("eax") address,options(nomem,nostack,preserves_flags));
        core::arch::asm!("in eax, dx",in("dx") 0xcfcu16,out("eax") value,options(nomem,nostack,preserves_flags));
    }
    value
}
pub fn for_each_storage(mut visit: impl FnMut(StorageController)) {
    for bus in 0u16..256 {
        for slot in 0..32 {
            if read(bus as u8, slot, 0, 0) as u16 == 0xffff {
                continue;
            }
            let functions = if read(bus as u8, slot, 0, 0x0c) & 0x00800000 != 0 {
                8
            } else {
                1
            };
            for function in 0..functions {
                let id = read(bus as u8, slot, function, 0);
                if id as u16 == 0xffff {
                    continue;
                }
                let class = read(bus as u8, slot, function, 8);
                if class >> 24 != 1 {
                    continue;
                }
                let mut bars = [0; 4];
                for (i, bar) in bars.iter_mut().enumerate() {
                    *bar = read(bus as u8, slot, function, 0x10 + i as u8 * 4);
                }
                visit(StorageController {
                    bus: bus as u8,
                    slot,
                    function,
                    vendor: id as u16,
                    device: (id >> 16) as u16,
                    subclass: (class >> 16) as u8,
                    interface: (class >> 8) as u8,
                    command: read(bus as u8, slot, function, 4) as u16,
                    bars,
                    mapping: read(bus as u8, slot, function, 0x90),
                });
            }
        }
    }
}
// Native IDE uses PCI I/O BARs; the control register is BAR1/BAR3 + 2.
pub fn ide_channel(
    interface: u8,
    command: u16,
    bars: [u32; 4],
    secondary: bool,
) -> Option<IdeChannel> {
    if command & 1 == 0 {
        return None;
    }
    let native = interface & if secondary { 4 } else { 1 } != 0;
    if !native {
        return Some(if secondary {
            IdeChannel::SECONDARY
        } else {
            IdeChannel::PRIMARY
        });
    }
    let i = if secondary { 2 } else { 0 };
    let io = bars[i] & !3;
    let control = bars[i + 1] & !3;
    if bars[i] & 1 == 0
        || bars[i + 1] & 1 == 0
        || io == 0
        || control == 0
        || io > 0xfff8
        || control > 0xfffc
        || io & 7 != 0
    {
        return None;
    }
    Some(IdeChannel {
        io: io as u16,
        control: control as u16 + 2,
    })
}
pub fn primary_channel() -> Option<IdeChannel> {
    let mut first = None;
    let mut ide_seen = false;
    let mut storage_seen = false;
    for_each_storage(|controller| {
        storage_seen = true;
        if controller.subclass == 1 && !ide_seen {
            ide_seen = true;
            first = ide_channel(
                controller.interface,
                controller.command,
                controller.bars,
                false,
            );
        }
    });
    if !storage_seen {
        Some(IdeChannel::PRIMARY)
    } else {
        first
    }
}
pub fn default_target() -> Option<crate::disk_target::Target> {
    let mut first = None;
    let mut storage_seen = false;
    for_each_storage(|c| {
        storage_seen = true;
        if c.subclass == 1 && first.is_none() {
            first = Some(crate::disk_target::Target {
                bdf: (c.bus as u32) << 16 | (c.slot as u32) << 8 | c.function as u32,
                id: (c.device as u32) << 16 | c.vendor as u32,
                position: 0,
            });
        }
    });
    if !storage_seen {
        Some(crate::disk_target::Target::legacy(false, false))
    } else {
        first
    }
}
pub fn target_channel(target: crate::disk_target::Target) -> Option<IdeChannel> {
    if !target.valid() {
        return None;
    }
    if target.bdf == u32::MAX {
        return Some(if target.secondary() {
            IdeChannel::SECONDARY
        } else {
            IdeChannel::PRIMARY
        });
    }
    let (bus, slot, function) = (
        (target.bdf >> 16) as u8,
        (target.bdf >> 8) as u8,
        target.bdf as u8,
    );
    if read(bus, slot, function, 0) != target.id {
        return None;
    }
    let class = read(bus, slot, function, 8);
    if class >> 16 != 0x0101 {
        return None;
    }
    let mut bars = [0; 4];
    for (i, bar) in bars.iter_mut().enumerate() {
        *bar = read(bus, slot, function, 0x10 + i as u8 * 4);
    }
    ide_channel(
        (class >> 8) as u8,
        read(bus, slot, function, 4) as u16,
        bars,
        target.secondary(),
    )
}

// Write only the command halfword; never echo write-one-to-clear status bits.
pub fn write_command(bus:u8,slot:u8,function:u8,command:u16) {
    let address=0x80000000u32|(bus as u32)<<16|(slot as u32)<<11|(function as u32)<<8|4;
    unsafe {core::arch::asm!("out dx,eax",in("dx")0xcf8u16,in("eax")address,options(nomem,nostack));
        core::arch::asm!("out dx,ax",in("dx")0xcfcu16,in("ax")command,options(nomem,nostack));}
}
