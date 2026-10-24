//! The Zapper and Four Score selection through the runtime: it applies to
//! the loaded machine, survives resets and cartridge changes, and rides in
//! snapshots.

use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet, ResetKind};
use runtime_nintendo_nes::{InputDevices, Model, NesRuntime};

/// 16 KiB of NOPs with the reset vector at `$8000`, and CHR RAM.
fn minimal_ines() -> Vec<u8> {
    let mut rom = vec![0u8; 16 + 16 * 1024];
    rom[..4].copy_from_slice(b"NES\x1A");
    rom[4] = 1;
    rom[16..].fill(0xEA);
    rom[16 + 0x3FFC] = 0x00;
    rom[16 + 0x3FFD] = 0x80;
    rom
}

fn load(runtime: &mut NesRuntime) {
    let rom = minimal_ines();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("cartridge loads");
}

fn machine_devices(runtime: &NesRuntime) -> InputDevices {
    runtime.machine().expect("machine").input_devices()
}

#[test]
fn the_choice_applies_before_and_after_loading_and_survives_a_reset() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    runtime.set_input_devices(InputDevices::FourScore);
    load(&mut runtime);
    assert_eq!(machine_devices(&runtime), InputDevices::FourScore);
    runtime.set_input_devices(InputDevices::Zapper);
    assert_eq!(machine_devices(&runtime), InputDevices::Zapper);
    runtime.reset(ResetKind::Hard);
    assert_eq!(machine_devices(&runtime), InputDevices::Zapper);
}

#[test]
fn a_snapshot_carries_the_devices() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    load(&mut runtime);
    runtime.set_input_devices(InputDevices::FourScore);
    let snapshot = runtime.snapshot().expect("snapshot");
    let mut restored = NesRuntime::blank(Model::NesNtsc);
    restored.restore(&snapshot).expect("restore");
    assert_eq!(restored.input_devices(), InputDevices::FourScore);
    assert_eq!(machine_devices(&restored), InputDevices::FourScore);
}
