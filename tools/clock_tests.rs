#[path = "../src/clock_model.rs"] mod clock_model;
use clock_model::{Clock, PM_HZ, pm_timer};

#[test] fn elapsed_time_does_not_depend_on_interrupt_or_sampling_rate() {
    let mut idle = Clock::new(0, 0x00ff_ffff);
    let mut busy = Clock::new(0, 0x00ff_ffff);
    for step in 1..=10000 {
        let hardware = (PM_HZ * step / 10000) as u32;
        let ticks = busy.sample(hardware);
        if step % 100 == 0 { assert_eq!(idle.sample(hardware), ticks); }
    }
    assert_eq!(busy.sample(PM_HZ as u32), 100);
    for _ in 0..1000 { assert_eq!(busy.sample(PM_HZ as u32), 100); }
}

#[test] fn fractional_ticks_and_24_or_32_bit_wrap_are_preserved() {
    for mask in [0x00ff_ffff, u32::MAX] {
        let start = mask - 1234;
        let mut clock = Clock::new(start, mask);
        for step in 1..=100 {
            let value = start.wrapping_add(((PM_HZ * step + 99) / 100) as u32) & mask;
            assert_eq!(clock.sample(value), step as u32);
        }
    }
}

fn fadt() -> [u8; 220] {
    let mut table = [0; 220];
    table[76..80].copy_from_slice(&0x608u32.to_le_bytes());
    table[91] = 4;
    table
}

#[test] fn timer_source_checks_fadt_bounds_flags_and_io_registers() {
    let mut table = fadt();
    assert_eq!(pm_timer(&table[..115]), None);
    assert_eq!(pm_timer(&table), Some((0x608, 0x00ff_ffff)));
    table[112..116].copy_from_slice(&(1u32 << 8).to_le_bytes());
    assert_eq!(pm_timer(&table), Some((0x608, u32::MAX)));
    table[112..116].copy_from_slice(&(1u32 << 20).to_le_bytes());
    assert_eq!(pm_timer(&table), None);
    table = fadt();table[91] = 0;
    assert_eq!(pm_timer(&table), None);
    table[208] = 1;table[209] = 32;table[211] = 3;
    table[212..220].copy_from_slice(&0x608u64.to_le_bytes());
    assert_eq!(pm_timer(&table), Some((0x608, 0x00ff_ffff)));
    table[208] = 0;assert_eq!(pm_timer(&table), None);
    table = fadt();table[76..80].copy_from_slice(&65535u32.to_le_bytes());
    assert_eq!(pm_timer(&table), None);
}
