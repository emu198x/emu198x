use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, MediaImage, MediaKind, MediaSet,
    ResetKind,
};
use runtime_commodore_vic_20::{
    BASIC_FIRMWARE_ID, CHAR_FIRMWARE_ID, KERNAL_FIRMWARE_ID, Model, Vic20RamExpansion, Vic20Runtime,
};

#[test]
fn restored_roms_ram_and_cartridge_survive_reset_and_regional_replacement() {
    let images = [
        (KERNAL_FIRMWARE_ID, vec![0x4c; 8192]),
        (BASIC_FIRMWARE_ID, vec![0x42; 8192]),
        (CHAR_FIRMWARE_ID, vec![0x3c; 4096]),
    ];
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &images {
        firmware.push(FirmwareImage::new(*id, bytes));
    }
    let cart = vec![0x5a; 8192];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &cart));
    for model in Model::ALL {
        let mut source = Vic20Runtime::from_firmware(model, &firmware).expect("runtime");
        source.set_ram_expansion(Vic20RamExpansion::EXP_16K);
        source.load_media(&media).expect("cartridge");
        source.machine_mut().expect("machine").poke(0x4000, 0x77);
        let snap = source.snapshot().expect("snapshot");
        let mut restored = Vic20Runtime::blank(model);
        restored.restore(&snap).expect("restore");
        assert_eq!(restored.snapshot().expect("snapshot"), snap);
        assert_eq!(restored.ram_expansion(), Vic20RamExpansion::EXP_16K);
        restored.reset(ResetKind::Hard);
        assert_eq!(restored.machine().expect("machine").peek(0xc000), 0x42);
        assert_eq!(restored.machine().expect("machine").peek(0xa000), 0x5a);
        assert_eq!(restored.machine().expect("machine").peek(0x4000), 0);
        restored.attach_esp_at_tcp_bridge(115, 64);
        for target in Model::ALL {
            let mut replacement = restored.replacement(target, &firmware).expect("switch");
            assert_eq!(replacement.ram_expansion(), Vic20RamExpansion::EXP_16K);
            assert!(replacement.esp_at_tcp_bridge().is_none());
            assert_eq!(replacement.machine().expect("machine").peek(0xa000), 0x5a);
            replacement
                .machine_mut()
                .expect("machine")
                .poke(0x4000, 0x66);
            assert_eq!(replacement.machine().expect("machine").peek(0x4000), 0x66);
            assert_eq!(replacement.model(), target);
            assert_eq!(replacement.native_frame_ticks(), target.frame_ticks());
        }
    }
}
