use super::*;
use common_commodore_c128::{C128 as Raw, C128Model, C128Roms};
fn roms() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut k = vec![0xea; 0x4000];
    k[0x1000..0x1009].copy_from_slice(&[1, 5, 0xd5, 0x3e, 1, 0xed, 0x79, 0x18, 0xfe]);
    let mut p = vec![0x78];
    for (a, v) in [
        (0x400u16, 0x42),
        (0xd020, 6),
        (0xd011, 0x1b),
        (0xd400, 0x80),
        (0xd401, 0x20),
        (0xd405, 0),
        (0xd406, 0xf0),
        (0xd404, 0x21),
        (0xd418, 15),
    ] {
        p.extend([0xa9, v, 0x8d, a as u8, (a >> 8) as u8]);
    }
    let end = 0xe000 + p.len() as u16;
    p.extend([0x4c, end as u8, (end >> 8) as u8]);
    k[0x2000..0x2000 + p.len()].copy_from_slice(&p);
    k[0x3ffc..0x3ffe].copy_from_slice(&[0, 0xe0]);
    (k, vec![0; 0x4000], vec![0; 0x4000], vec![0xaa; 0x2000])
}
fn with_roms<T>(f: impl FnOnce(&C128Roms<'_>) -> T) -> T {
    let (k, l, h, c) = roms();
    f(&C128Roms {
        kernal: &k,
        basic_lo: &l,
        basic_hi: &h,
        chargen: &c,
    })
}
fn machine() -> C128 {
    with_roms(|r| C128::new(r).expect("ROMs"))
}
#[test]
fn fixed_vic_and_clock_agree_with_the_running_board() {
    let m = machine();
    assert_eq!(m.model(), Configuration::MODEL);
    assert_eq!(
        u32::from(m.vic().model().lines_per_frame()) * u32::from(m.vic().model().cycles_per_line()),
        m.timing().cycles_per_frame
    );
    assert_eq!(m.vdc_ram().len(), 0x4000);
}
#[test]
fn bios_hands_the_bus_to_the_8502_and_runs_the_reset_vector() {
    let mut m = machine();
    assert!(!m.running_8502());
    for _ in 0..400 {
        m.tick();
    }
    assert!(m.running_8502());
    assert_eq!(m.ram(0x400), 0x42);
}
#[test]
fn malformed_firmware_is_rejected() {
    with_roms(|r| {
        let bad = C128Roms {
            kernal: &r.kernal[..0x2000],
            basic_lo: r.basic_lo,
            basic_hi: r.basic_hi,
            chargen: r.chargen,
        };
        assert!(C128::new(&bad).is_err());
    });
}
#[test]
fn native_fields_count_the_same_cycles_as_the_oscillator() {
    let mut m = machine();
    let before = m.phi2_cycles();
    let a = m.run_frame();
    let b = m.run_frame();
    assert_eq!(m.phi2_cycles() - before, a + b);
    assert_eq!(m.frame_count(), 2);
    assert_eq!(b, u64::from(m.timing().cycles_per_frame));
}
#[test]
fn guest_palette_write_reaches_the_completed_picture() {
    let mut m = machine();
    m.run_frame();
    assert_eq!(m.vic_register(0x20) & 15, 6);
    assert!(m.framebuffer().iter().any(|p| *p & 0xffffff != 0));
}
#[test]
fn guest_sid_voice_reaches_the_audio_mix() {
    let mut m = machine();
    m.run_frame();
    assert!(m.take_audio_buffer().iter().any(|x| x.abs() > 0.0001));
}
#[test]
fn partial_z80_boot_restores_exactly_and_foreign_board_is_refused() {
    let mut m = machine();
    m.keyboard_mut().set_key(1, 2, true);
    m.set_80_column_key(true);
    for _ in 0..37 {
        m.tick();
    }
    let mut restored = C128::from_state(m.snapshot_state()).expect("same owner");
    for _ in 0..400 {
        assert_eq!(m.tick(), restored.tick());
    }
    assert_eq!(m.cpu().regs.pc, restored.cpu().regs.pc);
    assert_eq!(m.z80().regs.pc, restored.z80().regs.pc);
    assert_eq!(m.ram(0x400), restored.ram(0x400));
    assert_eq!(m.vic_register(0x2f), restored.vic_register(0x2f));
    assert_eq!(m.framebuffer(), restored.framebuffer());
    assert_eq!(m.take_audio_buffer(), restored.take_audio_buffer());
    with_roms(|r| {
        let other = if Configuration::MODEL == C128Model::Pal {
            C128Model::Ntsc
        } else {
            C128Model::Pal
        };
        let foreign = Raw::with_model(r, other).expect("foreign");
        assert!(C128::from_state(foreign).is_err());
    });
}
