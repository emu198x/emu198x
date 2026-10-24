//! The Master 128 runtime: its ROM set, its drives and its CPU across a
//! snapshot.
//!
//! Blank firmware is enough here; the real MOS 3.20 booting and reading a
//! disc is `machine-acorn-bbc-micro`'s `master_disc` test.

use emu198x_mos_6502::Variant;
use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, HostIo, MachineCore, MachineTime, MediaImage,
    MediaKind, MediaSet, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_acorn_bbc_micro::{BbcMicroRuntime, MASTER_MOS_FIRMWARE_ID, MASTER_ROMS, Model};

fn master() -> BbcMicroRuntime {
    let mos = vec![0; 16384];
    let roms: Vec<Vec<u8>> = MASTER_ROMS
        .iter()
        .map(|rom| vec![rom.bank as u8; 16384])
        .collect();
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(MASTER_MOS_FIRMWARE_ID, &mos));
    for (rom, bytes) in MASTER_ROMS.iter().zip(&roms) {
        firmware.push(FirmwareImage::new(rom.id, bytes));
    }
    BbcMicroRuntime::from_firmware_with_basic(Model::Master128, &firmware)
        .expect("a Master runtime")
}

#[test]
fn the_catalogue_offers_the_master() {
    assert_eq!(
        BbcMicroRuntime::variant_ids(),
        &[
            "acorn-bbc-micro-b",
            "acorn-bbc-micro-b-plus",
            "acorn-bbc-master-128"
        ]
    );
}

#[test]
fn the_rom_set_fills_banks_9_to_15_and_4_to_7_are_ram() {
    let mut runtime = master();
    assert_eq!(
        runtime.profile().profile_id.as_str(),
        "acorn-bbc-master-128"
    );
    let machine = runtime.machine_mut().expect("machine");
    for rom in &MASTER_ROMS {
        machine.poke(0xFE30, rom.bank as u8);
        assert_eq!(machine.peek(0x8000), rom.bank as u8, "bank {}", rom.bank);
    }
    // BASIC 4 stays in bank 12; bank 15 is the Terminal, not a second BASIC.
    machine.poke(0xFE30, 15);
    assert_eq!(machine.peek(0x8000), 15);
    machine.poke(0xFE30, 5);
    machine.poke(0x8000, 0x5A);
    assert_eq!(machine.peek(0x8000), 0x5A, "bank 5 is sideways RAM");
    assert_eq!(machine.cpu().variant(), Variant::Cmos);
}

#[test]
fn a_disc_loads_and_a_snapshot_keeps_the_65c12() {
    let mut runtime = master();
    let image = vec![0u8; 3 * 2560];
    runtime
        .load_media(&MediaSet {
            images: vec![MediaImage::new("drive-0", MediaKind::Disk, &image)],
        })
        .expect("loads");
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
    let mut restored = BbcMicroRuntime::blank(Model::Master128);
    restored.restore(&snapshot).expect("restores");
    assert_eq!(restored.snapshot().expect("re-snapshot"), snapshot);
    let machine = restored.machine().expect("machine");
    assert_eq!(
        machine.cpu().variant(),
        Variant::Cmos,
        "set again on restore"
    );
    assert!(machine.disk(0).is_some());
}
