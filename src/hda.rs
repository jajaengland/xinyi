//! Intel HD Audio discovery, analog routing and bounded PCM playback.
use crate::{
    audio_model::{Ring, BYTES},
    interrupts::timer_ticks,
    loader::Frames,
    pci, println,
};
use core::{
    arch::asm,
    ptr,
    sync::atomic::{compiler_fence, Ordering},
};

const GCTL: u32 = 0x08;
const STATESTS: u32 = 0x0e;
const INTCTL: u32 = 0x20;
const ICW: u32 = 0x60;
const IRR: u32 = 0x64;
const ICS: u32 = 0x68;
const FORMAT: u16 = 0x11; // 48 kHz, 16-bit, stereo
const DEPTH: usize = 8;
type Result<T> = core::result::Result<T, &'static str>;

struct Controller {
    base: u32,
    bdf: (u8, u8, u8),
    command: u16,
}

impl Controller {
    fn open(bdf: (u8, u8, u8)) -> Result<Self> {
        let (bus, slot, function) = bdf;
        let bar = pci::read(bus, slot, function, 0x10);
        let base = bar & !15;
        let kind = bar & 7;
        if !matches!(kind, 0 | 4)
            || (kind == 4 && pci::read(bus, slot, function, 0x14) != 0)
            || !(0x8000_0000..=0xffff_c000).contains(&base)
            || base & 0x3fff != 0
        {
            return Err("HD Audio BAR is outside the supported 32-bit PCI window.");
        }
        if crate::multiboot::mmap_range().is_none() {
            return Err("HD Audio requires a firmware memory map.");
        }
        let mut overlaps_ram = false;
        crate::multiboot::for_each_region(|r| {
            if r.kind == 1
                && r.base < base as u64 + 0x4000
                && r.base.saturating_add(r.length) > base as u64
            {
                overlaps_ram = true;
            }
        });
        if overlaps_ram {
            return Err("HD Audio registers overlap usable RAM.");
        }
        let command = pci::read(bus, slot, function, 4) as u16;
        // Disable bus mastering and legacy IRQs before touching firmware state.
        pci::write_command(bus, slot, function, (command & !4) | 0x402);
        let dev = Self { base, bdf, command };
        if dev.read::<u8>(3) != 1 {
            return Err("HD Audio registers did not respond.");
        }
        dev.write(INTCTL, 0u32);
        dev.write(STATESTS, 0x7fffu16);
        dev.write(GCTL, 0u32);
        if !wait(|| dev.read::<u32>(GCTL) & 1 == 0) {
            return Err("HD Audio controller reset timed out.");
        }
        pause(1);
        dev.write(GCTL, 1u32);
        if !wait(|| dev.read::<u32>(GCTL) & 1 != 0) {
            return Err("HD Audio controller did not leave reset.");
        }
        pause(2);
        Ok(dev)
    }

    fn read<T: Copy>(&self, offset: u32) -> T {
        unsafe { ptr::read_volatile((self.base + offset) as *const T) }
    }

    fn write<T>(&self, offset: u32, value: T) {
        unsafe { ptr::write_volatile((self.base + offset) as *mut T, value) }
    }

    fn master(&self, enabled: bool) -> bool {
        let (bus, slot, function) = self.bdf;
        let command = (self.command & !4) | 0x402 | if enabled { 4 } else { 0 };
        pci::write_command(bus, slot, function, command);
        (pci::read(bus, slot, function, 4) & 4 != 0) == enabled
    }

    fn verb(&self, codec: u8, node: u8, verb: u32) -> Result<u32> {
        if !wait(|| self.read::<u16>(ICS) & 1 == 0) {
            return Err("HD Audio command interface stayed busy.");
        }
        self.write(ICS, 2u16);
        self.write(ICW, u32::from(codec) << 28 | u32::from(node) << 20 | verb);
        self.write(ICS, 1u16);
        if !wait(|| self.read::<u16>(ICS) & 3 == 2) {
            self.write(ICS, 0u16);
            return Err("HD Audio codec command timed out.");
        }
        Ok(self.read(IRR))
    }

