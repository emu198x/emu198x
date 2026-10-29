//! Board identity, exact native field cadence and cycle-level external lines.
use machine_commodore_vic_20_pal::{Vic20, Vic20RamExpansion};
fn machine() -> Vic20 {
    let mut kernal = vec![0xEA; 8192];
    kernal[..3].copy_from_slice(&[0x4C, 0, 0xE0]);
    kernal[0x1FFC..0x1FFE].copy_from_slice(&[0, 0xE0]);
    Vic20::new(
        kernal,
        vec![0; 8192],
        vec![0; 4096],
        Vic20RamExpansion::NONE,
    )
}
#[test]
fn concrete_board_has_the_right_raster_and_field_cadence() {
    let mut m = machine();
    assert_eq!((m.framebuffer_width(), m.framebuffer_height()), (230, 288));
    assert_eq!(format!("{:?}", m.model()), "Pal");
    assert!(m.run_native_frame().1);
    let (elapsed, complete) = m.run_native_frame();
    assert!(complete);
    assert_eq!(elapsed, 22152);
}
#[test]
fn user_port_exchange_runs_once_per_cpu_cycle_and_reset_keeps_board() {
    let mut m = machine();
    let mut calls = 0_u64;
    let (elapsed, complete) = m.run_native_frame_with_user_port(&mut |tx| {
        assert!(tx);
        calls += 1;
        calls.is_multiple_of(2)
    });
    assert!(complete);
    assert_eq!(calls, elapsed);
    assert_eq!(m.user_port_pb0(), calls.is_multiple_of(2));
    let reset = m.cold_boot(Vic20RamExpansion::EXP_3K);
    assert_eq!(
        (reset.framebuffer_width(), reset.framebuffer_height()),
        (230, 288)
    );
    assert_eq!(reset.master_clock(), 0);
    assert!(reset.expansion().exp_3k);
}
