//! The B+ runtime: its firmware, its disc slots, and a disc's life in them.
//!
//! Blank firmware is enough here; the real OS and DFS reading a disc is
//! `machine-acorn-bbc-micro-b-plus`'s `bplus_disc` test.

use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, HostIo, MachineCore, MachineError, MachineTime,
    MediaImage, MediaKind, MediaSet, NullAudioSink, NullFrameSink, NullTraceSink, ResetKind,
};
use runtime_acorn_bbc_micro::{
    BPLUS_MOS_FIRMWARE_ID, BbcMicroRuntime, DFS_FIRMWARE_ID, MOS_FIRMWARE_ID, Model,
};

fn b_plus() -> BbcMicroRuntime {
    let mos = vec![0; 16384];
    let dfs = vec![0x5A; 16384];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(BPLUS_MOS_FIRMWARE_ID, &mos));
    firmware.push(FirmwareImage::new(DFS_FIRMWARE_ID, &dfs));
    <BbcMicroRuntime as FamilyRuntime>::from_firmware(Model::BbcBPlus, &firmware)
        .expect("a B+ runtime")
}

/// A 40-track single-sided image, trimmed after three tracks as tools do.
fn short_ssd() -> Vec<u8> {
    let mut image = vec![0u8; 3 * 2560];
    image[..8].copy_from_slice(b"EMU198X ");
    image
}

fn load(
    runtime: &mut BbcMicroRuntime,
    slot: &'static str,
    bytes: &[u8],
) -> Result<(), MachineError> {
    let mut image = MediaImage::new(slot, MediaKind::Disk, bytes);
    image.writable = true;
    runtime.load_media(&MediaSet {
        images: vec![image],
    })
}

#[test]
fn the_catalogue_names_the_b_plus_and_its_mos() {
    assert!(BbcMicroRuntime::variant_ids().contains(&"acorn-bbc-micro-b-plus"));
    assert_eq!(Model::BbcModelB.mos_firmware_id(), MOS_FIRMWARE_ID);
    assert_eq!(Model::BbcBPlus.mos_firmware_id(), BPLUS_MOS_FIRMWARE_ID);
}

#[test]
fn the_b_plus_fits_its_dfs_in_bank_14() {
    let mut runtime = b_plus();
    assert_eq!(
        runtime.profile().profile_id.as_str(),
        "acorn-bbc-micro-b-plus"
    );
    let machine = runtime.machine_mut().expect("machine");
    machine.poke(0xFE30, 14);
    assert_eq!(machine.peek(0x8000), 0x5A);
}

#[test]
fn a_disc_loads_survives_a_reset_exports_and_ejects() {
    let mut runtime = b_plus();
    load(&mut runtime, "drive-0", &short_ssd()).expect("loads");
    let disk = runtime
        .machine()
        .and_then(|m| m.disk(0))
        .expect("in drive 0");
    assert_eq!(disk.data().len(), 40 * 2560, "padded to 40 tracks");

    runtime.reset(ResetKind::Hard);
    assert!(
        runtime.machine().and_then(|m| m.disk(0)).is_some(),
        "a reset keeps the disc"
    );

    let exported = runtime.export_media("drive-0", "ssd").expect("exports");
    assert_eq!(&exported[..8], b"EMU198X ");
    assert!(
        runtime.export_media("drive-0", "dsd").is_err(),
        "one side is an ssd"
    );
    assert!(
        runtime.export_media("drive-1", "ssd").is_err(),
        "drive 1 is empty"
    );

    runtime.eject_media("drive-0").expect("ejects");
    assert!(runtime.machine().and_then(|m| m.disk(0)).is_none());
}

#[test]
fn a_disc_snapshot_restores() {
    let mut runtime = b_plus();
    load(&mut runtime, "drive-1", &short_ssd()).expect("loads");
    runtime
        .run_until(
            MachineTime::new(100_000),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("runs");
    let snapshot = runtime.snapshot().expect("snapshot");
    let mut restored = BbcMicroRuntime::blank(Model::BbcBPlus);
    restored.restore(&snapshot).expect("restores");
    assert_eq!(restored.snapshot().expect("re-snapshot"), snapshot);
    assert!(restored.machine().and_then(|m| m.disk(1)).is_some());
    assert!(
        BbcMicroRuntime::blank(Model::BbcModelB)
            .restore(&snapshot)
            .is_err(),
        "a B+ snapshot is not a Model B's"
    );
}

#[test]
fn a_model_b_has_no_disc_slots() {
    let mut runtime = BbcMicroRuntime::blank(Model::BbcModelB);
    assert!(matches!(
        load(&mut runtime, "drive-0", &short_ssd()),
        Err(MachineError::UnknownMediaSlot { .. })
    ));
    assert!(runtime.eject_media("drive-0").is_err());
}