    fn param(&self, codec: u8, node: u8, parameter: u8) -> Result<u32> {
        self.verb(codec, node, 0xf0000 | u32::from(parameter))
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        let (bus, slot, function) = self.bdf;
        // Keep mastering disabled for this diagnostic-only controller.
        pci::write_command(bus, slot, function, self.command & !4);
    }
}

fn wait(mut ready: impl FnMut() -> bool) -> bool {
    let start = timer_ticks();
    for _ in 0..200_000 {
        if ready() {
            return true;
        }
        if timer_ticks().wrapping_sub(start) >= 5 {
            break;
        }
        core::hint::spin_loop();
    }
    false
}

fn pause(ticks: u32) {
    let start = timer_ticks();
    while timer_ticks().wrapping_sub(start) < ticks {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

fn nodes(value: u32) -> core::ops::Range<usize> {
    let start = ((value >> 16) & 0xff) as usize;
    start.min(128)..(start + (value & 0xff) as usize).min(128)
}

#[derive(Clone, Copy)]
struct Hop {
    node: u8,
    input: u8,
}
const EMPTY_HOP: Hop = Hop { node: 0, input: 0 };

struct Group<'a> {
    dev: &'a Controller,
    codec: u8,
    group: u8,
    caps: [u32; 128],
}

impl Group<'_> {
    fn param(&self, node: u8, id: u8) -> Result<u32> {
        self.dev.param(self.codec, node, id)
    }

    fn verb(&self, node: u8, cmd: u32) -> Result<u32> {
        self.dev.verb(self.codec, node, cmd)
    }

    fn route(
        &self,
        node: u8,
        path: &mut [Hop; DEPTH],
        depth: usize,
        budget: &mut u8,
    ) -> Result<Option<usize>> {
        if depth == DEPTH || *budget == 0 || path[..depth].iter().any(|hop| hop.node == node) {
            return Ok(None);
        }
        *budget -= 1;
        let caps = self.caps[usize::from(node)];
        if caps == u32::MAX || caps & (1 << 9) != 0 {
            return Ok(None);
        }
        let kind = (caps >> 20) & 15;
        path[depth] = Hop { node, input: 0 };
        if kind == 0 && caps & 1 != 0 {
            let pcm = self.param(
                if caps & (1 << 4) != 0 {
                    node
                } else {
                    self.group
                },
                0x0a,
            )?;
            return Ok(if pcm & 0x20040 == 0x20040 {
                Some(depth + 1)
            } else {
                None
            });
        }
        if !matches!(kind, 2 | 3 | 4) || (kind == 4 && depth != 0) || caps & 0x100 == 0 {
            return Ok(None);
        }

        let len = self.param(node, 0x0e)? as u8;
        let long = len & 0x80 != 0;
        let bits = if long { 16 } else { 8 };
        let per_word = 32 / bits;
        let range_bit = 1 << (bits - 1);
        let mut previous = None;
        let mut input = 0u16;
        let mut word = 0;
        for entry in 0..u32::from(len & 0x7f) {
            if entry % per_word == 0 {
                word = self.verb(node, 0xf0200 | entry)?;
            }
            let raw = (word >> ((entry % per_word) * bits)) & (range_bit * 2 - 1);
            let end = raw & (range_bit - 1);
            let first = if raw & range_bit != 0 {
                match previous {
                    Some(prior) if prior < end => prior + 1,
                    _ => return Ok(None),
                }
            } else {
                end
            };
            previous = Some(end);
            for next in first..=end {
                if next < 128 && input <= if caps & 2 != 0 { 15 } else { 255 } {
                    path[depth].input = input as u8;
                    if let Some(length) = self.route(next as u8, path, depth + 1, budget)? {
                        return Ok(Some(length));
                    }
                }
                input += 1;
                if input > 255 || *budget == 0 {
                    return Ok(None);
                }
            }
        }
        Ok(None)
    }

    fn configure(&self, path: &[Hop], pin_caps: u32) -> Result<()> {
        self.verb(self.group, 0x70500)?;
        pause(2);
        for (i, hop) in path.iter().enumerate() {
            let caps = self.caps[usize::from(hop.node)];
            let kind = (caps >> 20) & 15;
            if caps & (1 << 10) != 0 {
                self.verb(hop.node, 0x70500)?;
            }
            if i + 1 < path.len() && kind != 2 {
                self.verb(hop.node, 0x70100 | u32::from(hop.input))?;
            }
            let amp_node = if caps & 8 != 0 { hop.node } else { self.group };
            if caps & 4 != 0 {
                let offset = self.param(amp_node, 0x12)? & 0x7f;
                self.verb(hop.node, 0x3b000 | offset)?;
            }
            if caps & 2 != 0 && i + 1 < path.len() {
                let offset = self.param(amp_node, 0x0d)? & 0x7f;
                self.verb(hop.node, 0x37000 | (u32::from(hop.input) << 8) | offset)?;
            }
        }
        let dac = path[path.len() - 1].node;
        self.verb(dac, 0x20000 | u32::from(FORMAT))?;
        self.verb(dac, 0x70610)?;
        self.verb(path[0].node, 0x70740)?;
        if pin_caps & (1 << 16) != 0 {
            self.verb(path[0].node, 0x70c02)?;
        }
        Ok(())
    }
}

