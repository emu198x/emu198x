//! Verify catalogue switching installs hardware, not just a new label.
use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore};
use runtime_memotech_mtx::{Model, MtxRuntime, ROM_FIRMWARE_ID};

#[test]
fn every_preset_switches_with_its_real_memory_map_and_frame_budget() {
    let rom = vec![0; 24576];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(ROM_FIRMWARE_ID, &rom));
    let mut session = HeadlessSession::new(MtxRuntime::blank(Model::Mtx500), 1);
    for model in Model::ALL {
        assert_eq!(MtxRuntime::model_from_id(model.variant_id()), Some(model));
        session.swap_machine(model, &firmware).expect("switch");
        assert_eq!(
            session.machine().profile().profile_id.as_str(),
            model.profile_id()
        );
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        let machine = session.machine_mut().machine_mut().expect("loaded");
        machine.poke(0x4000, 0x5a);
        assert_eq!(
            machine.peek(0x4000),
            if model == Model::Mtx512 { 0x5a } else { 0xff }
        );
        assert_eq!(machine.peek(0x2000), 0, "paged BASIC ROM remains installed");
        let before = session.machine().model();
        assert!(
            session
                .swap_machine(Model::Mtx500, &FirmwareSet::new())
                .is_err()
        );
        assert_eq!(session.machine().model(), before);
    }
}
