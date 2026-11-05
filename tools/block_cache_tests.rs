#[path = "../src/block_cache.rs"] mod block_cache;
use block_cache::Cache;
#[test] fn read_through_cache_keeps_alternating_fat_mirrors_and_updates_writes() {
    let mut cache=Cache::new();let mut output=[0;512];
    cache.store(1,&[0x11;512]);cache.store(129,&[0x22;512]);
    for _ in 0..100 {assert!(cache.read(1,&mut output));assert_eq!(output,[0x11;512]);assert!(cache.read(129,&mut output));}
    cache.store(1,&[0x33;512]);assert!(cache.read(1,&mut output));assert_eq!(output,[0x33;512]);
    cache.clear();assert!(!cache.read(1,&mut output));assert!(!cache.read(129,&mut output));
}
#[test] fn replacement_evicts_the_least_recently_used_sector_without_aliasing() {
    let mut cache=Cache::new();let mut output=[0;512];
    for block in 0..block_cache::CAPACITY as u32 {cache.store(block,&[block as u8;512]);}
    assert!(cache.read(0,&mut output));cache.store(block_cache::CAPACITY as u32,&[4;512]);
    assert!(!cache.read(1,&mut output));
    for block in [0,2,3,4] {assert!(cache.read(block,&mut output));assert_eq!(output,[block as u8;512]);}
    assert!(!cache.read(0,&mut [0;511]));cache.store(0,&[7;511]);
    assert!(cache.read(0,&mut output));assert_eq!(output,[0;512]);
}

#[test]fn hot_metadata_survives_streaming_data_without_losing_verified_updates() {
    let mut cache=Cache::new();let mut data=[0;512];
    cache.store(1,&[11;512]);cache.store(129,&[22;512]);
    for sector in 200..400 {
        assert!(cache.read(1,&mut data));assert_eq!(data,[11;512]);
        assert!(cache.read(129,&mut data));assert_eq!(data,[22;512]);
        cache.store(sector,&[sector as u8;512]);
    }
    cache.store(129,&[33;512]);assert!(cache.read(129,&mut data));assert_eq!(data,[33;512]);
    cache.clear();for sector in [1,129,399] {assert!(!cache.read(sector,&mut data));}
}
