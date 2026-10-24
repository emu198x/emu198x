//! The MSX2 models through the runtime surface, with synthetic firmware: a
//! BIOS that halts and a blank SUB-ROM, so these run on every push.

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HostIo, MachineCore, MachineError, MachineTime, NullAudioSink,
    NullFrameSink, NullTraceSink, SessionQueryProvider,
};
use runtime_msx::{
    BIOS_FIRMWARE_ID, Model, MsxRuntime, MsxSessionQueryProvider, SUB_ROM_FIRMWARE_ID,
};

/// `DI; HALT` — the machine runs but does nothing.
fn bios() -> Vec<u8> {
    let mut rom = vec![0; 32 * 1024];
    rom[..2].copy_from_slice(&[0xF3, 0x76]);
    rom
}

fn run(runtime: &mut MsxRuntime, frames: u64) {
    runtime
        .run_until(
            MachineTime::new(runtime.model().frame_ticks() * frames),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("the runtime should run");
}

fn msx2(model: Model) -> MsxRuntime {
    let mut runtime = MsxRuntime::new(model, bios()).expect("a 32 KB BIOS");
    runtime
        .set_sub_rom(vec![0; 16 * 1024])
        .expect("a 16 KB SUB-ROM");
    runtime
}

#[test]
fn an_msx2_needs_both_roms() {
    let mut runtime = MsxRuntime::new(Model::PhilipsVg8235, bios()).expect("a 32 KB BIOS");
    assert!(runtime.machine().is_none());
    let err = runtime
        .set_sub_rom(vec![0; 8 * 1024])
        .expect_err("8 KB is too short");
    assert!(
        matches!(err, MachineError::InvalidFirmware { ref id, .. } if id == SUB_ROM_FIRMWARE_ID)
    );
    runtime.set_sub_rom(vec![0; 16 * 1024]).expect("16 KB");
    let machine = runtime.machine().expect("both ROMs make a machine");
    assert!(machine.is_msx2());
    assert_eq!(machine.sub_rom().map(<[u8]>::len), Some(16 * 1024));
}

#[test]
fn from_firmware_reads_the_sub_rom() {
    let bios = bios();
    let sub_rom = vec![0x11; 16 * 1024];
    let mut firmware = FirmwareSet::default();
    firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &bios));
    let missing = MsxRuntime::from_firmware(Model::SonyHbF1, &firmware);
    assert!(missing.is_err(), "the SUB-ROM is required");
    firmware.push(FirmwareImage::new(SUB_ROM_FIRMWARE_ID, &sub_rom));
    let runtime = MsxRuntime::from_firmware(Model::SonyHbF1, &firmware).expect("both ROMs");
    assert_eq!(
        runtime
            .machine()
            .and_then(|m| m.sub_rom())
            .map(|rom| rom[0]),
        Some(0x11)
    );
}

#[test]
fn an_msx2_snapshot_restores_its_sub_rom() {
    let mut runtime = msx2(Model::PanasonicFsA1);
    run(&mut runtime, 3);
    let snapshot = runtime.snapshot().expect("snapshot");

    let mut restored = MsxRuntime::blank(Model::PanasonicFsA1);
    restored.restore(&snapshot).expect("restore");
    assert_eq!(restored.snapshot().expect("re-snapshot"), snapshot);
    // A reset rebuilds from the restored firmware, SUB-ROM included.
    restored.reset(emu198x_shell::ResetKind::Hard);
    assert!(restored.machine().is_some_and(|m| m.is_msx2()));
}

#[test]
fn the_v9938_framebuffer_has_half_width_pixels() {
    let runtime = msx2(Model::Msx2Ntsc);
    let machine = runtime.machine().expect("a machine");
    assert_eq!(machine.framebuffer_width(), 560);
    let msx2_aspect = runtime
        .display()
        .expect("a television")
        .pixel_aspect_ratio(560, 240);
    let msx1 = MsxRuntime::new(Model::Msx1Ntsc, bios()).expect("MSX1");
    let msx1_aspect = msx1
        .display()
        .expect("a television")
        .pixel_aspect_ratio(288, 240);
    assert!(
        (msx1_aspect / msx2_aspect - 2.0).abs() < 0.01,
        "{msx1_aspect} against {msx2_aspect}"
    );
}

#[test]
fn queries_name_the_chip_and_the_msx2_parts() {
    let mut runtime = msx2(Model::Msx2Pal);
    run(&mut runtime, 1);
    let provider = MsxSessionQueryProvider;
    let query = |path: &str| {
        provider
            .query(&runtime, path)
            .expect("answers")
            .expect("known path")
            .value
    };
    assert_eq!(query("vdp")["chip"], "V9938");
    assert_eq!(query("vdp.screen_mode"), "G1");
    assert_eq!(query("mapper.registers"), serde_json::json!([0, 0, 0, 0]));
    assert_eq!(query("rtc"), "1980-01-01 00:00:00");
    assert_eq!(query("vdp.framebuffer_width"), 556);

    let msx1 = MsxRuntime::new(Model::Msx1Pal, bios()).expect("MSX1");
    let value = provider
        .query(&msx1, "vdp")
        .expect("answers")
        .expect("known")
        .value;
    assert_eq!(value["chip"], "TMS9918");
    let mode = provider
        .query(&msx1, "vdp.screen_mode")
        .expect("answers")
        .expect("known")
        .value;
    assert!(mode.is_null(), "an MSX1 has no V9938 mode");
}
