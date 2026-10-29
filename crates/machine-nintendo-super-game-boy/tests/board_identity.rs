//! Fixed physical construction and native Game Boy field boundaries.
use machine_nintendo_super_game_boy::{BootProfile, GameBoy};
fn rom() -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/nintendo/synthetic-cart/game-boy.gb"),
    )
    .expect("project cartridge")
}
#[test]
fn constructor_cannot_select_another_physical_board() {
    let (_, m) = GameBoy::from_rom(rom()).expect("fixed board");
    assert_eq!(format!("{:?}", m.hardware()), "Sgb");
    assert!(!m.is_cgb());
    for profile in [
        BootProfile::Dmg0,
        BootProfile::DmgAbc,
        BootProfile::Mgb,
        BootProfile::Sgb,
        BootProfile::Sgb2,
        BootProfile::Cgb,
        BootProfile::Agb,
    ] {
        assert_eq!(
            GameBoy::try_from_rom_with_boot_rom(rom(), profile, None).is_ok(),
            matches!(profile, BootProfile::Sgb),
            "{profile:?}"
        );
    }
}
#[test]
fn native_game_boy_fields_have_the_ppus_complete_dot_count() {
    let (_, mut m) = GameBoy::from_rom(rom()).expect("fixed board");
    assert!(m.run_frame_dots_bounded().1);
    let (dots, complete) = m.run_frame_dots_bounded();
    assert!(complete);
    assert_eq!(dots, 456 * 154);
}
