//! Verify catalogue switching installs hardware, not just a new label.
use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore};
use runtime_jupiter_ace::{BIOS_FIRMWARE_ID, JupiterAceRuntime, Model};

#[test]
fn every_preset_switches_with_its_real_memory_map_and_frame_budget() {
    let rom = vec![0; 8192];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &rom));
    let mut session = HeadlessSession::new(JupiterAceRuntime::blank(Model::Ace3k), 1);
    for model in Model::ALL {
        assert_eq!(
            JupiterAceRuntime::model_from_id(model.variant_id()),
            Some(model)
        );
        session.swap_machine(model, &firmware).expect("switch");
        assert_eq!(
            session.machine().profile().profile_id.as_str(),
            model.profile_id()
        );
        assert_eq!(session.native_frame_ticks(), model.frame_ticks());
        let machine = session.machine_mut().machine_mut().expect("loaded");
        for (addr, mapped) in [
            (0x4000, model != Model::Ace3k),
            (0x8000, model == Model::Ace48k),
        ] {
            machine.poke(addr, 0x5a);
            assert_eq!(machine.peek(addr), if mapped { 0x5a } else { 0xff });
        }
        let before = session.machine().model();
        assert!(
            session
                .swap_machine(Model::Ace3k, &FirmwareSet::new())
                .is_err()
        );
        assert_eq!(session.machine().model(), before);
    }
}
