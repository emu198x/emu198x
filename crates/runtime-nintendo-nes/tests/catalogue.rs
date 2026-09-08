//! Regional construction, save RAM retention and snapshot compatibility.
mod common;
use emu198x_shell::{
    FamilyRuntime, FirmwareSet, MachineCore, MediaImage, MediaKind, MediaSet, ResetKind,
};
use runtime_nintendo_nes::{ApuChannel, Model, NesRuntime};

fn runtime(model: Model) -> NesRuntime {
    let mut runtime =
        NesRuntime::from_firmware(model, &FirmwareSet::new()).expect("firmwareless runtime");
    let mut rom = common::minimal_ines();
    rom[6] |= 2;
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("cartridge");
    runtime.restore_cartridge_ram(&[0x5a; 8192]).expect("RAM");
    runtime.set_audio_channel_enabled(ApuChannel::Pulse1, false);
    runtime
}

#[test]
fn region_switch_and_reset_keep_cartridge_ram_and_mixer_but_restart_cpu_and_ram() {
    let mut original = runtime(Model::NesNtsc);
    original.machine_mut().expect("machine").poke(0x100, 77);
    for model in Model::ALL {
        let mut next = original
            .replacement(model, &FirmwareSet::new())
            .expect("switch");
        assert_eq!(next.model(), model);
        assert_eq!(
            next.machine().expect("machine").region(),
            model.machine_region()
        );
        assert_eq!(next.machine().expect("machine").peek(0x100), 0);
        assert_eq!(next.cartridge_ram(), Some([0x5a; 8192].as_slice()));
        assert_eq!(next.audio_controls(), original.audio_controls());
        next.reset(ResetKind::Hard);
        assert_eq!(
            next.machine().expect("machine").region(),
            model.machine_region()
        );
        assert_eq!(next.cartridge_ram(), Some([0x5a; 8192].as_slice()));
    }
    assert_eq!(original.machine().expect("machine").peek(0x100), 77);
}

#[test]
fn snapshots_restore_battery_metadata_and_reject_a_different_region_atomically() {
    for model in Model::ALL {
        let original = runtime(model);
        let bytes = original.snapshot().expect("snapshot");
        let mut restored = NesRuntime::blank(model);
        restored.restore(&bytes).expect("same region");
        assert!(restored.has_battery_backed_ram());
        assert_eq!(restored.cartridge_ram(), original.cartridge_ram());
        restored.reset(ResetKind::Hard);
        assert_eq!(restored.cartridge_ram(), original.cartridge_ram());
        let other = if model == Model::NesNtsc {
            Model::NesPal
        } else {
            Model::NesNtsc
        };
        let mut wrong_region = runtime(other);
        let before = wrong_region.snapshot().expect("before");
        assert!(wrong_region.restore(&bytes).is_err());
        assert_eq!(wrong_region.snapshot().expect("after"), before);
        let replacement = restored
            .replacement(other, &FirmwareSet::new())
            .expect("switch restored cart");
        assert_eq!(replacement.cartridge_ram(), original.cartridge_ram());
    }
}

#[test]
fn catalogue_regions_drive_the_existing_machine_clock_dividers() {
    for (model, dots, cpu_cycles) in [
        (Model::NesNtsc, 89342, 29780..=29781),
        (Model::NesPal, 106392, 33247..=33248),
    ] {
        let mut runtime = runtime(model);
        assert_eq!(runtime.native_frame_ticks(), dots);
        let machine = runtime.machine_mut().expect("machine");
        machine.run_frame();
        let before = machine.cpu_cycle_count();
        assert_eq!(machine.run_frame(), dots);
        assert!(cpu_cycles.contains(&(machine.cpu_cycle_count() - before)));
    }
}
