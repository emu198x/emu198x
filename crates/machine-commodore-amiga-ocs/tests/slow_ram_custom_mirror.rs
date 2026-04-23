use machine_commodore_amiga_ocs::AmigaOcs;

fn blank_kickstart() -> Vec<u8> {
    vec![0u8; 256 * 1024]
}

#[test]
fn absent_slow_ram_hole_mirrors_custom_registers() {
    let mut amiga = AmigaOcs::new(blank_kickstart());

    amiga.poke_word(0x00C3_F09A, 0xBFFF);

    assert_eq!(amiga.read_word(0x00C3_F01C), 0x3FFF);
    assert_eq!(amiga.read_word(0x00DF_F01C), 0x3FFF);
}

#[test]
fn installed_a501_slow_ram_masks_custom_mirror_until_ram_ends() {
    let mut amiga = AmigaOcs::with_slow_ram(blank_kickstart(), 512 * 1024);

    amiga.poke_word(0x00C7_F09A, 0x1357);
    assert_eq!(amiga.read_word(0x00C7_F09A), 0x1357);
    assert_eq!(amiga.read_word(0x00DF_F01C), 0x0000);

    amiga.poke_word(0x00CB_F09A, 0xBFFF);
    assert_eq!(amiga.read_word(0x00CB_F01C), 0x3FFF);
    assert_eq!(amiga.read_word(0x00DF_F01C), 0x3FFF);
}
