//! The Game Boy Color models: colour frames, real-time pacing at
//! double speed, the optional boot ROM, and snapshots.

#![allow(clippy::unwrap_used)]

mod common;

use emu198x_shell::{
    FirmwareImage, FirmwareSet, FramePacket, FrameSink, HostIo, MachineCore, MachineError,
    MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink, NullTraceSink, PixelFormat,
};
use runtime_nintendo_game_boy::{CGB_BOOT_ROM_ID, GameBoyRuntime, Model};

/// `LD A,1 / LDH ($4D),A / STOP / JR -2` on a CGB-only cartridge.
fn speed_switch_rom() -> Vec<u8> {
    let mut rom = common::loop_rom();
    rom[0x0100..0x0108].copy_from_slice(&[0x3E, 0x01, 0xE0, 0x4D, 0x10, 0x00, 0x18, 0xFE]);
    rom[0x0143] = 0xC0;
    let mut checksum: u8 = 0;
    for &byte in &rom[0x0134..=0x014C] {
        checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
    }
    rom[0x014D] = checksum;
    rom
}

fn load(runtime: &mut GameBoyRuntime, rom: &[u8]) {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, rom));
    runtime.load_media(&media).unwrap();
}

/// Records the format and size of every frame.
#[derive(Default)]
struct Frames(Vec<(PixelFormat, usize, MachineTime)>);

impl FrameSink for Frames {
    fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
        self.0
            .push((frame.format, frame.pixels.len(), frame.timestamp));
        Ok(())
    }
}

fn run(runtime: &mut GameBoyRuntime, target: u64) -> Frames {
    let mut frames = Frames::default();
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frames,
        audio_sink: &mut NullAudioSink,
        trace_sink: &mut NullTraceSink,
    };
    runtime
        .run_until(MachineTime::new(target), &mut host)
        .unwrap();
    frames
}

#[test]
fn a_cgb_emits_rgba_frames_and_a_dmg_indexed_ones() {
    let mut cgb = GameBoyRuntime::blank(Model::Cgb);
    load(&mut cgb, &common::loop_rom());
    let frames = run(&mut cgb, 1);
    assert_eq!(frames.0[0].0, PixelFormat::Rgba8888);
    assert_eq!(frames.0[0].1, 160 * 144 * 4);

    let mut dmg = GameBoyRuntime::blank(Model::Dmg);
    load(&mut dmg, &common::loop_rom());
    let frames = run(&mut dmg, 1);
    assert_eq!(frames.0[0].0, PixelFormat::Indexed8);
}

#[test]
fn double_speed_keeps_real_time() {
    let mut runtime = GameBoyRuntime::blank(Model::Cgb);
    load(&mut runtime, &speed_switch_rom());
    // Past the speed switch and its HALT (about two frames).
    run(&mut runtime, 4 * 17_556);
    assert!(runtime.machine().unwrap().double_speed());

    let before = runtime.time().get();
    let frames = run(&mut runtime, before + 10 * 17_556);
    let elapsed = frames.0.last().unwrap().2.get() - before;
    assert_eq!(frames.0.len(), 10, "60 frames a second at either speed");
    assert_eq!(elapsed, 10 * 17_556);
}

#[test]
fn the_boot_rom_must_be_2304_bytes() {
    let short = vec![0u8; 256];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(CGB_BOOT_ROM_ID, &short));
    let Err(error) = GameBoyRuntime::from_firmware(Model::Cgb, &firmware) else {
        panic!("a 256-byte CGB boot ROM was accepted");
    };
    assert!(matches!(error, MachineError::InvalidFirmware { .. }));
}

#[test]
fn a_supplied_boot_rom_runs_from_zero() {
    // JR -2 at $0000: the boot ROM never hands over.
    let mut boot_rom = vec![0u8; 0x900];
    boot_rom[0] = 0x18;
    boot_rom[1] = 0xFE;
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(CGB_BOOT_ROM_ID, &boot_rom));
    let mut runtime = GameBoyRuntime::from_firmware(Model::Cgb, &firmware).unwrap();
    load(&mut runtime, &common::loop_rom());
    run(&mut runtime, 17_556);
    assert_eq!(runtime.machine().unwrap().cpu_pc(), 0x0000);
}

#[test]
fn a_cgb_snapshot_round_trips() {
    let mut runtime = GameBoyRuntime::blank(Model::Cgb);
    load(&mut runtime, &speed_switch_rom());
    run(&mut runtime, 4 * 17_556);
    let snapshot = runtime.snapshot().unwrap();

    let mut restored = GameBoyRuntime::blank(Model::Cgb);
    restored.restore(&snapshot).unwrap();
    let machine = restored.machine().unwrap();
    assert!(machine.cgb_mode());
    assert!(machine.double_speed());
    assert_eq!(restored.time(), runtime.time());
}
