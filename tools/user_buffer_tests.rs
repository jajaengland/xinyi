#[path = "../src/user_buffer.rs"] mod user_buffer;

#[test]
fn large_fixed_requests_still_require_a_single_valid_user_range() {
    use user_buffer::{readable, readable_range};
    assert!(!readable(0x403fc000,3468,65536));
    assert!(readable_range(0x403fc000,3468,65536));
    assert!(readable_range(0x40000000,3468,3468));
    assert!(!readable_range(0x40000000,3468,3467));
    assert!(!readable_range(0x403ffff0,3468,65536));
    assert!(!readable_range(0x4000fff0,3468,65536));
    assert!(!readable_range(0xfffffff0,3468,65536));
    assert!(!readable_range(0x100000,3468,65536));
}
#[test]
fn file_outputs_accept_only_writable_stack_ranges() {
    use user_buffer::writable;
    assert!(writable(0x403fc000,1024));assert!(writable(0x403fffff,1));
    for address in [0,0x100000,0xb8000,0x40000000,0x403fbfff,0x40400000,0xffffffff] {assert!(!writable(address,1));}
    assert!(!writable(0x403fffff,2));assert!(writable(0xffffffff,0));
}
#[test]
fn console_ranges_accept_only_complete_image_or_stack_buffers() {
    use user_buffer::readable;
    assert!(readable(0x40000000, 32, 32));
    assert!(readable(0x4000001f, 1, 32));
    assert!(!readable(0x4000001f, 2, 32));
    assert!(!readable(0x40000020, 1, 32));
    assert!(readable(0x403fc000, 1024, 32));
    assert!(readable(0x403fffff, 1, 32));
    assert!(!readable(0x403fffff, 2, 32));
    for address in [0, 0x100000, 0xb8000, 0x40010000, 0x403fbfff, 0x40400000, 0xffffffff] {
        assert!(!readable(address, 1, 32));
    }
    assert!(!readable(0xfffffff0, 32, 32));
    assert!(!readable(0x403fc000, 1025, 32));
    assert!(!readable(0x40000000, 1, 0));
    assert!(!readable(0x40000000, 1, 65537));
    assert!(readable(0xffffffff, 0, 32));
}