fn output_stream(dev: &Controller) -> Result<u32> {
    let caps = dev.read::<u16>(0);
    if caps >> 12 == 0 {
        return Err("HD Audio has no dedicated output stream.");
    }
    Ok(0x80 + u32::from((caps >> 8) & 15) * 0x20)
}

fn reset_stream(dev: &Controller, stream: u32) -> Result<()> {
    dev.write(stream, 0u8);
    if !wait(|| dev.read::<u8>(stream) & 2 == 0) {
        return Err("HD Audio stream would not stop.");
    }
    dev.write(stream, 1u8);
    if !wait(|| dev.read::<u8>(stream) & 1 != 0) {
        return Err("HD Audio stream reset timed out.");
    }
    dev.write(stream, 0u8);
    if !wait(|| dev.read::<u8>(stream) & 1 == 0) {
        return Err("HD Audio stream remained in reset.");
    }

    Ok(())
}

fn tone(dev: &Controller) -> Result<()> {
    let stream = output_stream(dev)?;
    reset_stream(dev, stream)?;
    const HALF: u32 = 1920;
    let buffer = Frames::new(2).ok_or("Not enough memory for HD Audio DMA.")?;
    let samples = buffer.base() + 4096;
    unsafe {
        let bdl = buffer.base() as *mut u32;
        for index in 0..2 {
            bdl.add(index * 4).write(samples + index as u32 * HALF);
            bdl.add(index * 4 + 2).write(HALF);
        }
        let pcm = samples as *mut i16;
        for frame in 0..(HALF / 2) as usize {
            let phase = (frame % 80) as i16;
            let sample = (20 - (phase - 40).abs()) * 64;
            pcm.add(frame * 2).write(sample);
            pcm.add(frame * 2 + 1).write(sample);
        }
    }
    dev.write(stream + 3, 0x1cu8);
    dev.write(stream + 2, 0x10u8);
    dev.write(stream + 8, HALF * 2);
    dev.write(stream + 0x0c, 1u16);
    dev.write(stream + 0x12, FORMAT);
    dev.write(stream + 0x18, buffer.base());
    dev.write(stream + 0x1c, 0u32);
    compiler_fence(Ordering::Release);
    if !dev.master(true) {
        return Err("Could not enable HD Audio DMA.");
    }
    dev.write(stream, 2u8);

    let mut moved = false;
    let mut fault = false;
    let initial = dev.read::<u32>(stream + 4);
    for _ in 0..50 {
        pause(1);
        moved |= dev.read::<u32>(stream + 4) != initial;
        if dev.read::<u8>(stream + 3) & 0x18 != 0 {
            fault = true;
            break;
        }
    }
    dev.write(stream, 0u8);
    let stopped = wait(|| dev.read::<u8>(stream) & 2 == 0);
    let mastering_off = dev.master(false);
    if stopped {
        dev.write(stream + 0x18, 0u32);
    }
    if !stopped && !mastering_off {
        // Do not return memory to the allocator while DMA may still target it.
        core::mem::forget(buffer);
    }
    if !stopped || !mastering_off || fault || !moved {
        return Err("HD Audio DMA did not complete cleanly.");
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Route {
    codec: u8,
    group: u8,
    path: [Hop; DEPTH],
    length: usize,
    pin_caps: u32,
    speaker: bool,
}

struct Discovery {
    ids: [u32; 15],
    route: Option<Route>,
}

impl Discovery {
    fn report(&self) {
        for (address, id) in self.ids.iter().copied().enumerate() {
            if id == 0 {
                continue;
            }
            println!(
                "HD Audio codec {}: {:04x}:{:04x}",
                address,
                id >> 16,
                id as u16
            );
            if let Some(route) = self
                .route
                .filter(|route| usize::from(route.codec) == address)
            {
                println!(
                    "  {} pin {:02x}, converter {:02x}",
                    if route.speaker { "Speaker" } else { "Line-out" },
                    route.path[0].node,
                    route.path[route.length - 1].node
                );
            }
        }
    }
}

fn group(dev: &Controller, codec: u8, group: u8) -> Result<(Group<'_>, core::ops::Range<usize>)> {
    let mut graph = Group {
        dev,
        codec,
        group,
        caps: [u32::MAX; 128],
    };
    let widgets = nodes(graph.param(group, 4)?);
    for node in widgets.clone() {
        graph.caps[node] = graph.param(node as u8, 9)?;
    }
    Ok((graph, widgets))
}

fn discover(dev: &Controller) -> Result<Discovery> {
    let mask = dev.read::<u16>(STATESTS) & 0x7fff;
    if mask == 0 {
        return Err("HD Audio reported no attached codecs.");
    }
    let mut discovery = Discovery {
        ids: [0; 15],
        route: None,
    };
    for codec in 0..15 {
        if mask & (1 << codec) == 0 {
            continue;
        }
        let Ok(id) = dev.param(codec, 0, 0) else {
            continue;
        };
        if id == 0 || id == u32::MAX {
            continue;
        }
        discovery.ids[usize::from(codec)] = id;
        if discovery.route.is_some_and(|route| route.speaker) {
            continue;
        }
        // A secondary codec may be present but not usable. Keep any analog
        // route already found on another codec.
        if let Ok(Some(route)) = find_route(dev, codec) {
            if discovery.route.is_none() || route.speaker {
                discovery.route = Some(route);
            }
        }
    }
    if discovery.ids.iter().all(|&id| id == 0) {
        return Err("No HD Audio codec answered.");
    }
    Ok(discovery)
}

fn find_route(dev: &Controller, codec: u8) -> Result<Option<Route>> {
    let mut line_out = None;
    for af in nodes(dev.param(codec, 0, 4)?) {
        if dev.param(codec, af as u8, 5)? & 0xff != 1 {
            continue;
        }
        let (graph, widgets) = group(dev, codec, af as u8)?;
        for node in widgets {
            if (graph.caps[node] >> 20) & 15 != 4 {
                continue;
            }
            let pin_caps = graph.param(node as u8, 0x0c)?;
            let config = graph.verb(node as u8, 0xf1c00)?;
            let output = (config >> 20) & 15;
            if pin_caps & 0x10 == 0 || (config >> 30) == 1 || !matches!(output, 0 | 1) {
                continue;
            }
            let mut path = [EMPTY_HOP; DEPTH];
            if let Some(length) = graph.route(node as u8, &mut path, 0, &mut 128)? {
                let route = Route {
                    codec,
                    group: af as u8,
                    path,
                    length,
                    pin_caps,
                    speaker: output == 1,
                };
                if route.speaker {
                    return Ok(Some(route));
                }
                if line_out.is_none() {
                    line_out = Some(route);
                }
            }
        }
    }
    Ok(line_out)
}

fn configure(dev: &Controller, route: Route) -> Result<()> {
    let (graph, _) = group(dev, route.codec, route.group)?;
    graph.configure(&route.path[..route.length], route.pin_caps)
}

pub fn run(bdf: (u8, u8, u8), play: bool) -> Result<()> {
    let dev = Controller::open(bdf)?;
    let discovery = discover(&dev)?;
    discovery.report();
    if play {
        let route = discovery
            .route
            .ok_or("No supported analog speaker or line-out route was found.")?;
        let result = configure(&dev, route).and_then(|()| tone(&dev));
        let _ = dev.verb(route.codec, route.path[0].node, 0x70700);
        let _ = dev.verb(route.codec, route.path[route.length - 1].node, 0x70600);
        result?;
        println!("HD Audio test tone completed.");
    }
    Ok(())
}

// Fixed-size cyclic DMA periods. The software queue can still accept writes
// of any supported length without changing live hardware descriptors.
const PERIODS: usize = 4;
const PERIOD_FRAMES: usize = 2048;
const PERIOD_BYTES: usize = PERIOD_FRAMES * 4;
const CYCLE_BYTES: u32 = (PERIODS * PERIOD_BYTES) as u32;
const CLOCK_LIMIT: u32 = 24_000 * 125;

pub struct Pcm {
    dev: Controller,
    discovery: Discovery,
    stream: u32,
    dma: Frames,
    pending: [u16; PERIODS],
    draining: u16,
    source_offset: usize,
    period: usize,
    last_clock: u32,
    running: bool,
    usable: bool,
}

impl Pcm {
    pub fn open(bdf: (u8, u8, u8)) -> Result<Self> {
        let dev = Controller::open(bdf)?;
        let discovery = discover(&dev)?;
        let route = discovery
            .route
            .ok_or("No supported analog speaker or line-out route was found.")?;
        let stream = output_stream(&dev)?;
        let dma = Frames::new(1 + PERIODS * PERIOD_BYTES / 4096)
            .ok_or("Not enough memory for HD Audio DMA.")?;
        configure(&dev, route)?;
        crate::debug::write_fmt(format_args!(
            "Audio: HD Audio PCM ready codec={:08x} pin={:02x} converter={:02x}\n",
            discovery.ids[usize::from(route.codec)],
            route.path[0].node,
            route.path[route.length - 1].node
        ));
        let mut pcm = Self {
            dev,
            discovery,
            stream,
            dma,
            pending: [0; PERIODS],
            draining: 0,
            source_offset: 0,
            period: 0,
            last_clock: 0,
            running: false,
            usable: true,
        };
        if !pcm.reset() || !pcm.dev.master(true) {
            return Err("Could not initialize HD Audio playback.");
        }
        Ok(pcm)
    }

    pub fn codec_ids(&self) -> [u32; 15] { self.discovery.ids }

    pub fn report(&self) {
        self.discovery.report();
    }

    pub fn test(&mut self) -> Result<()> {
        self.report();
        let result = tone(&self.dev);
        if !self.reset() || !self.dev.master(true) {
            self.usable = false;
            return Err("Could not restore HD Audio playback after the test.");
        }
        result?;
        println!("HD Audio test tone completed.");
        Ok(())
    }

    fn stop(&mut self) -> bool {
        self.dev.write(self.stream, 0u8);
        self.running = false;
        if wait(|| self.dev.read::<u8>(self.stream) & 2 == 0) {
            return true;
        }
        self.usable = false;
        self.dev.master(false);
        false
    }

    pub fn reset(&mut self) -> bool {
        if !self.usable || !self.stop() {
            return false;
        }
        if reset_stream(&self.dev, self.stream).is_err() {
            self.usable = false;
            self.dev.master(false);
            return false;
        }
        self.pending.fill(0);
        self.draining = 0;
        self.source_offset = 0;
        self.period = 0;
        true
    }

    pub fn queued(&self, ring: &Ring) -> u32 {
        ring.frames - self.source_offset as u32
            + u32::from(self.draining)
            + self.pending.iter().map(|&n| u32::from(n)).sum::<u32>()
    }

    fn render(&mut self, period: usize, ring: &mut Ring, source: u32, volume: u32) {
        let target = (self.dma.base() + 4096 + (period * PERIOD_BYTES) as u32) as *mut i16;
        let mut written = 0;
        while written < PERIOD_FRAMES {
            let Some(length) = ring.front() else { break };
            let count = (length - self.source_offset).min(PERIOD_FRAMES - written);
            let samples =
                (source + (ring.head * BYTES + self.source_offset * 4) as u32) as *const i16;
            unsafe {
                let dest = target.add(written * 2);
                if volume == 100 {
                    ptr::copy_nonoverlapping(samples, dest, count * 2);
                } else {
                    for i in 0..count * 2 {
                        dest.add(i)
                            .write((i32::from(samples.add(i).read()) * volume as i32 / 100) as i16);
                    }
                }
            }
            written += count;
            self.source_offset += count;
            if self.source_offset == length {
                ring.consume_front();
                self.source_offset = 0;
            }
        }
        unsafe {
            ptr::write_bytes(target.add(written * 2), 0, (PERIOD_FRAMES - written) * 2);
        }
        self.pending[period] = written as u16;
        compiler_fence(Ordering::Release);
    }

    fn start(&mut self, ring: &mut Ring, source: u32, volume: u32) {
        let base = self.dma.base();
        for period in 0..PERIODS {
            self.render(period, ring, source, volume);
            let descriptor = (base + period as u32 * 16) as *mut u32;
            unsafe {
                descriptor.write(base + 4096 + (period * PERIOD_BYTES) as u32);
                descriptor.add(2).write(PERIOD_BYTES as u32);
            }
        }
        self.dev.write(self.stream + 3, 0x1cu8);
        self.dev.write(self.stream + 2, 0x10u8);
        self.dev.write(self.stream + 8, CYCLE_BYTES);
        self.dev.write(self.stream + 0x0c, (PERIODS - 1) as u16);
        self.dev.write(self.stream + 0x12, FORMAT);
        self.dev.write(self.stream + 0x18, base);
        self.dev.write(self.stream + 0x1c, 0u32);
        compiler_fence(Ordering::Release);
        self.last_clock = self.dev.read(0x30);
        self.period = 0;
        self.dev.write(self.stream, 2u8);
        self.running = true;
    }

    pub fn service(&mut self, ring: &mut Ring, source: u32, volume: u32) -> bool {
        if !self.usable {
            return false;
        }
        if !self.running {
            if ring.count != 0 {
                if reset_stream(&self.dev, self.stream).is_err() {
                    return false;
                }
                self.start(ring, source, volume);
            }
            return true;
        }
        let now = self.dev.read::<u32>(0x30);
        let position = self.dev.read::<u32>(self.stream + 4);
        let period = position as usize / PERIOD_BYTES;
        let advanced = (period + PERIODS - self.period) % PERIODS;
        // If a poll was missed, the position might have wrapped. Stop instead
        // of writing a period the controller may still be reading.
        if now.wrapping_sub(self.last_clock) >= CLOCK_LIMIT
            || position >= CYCLE_BYTES
            || advanced > 2
            || self.dev.read::<u8>(self.stream + 3) & 0x18 != 0
        {
            self.stop();
            return false;
        }
        self.last_clock = now;
        while self.period != period {
            // Link position can advance before the codec has played its
            // buffered samples. Keep one completed period in the drain count.
            self.draining = self.pending[self.period];
            self.render(self.period, ring, source, volume);
            self.period = (self.period + 1) % PERIODS;
        }
        if self.queued(ring) == 0 {
            return self.stop();
        }
        true
    }
}
