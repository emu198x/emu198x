//! Concrete board identity and native CRTC field cadence.
use machine_acorn_bbc_master_128::{BbcMicro, BbcModel};
fn machine() -> BbcMicro {
    let mut mos = vec![0xEA; 16384];
    mos[0x2000..0x2003].copy_from_slice(&[0x4C, 0, 0xE0]);
    mos[0x3FFC..0x3FFE].copy_from_slice(&[0, 0xE0]);
    BbcMicro::new(mos)
}
#[test]
fn fixed_board_constructs_the_right_cpu_and_paged_ram() {
    let mut m = machine();
    assert_eq!(m.model(), BbcModel::Master128);
    assert_eq!(format!("{:?}", m.cpu().variant()), "Cmos");
    m.poke(0xFE30, 0x80);
    m.poke(0x8000, 0xA5);
    assert_eq!(m.peek(0x8000), 165, "board's paged RAM decoding");
}
#[test]
fn native_frames_follow_the_programmed_crtc_in_this_board() {
    let mut m = machine();
    let regs = [127, 80, 98, 0x28, 38, 0, 32, 34, 0, 7, 0x67, 8, 6, 0];
    for (reg, value) in regs.into_iter().enumerate() {
        m.poke(0xFE00, reg as u8);
        m.poke(0xFE01, value);
    }
    m.poke(0xFE20, 0x9C);
    assert!(m.run_native_frame().1);
    let (ticks, complete) = m.run_native_frame();
    assert!(complete);
    assert_eq!(ticks, 128 * 39 * 8);
}
