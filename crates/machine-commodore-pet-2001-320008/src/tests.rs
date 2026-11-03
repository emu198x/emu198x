use super::*;
fn firmware() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut k = vec![0xea; 4096];
    k[..3].copy_from_slice(&[0x4c, 0, 0xf0]);
    k[0xffc..0xffe].copy_from_slice(&[0, 0xf0]);
    (k, vec![0; 8192], vec![0; 2048], vec![0xff; 2048])
}
fn machine() -> Pet {
    let (k, b, e, c) = firmware();
    Pet::new(k, b, e, c).expect("original board")
}
#[test]
fn original_fitting_has_eight_kib_and_four_screen_aliases() {
    let mut m = machine();
    m.poke(0x1fff, 0x42);
    m.poke(0x2000, 0x93);
    assert_eq!(m.peek(0x1fff), 0x42);
    assert_eq!(m.peek(0x2000), 0xff);
    for address in [0x8000, 0x8400, 0x8800, 0x8c00] {
        m.poke(address, 0x35);
        for alias in [0x8000, 0x8400, 0x8800, 0x8c00] {
            assert_eq!(m.peek(alias), 0x35);
        }
    }
    m.poke(0x8fff, 0x62);
    assert_eq!(m.peek(0x83ff), 0x62);
    m.poke(0x9000, 0x48);
    assert_eq!(m.peek(0x9000), 0xff);
    assert_eq!(m.run_frame(), 16_640);
}
#[test]
fn original_ca2_blanks_white_video_without_a_crtc() {
    let mut m = machine();
    m.poke(0xe811, 0x3c);
    m.run_frame();
    assert!(m.framebuffer().contains(&0xffff_ffff));
    assert!(!m.framebuffer().contains(&0xff00_ff00));
    m.poke(0xe811, 0x34);
    m.run_frame();
    assert!(m.framebuffer().iter().all(|&p| p == 0xff00_0000));
    m.poke(0xe811, 0x3c);
    m.run_frame();
    assert!(m.framebuffer().contains(&0xffff_ffff));
    m.poke(0xe880, 0);
    m.poke(0xe881, 0);
    assert_eq!(
        m.run_frame(),
        16_640,
        "unfitted CRTC registers do not change counters"
    );
}
#[test]
fn foreign_board_and_incorrect_rom_allocations_reject() {
    let (k, b, e, c) = firmware();
    let foreign = common_commodore_pet::Pet::new(
        k.clone(),
        b.clone(),
        e.clone(),
        c.clone(),
        Board::Pet2001n32k,
    );
    assert!(Pet::from_state(foreign).is_err());
    assert!(Pet::new(k, b[..8191].to_vec(), e, c).is_err());
}
#[test]
fn cold_boot_retains_the_concrete_owner_and_firmware() {
    let mut m = machine();
    m.poke(0x1000, 0x51);
    m.run_frame();
    let mut cold = m.cold_boot();
    assert_eq!(cold.peek(0x1000), 0);
    assert_eq!(cold.board(), Board::Pet2001Original8k);
    assert_eq!(cold.run_frame(), 16_640);
}

#[test]
fn guest_observes_floating_holes_screen_and_original_io_aliases() {
    let (mut k, b, e, c) = firmware();
    let code = [
        0xa9, 0x93, 0x8d, 0x00, 0x20, 0xad, 0x00, 0x20, 0x8d, 0x00, 0x02, 0xa9, 0x8f, 0x8d, 0xff,
        0x8f, 0xad, 0xff, 0x83, 0x8d, 0x01, 0x02, 0xa9, 0, 0x8d, 0x23, 0xe9, 0xa9, 0x0f, 0x8d,
        0x22, 0xe9, 0xa9, 4, 0x8d, 0x23, 0xe9, 0xa9, 5, 0x8d, 0x22, 0xe9, 0xa9, 4, 0x8d, 0x21,
        0xe9, 0xad, 0x20, 0xef, 0x8d, 0x02, 0x02, 0xad, 0x80, 0xe8, 0x8d, 0x03, 0x02,
    ];
    k[..code.len()].copy_from_slice(&code);
    let end = 0xf000 + code.len() as u16;
    k[code.len()] = 0x4c;
    k[code.len() + 1..code.len() + 3].copy_from_slice(&end.to_le_bytes());
    let mut m = Pet::new(k, b, e, c).expect("original owner");
    m.run_frame();
    assert_eq!(
        m.peek(0x200),
        0x20,
        "unfitted RAM leaves the last operand bus byte"
    );
    assert_eq!(m.peek(0x201), 0x8f, "four-way screen alias through CPU bus");
    assert_eq!(m.peek(0x202), 0xf5, "original E9/EF PIA aliases");
    assert_eq!(
        m.peek(0x203),
        0xe8,
        "unfitted CRTC floats at last operand byte"
    );
}
