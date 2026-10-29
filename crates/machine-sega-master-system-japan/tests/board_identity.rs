//! Fixed ownership and native boundaries agree with the unchanged chip cadence.
use common_sega_master_system::{SmsHardware, SmsState, SmsVariant};
use emu198x_zilog_z80::Z80Stepper;
use machine_sega_master_system_japan::Sms;
#[test]
fn construction_and_legacy_binding_keep_this_physical_board() {
    let m = Sms::new(vec![0; 32768]);
    assert_eq!(m.hardware(), SmsHardware::MasterSystemJapan);
    assert_eq!(m.variant(), SmsVariant::SmsJapanNtsc);
    for variant in [
        SmsVariant::SmsNtsc,
        SmsVariant::SmsPal,
        SmsVariant::SmsJapanNtsc,
        SmsVariant::Sms1Ntsc,
        SmsVariant::Sms1Pal,
        SmsVariant::MarkIII,
        SmsVariant::GameGear,
    ] {
        assert_eq!(
            Sms::from_state(SmsState::new(vec![0; 32768], variant)).is_ok(),
            variant == SmsVariant::SmsJapanNtsc
        );
    }
}
#[test]
fn partial_stepping_stops_at_the_next_real_chip_boundary() {
    let mut m = Sms::new(vec![0; 32768]);
    assert!(m.run_native_frame().1);
    for _ in 0..37 {
        m.step_tick();
    }
    let frames = m.frame_count();
    let before = m.cpu_tstates();
    let (_, complete) = m.run_native_frame();
    assert!(complete);
    assert_eq!(m.frame_count(), frames + 1);
    assert_eq!(m.cpu_tstates() - before + 37, 228 * 262);
}
