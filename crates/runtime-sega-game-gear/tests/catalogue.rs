use emu198x_shell::{FamilyRuntime, FirmwareOverrides, MachineCore, build_variant};
use runtime_sega_game_gear::{Model, SmsRuntime};
#[test]
fn catalogue_is_game_gear_only_and_needs_no_firmware() {
    assert_eq!(SmsRuntime::variant_ids(), &["game-gear"]);
    assert_eq!(Model::from_variant_id("gg"), Some(Model::GameGear));
    assert_eq!(
        Model::from_variant_id("sega-game-gear"),
        Some(Model::GameGear)
    );
    assert_eq!(Model::from_variant_id("sms-ntsc"), None);
    let runtime = build_variant::<SmsRuntime>(Model::GameGear, &FirmwareOverrides::none())
        .expect("BIOS-less");
    assert_eq!(runtime.profile().machine_id.as_str(), "sega-game-gear");
    assert!(
        !runtime
            .capabilities()
            .contains(&emu198x_shell::known_capability("variant-switch"))
    );
    assert_eq!(runtime.native_frame_ticks(), 228 * 262);
}
