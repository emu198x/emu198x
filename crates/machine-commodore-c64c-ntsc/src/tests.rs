use super::*;
use common_commodore_c64::{C64 as Raw, C64Config, C64Model};
fn roms() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut k = vec![0xea; 0x2000];
    k[..8].copy_from_slice(&[0xa9, 0x42, 0x8d, 0, 4, 0x4c, 5, 0xe0]);
    k[0x1ffc..0x1ffe].copy_from_slice(&[0, 0xe0]);
    (k, vec![0; 0x2000], vec![0xaa; 0x1000])
}
fn machine() -> C64 {
    let (k, b, c) = roms();
    C64::new(&k, &b, &c).expect("ROMs")
}
#[test]
fn fixed_chips_and_clock_agree_with_the_running_board() {
    let m = machine();
    assert_eq!(m.model(), Configuration::MODEL);
    assert_eq!(m.sid().model, Configuration::MODEL.sid_model());
    assert_eq!(m.cia1().model(), Configuration::MODEL.cia_model());
    assert_eq!(m.cia2().model(), Configuration::MODEL.cia_model());
    assert_eq!(
        u32::from(m.vic().model().lines_per_frame()) * u32::from(m.vic().model().cycles_per_line()),
        m.timing().cycles_per_frame
    );
}
#[test]
fn guest_reset_vector_runs_into_ram() {
    let mut m = machine();
    m.advance_phi2_cycles(200);
    assert_eq!(m.memory().ram_read(0x400), 0x42);
}
#[test]
fn malformed_firmware_is_rejected() {
    let (k, b, c) = roms();
    assert!(C64::new(&k[..0x1000], &b, &c).is_err());
}
#[test]
fn native_fields_count_the_same_cycles_as_the_oscillator() {
    let mut m = machine();
    let before = m.phi2_cycles();
    let a = m.run_frame();
    let b = m.run_frame();
    assert_eq!(m.phi2_cycles() - before, u64::from(a + b));
    assert_eq!(m.frame_count(), 2);
    assert_eq!(b, m.timing().cycles_per_frame);
}
#[test]
fn palette_write_reaches_the_completed_picture() {
    let mut m = machine();
    m.cpu_write(0xd020, 6);
    m.run_frame();
    assert!(m.framebuffer().iter().any(|p| *p & 0xffffff != 0));
    assert_eq!(m.vic_register(0x20) & 15, 6);
}
#[test]
fn sid_voice_reaches_the_audio_mix() {
    let mut m = machine();
    for (a, v) in [
        (0xd400, 0x80),
        (0xd401, 0x20),
        (0xd405, 0),
        (0xd406, 0xf0),
        (0xd404, 0x21),
        (0xd418, 15),
    ] {
        m.cpu_write(a, v);
    }
    m.run_frame();
    assert!(m.take_audio_buffer().iter().any(|x| x.abs() > 0.0001));
}
#[test]
fn held_contacts_and_peripherals_survive_restore_while_foreign_boards_are_refused() {
    let mut m = machine();
    m.keyboard_mut().set_key(1, 2, true);
    m.cpu_write(0xdc00, 0xfd);
    m.cpu_write(0xdc02, 0xff);
    m.cpu_write(0xdc03, 0);
    let held = m.cpu_read(0xdc01);
    assert_eq!(held & 4, 0);
    m.attach_reu(128);
    let state = m.snapshot_state();
    let mut restored = machine();
    restored
        .restore_snapshot_state(state)
        .expect("matching board");
    assert_eq!(restored.cpu_read(0xdc01), held);
    assert!(restored.has_reu());
    restored.keyboard_mut().set_key(1, 2, false);
    assert_ne!(restored.cpu_read(0xdc01) & 4, 0);
    for model in [
        C64Model::PalBreadbin,
        C64Model::NtscBreadbin,
        C64Model::PalC64c,
        C64Model::NtscC64c,
    ] {
        if model == Configuration::MODEL {
            continue;
        }
        let (k, b, c) = roms();
        let foreign = Raw::new(C64Config {
            model,
            kernal_rom: &k,
            basic_rom: &b,
            character_rom: &c,
        })
        .expect("foreign");
        let before = (
            restored.model(),
            restored.phi2_cycles(),
            restored.cpu().regs.pc,
        );
        assert!(
            restored
                .restore_snapshot_state(foreign.snapshot_state())
                .is_err()
        );
        assert_eq!(
            (
                restored.model(),
                restored.phi2_cycles(),
                restored.cpu().regs.pc
            ),
            before
        );
    }
}
