#[path="../src/acpi_sleep.rs"]mod acpi_sleep;
#[test]fn static_s5_packages_are_bounded_and_validate_types(){use acpi_sleep::sleep_types;let p=[8,92,95,83,53,95,0x12,6,2,0x0a,5,0x0a,6];assert_eq!(sleep_types(&p,false),Some((5<<10,6<<10)));for n in 0..p.len(){assert_eq!(sleep_types(&p[..n],false),None);}let mut bad=p;bad[10]=8;assert_eq!(sleep_types(&bad,false),None);let bare=[8,95,83,53,95,0x12,4,2,0,1];assert_eq!(sleep_types(&bare,false),None);assert_eq!(sleep_types(&bare,true),Some((0,1<<10)));assert_eq!(sleep_types(b"not a NameOp _S5_",true),None);}

#[test]
fn pm1_control_prefers_valid_extended_ports() {
    use acpi_sleep::pm1_ports;
    let mut fadt = [0; 196];
    fadt[89] = 2;
    fadt[64..68].copy_from_slice(&0x404u32.to_le_bytes());
    assert_eq!(pm1_ports(&fadt[..116]), Some((0x404, 0)));
    fadt[172..176].copy_from_slice(&[1, 16, 0, 2]);
    fadt[176..184].copy_from_slice(&0x1804u64.to_le_bytes());
    assert_eq!(pm1_ports(&fadt), Some((0x1804, 0)));
    fadt[172] = 0;
    assert_eq!(pm1_ports(&fadt), None);
}

#[test]
fn static_s5_supports_constant_variable_packages_and_single_pm1_block() {
    use acpi_sleep::static_sleep_types;
    let variable = [8, 92, 95, 83, 53, 95, 0x13, 7, 0x0a, 2, 0x0a, 5, 0x0a, 6];
    assert_eq!(static_sleep_types(&variable, false, true), Some((5 << 10, 6 << 10)));
    let single = [8, 92, 95, 83, 53, 95, 0x12, 4, 1, 0x0a, 5];
    assert_eq!(static_sleep_types(&single, false, false), Some((5 << 10, 0)));
    assert_eq!(static_sleep_types(&single, false, true), None);
}
