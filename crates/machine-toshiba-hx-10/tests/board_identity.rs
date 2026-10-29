//! Fixed board construction, topology rejection and native-boundary invariants.
use common_msx::{MsxBoard, MsxState};
use emu198x_zilog_z80::Z80Stepper;
use machine_toshiba_hx_10::{Board, Msx, MsxHardware, MsxRegion};
fn bios() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/synthetic-firmware/msx.rom"),
    )
    .expect("own BIOS")
}
fn state(region: MsxRegion, layout: common_msx::MsxLayout, msx2: bool) -> MsxState {
    if msx2 {
        MsxState::msx2(bios(), vec![0xC9; 16384], region, layout)
    } else {
        MsxState::with_layout(bios(), region, layout)
    }
}
#[test]
fn fixed_constructor_and_legacy_binding_reject_foreign_hardware() {
    let m = Msx::try_new(bios(), None).expect("named board");
    assert_eq!(m.hardware(), MsxHardware::ToshibaHx10);
    assert_eq!(m.region(), MsxRegion::Pal);
    assert!(!m.is_msx2());
    assert!(Msx::try_new(bios(), Some(vec![0xC9; 16384])).is_err());
    assert!(Msx::from_state(state(MsxRegion::Pal, Board::layout(), false)).is_ok());
    assert!(Msx::from_state(state(MsxRegion::Ntsc, Board::layout(), false)).is_err());
    assert!(Msx::from_state(state(MsxRegion::Pal, Board::layout(), true)).is_err());
    let mut foreign = Board::layout();
    foreign.cartridge_ports.swap(0, 1);
    assert!(Msx::from_state(state(MsxRegion::Pal, foreign, false)).is_err());
}
#[test]
fn partial_cpu_stepping_stops_at_the_next_real_video_boundary() {
    let mut m = Msx::try_new(bios(), None).expect("board");
    assert!(m.run_native_frame().1);
    let before = m.cpu_tstates();
    let frames = m.frame_count();
    for _ in 0..37 {
        m.step_tick();
    }
    assert!(m.run_native_frame().1);
    assert_eq!(m.frame_count(), frames + 1);
    assert_eq!(m.cpu_tstates() - before, 228 * 313);
}
