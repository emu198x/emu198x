//! Catalogue cold boots retain physical media, including snapshot-restored mappers.
use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, MediaImage, MediaKind, MediaSet,
    ResetKind,
};
use machine_atari_800xl::CartridgeKind;
use runtime_atari_800xl::{Atari800xlRuntime, Model, OS_FIRMWARE_ID};

fn cart() -> Vec<u8> {
    let mut bytes = vec![0; 16];
    bytes[..4].copy_from_slice(b"CART");
    bytes[4..8].copy_from_slice(&15_u32.to_be_bytes()); // OSS one-chip mapper, 16K payload.
    for bank in 0..4 {
        bytes.extend(vec![0x40 + bank; 4096]);
    }
    bytes
}

fn runtime() -> Atari800xlRuntime {
    Atari800xlRuntime::new(Model::A800xlNtsc, None, None, Some(cart()), false).expect("cart")
}

fn mount_disk(runtime: &mut Atari800xlRuntime) {
    let mut bytes = vec![0; 16 + 128];
    bytes[..6].copy_from_slice(&[0x96, 0x02, 8, 0, 128, 0]);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-1", MediaKind::Disk, &bytes));
    runtime.load_media(&media).expect("disk");
    let bus = runtime.machine_mut().expect("machine").sio_mut();
    let mut disk = bus.eject_disk(1).expect("disk");
    disk.write_sector(1, &[0x5a; 128]).expect("modified sector");
    bus.insert_disk(1, disk);
}

fn assert_disk(runtime: &Atari800xlRuntime) {
    assert_eq!(
        runtime
            .machine()
            .expect("machine")
            .sio()
            .drive(1)
            .expect("drive")
            .disk()
            .expect("disk")
            .sector(1),
        Some([0x5a; 128].as_slice())
    );
}

#[test]
fn replacement_preserves_cartridge_type_basic_policy_and_modified_disk_but_clears_ram_and_xex() {
    let mut runtime = runtime();
    mount_disk(&mut runtime);
    runtime.machine_mut().expect("machine").poke(0x4000, 0x77);
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "program-1",
        MediaKind::Program,
        &[0xff, 0xff, 0x00, 0x40, 0x00, 0x40, 0x60],
    ));
    runtime.load_media(&media).expect("XEX");
    for model in Model::ALL {
        let mut replacement = runtime
            .replacement(model, &FirmwareSet::new())
            .expect("replacement");
        assert_eq!(replacement.model(), model);
        assert_eq!(replacement.native_frame_ticks(), model.frame_ticks());
        assert!(!replacement.basic_enabled());
        assert_eq!(replacement.machine().expect("machine").peek(0x4000), 0);
        assert_eq!(
            replacement
                .machine()
                .expect("machine")
                .cartridge()
                .expect("cart")
                .kind(),
            CartridgeKind::OssOneChip
        );
        assert_disk(&replacement);
        replacement.reset(ResetKind::Hard);
        assert_disk(&replacement);
        use emu198x_shell::SessionQueryProvider;
        let query = runtime_atari_800xl::Atari800xlSessionQueryProvider;
        assert_eq!(
            query
                .query(&replacement, "program.loaded")
                .expect("query")
                .expect("known")
                .value,
            serde_json::json!(false)
        );
    }
    assert_eq!(runtime.machine().expect("machine").peek(0x4000), 0x77);
    assert_disk(&runtime);
}

#[test]
fn invalid_cartridge_does_not_poison_reset_or_eject_disk() {
    let mut runtime = runtime();
    mount_disk(&mut runtime);
    runtime.machine_mut().expect("machine").poke(0x4000, 0x77);
    let before = runtime.snapshot().expect("snapshot");
    assert!(runtime.insert_cartridge(Some(vec![])).is_err());
    assert_eq!(runtime.snapshot().expect("snapshot"), before);
    runtime.reset(ResetKind::Hard);
    assert_disk(&runtime);
    assert_eq!(
        runtime
            .machine()
            .expect("machine")
            .cartridge()
            .expect("cart")
            .kind(),
        CartridgeKind::OssOneChip
    );
}

#[test]
fn snapshot_restored_firmware_and_mapper_survive_reset_and_region_change() {
    let mut original = runtime();
    original.set_os(Some(vec![0x5c; 16384])).expect("OS");
    mount_disk(&mut original);
    let snapshot = original.snapshot().expect("snapshot");
    let mut restored = Atari800xlRuntime::blank(Model::A800xlNtsc);
    restored.restore(&snapshot).expect("restore");
    restored.reset(ResetKind::Hard);
    assert_eq!(restored.machine().expect("machine").peek(0xc000), 0x5c);
    assert_disk(&restored);
    let os = vec![0x6d; 16384];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(OS_FIRMWARE_ID, &os));
    let replacement = restored
        .replacement(Model::A800xlPal, &firmware)
        .expect("switch");
    assert_eq!(replacement.machine().expect("machine").peek(0xc000), 0x6d);
    assert_eq!(
        replacement
            .machine()
            .expect("machine")
            .cartridge()
            .expect("cart")
            .kind(),
        CartridgeKind::OssOneChip
    );
    assert_disk(&replacement);
}
