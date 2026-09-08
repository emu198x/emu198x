//! Profile changes restart the host machine without losing cartridge state.
mod common;
use emu198x_shell::{
    FamilyRuntime, FirmwareSet, MachineCore, MediaImage, MediaKind, MediaSet, ResetKind,
};
use runtime_nintendo_game_boy::{ApuChannel, GameBoyRuntime, Model};

fn rtc_runtime() -> GameBoyRuntime {
    let mut rom = common::loop_rom();
    rom.resize(0x10000, 0);
    for bank in 1..4 {
        rom[bank * 0x4000..(bank + 1) * 0x4000].fill(bank as u8);
    }
    rom[0x147] = 0x10; // MBC3, RTC, RAM and battery.
    rom[0x148] = 1;
    rom[0x149] = 2;
    rom[0x14d] = rom[0x134..=0x14c]
        .iter()
        .fold(0u8, |sum, b| sum.wrapping_sub(*b).wrapping_sub(1));
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("RTC cartridge");
    runtime.set_audio_channel_enabled(ApuChannel::Pulse1, false);
    let machine = runtime.machine_mut().expect("machine");
    machine.poke(0xc000, 0x77);
    let cart = machine.cartridge_mut();
    cart.ram_mut()[0] = 0x5a;
    cart.write_rom(0, 0x0a);
    cart.write_rom(0x4000, 0x0c);
    cart.write_ram(0xa000, 0x40); // Halt the clock for deterministic assertions.
    cart.write_rom(0x4000, 0x08);
    cart.write_ram(0xa000, 37);
    cart.write_rom(0x6000, 0);
    cart.write_rom(0x6000, 1);
    cart.write_rom(0x2000, 3);
    runtime
}

fn assert_persistent_state(runtime: &mut GameBoyRuntime) {
    let cart = runtime.machine_mut().expect("machine").cartridge_mut();
    assert_eq!(cart.ram()[0], 0x5a);
    assert_eq!(cart.read_rom(0x4000), 1, "mapper starts at bank 1");
    assert_eq!(cart.read_ram(0xa000), 0xff, "RAM starts disabled");
    cart.write_rom(0, 0x0a);
    cart.write_rom(0x4000, 0x08);
    assert_eq!(cart.read_ram(0xa000), 37, "latched RTC retained");
    cart.write_rom(0x6000, 0);
    cart.write_rom(0x6000, 1);
    assert_eq!(cart.read_ram(0xa000), 37, "live RTC retained");
}

#[test]
fn every_profile_keeps_ram_rtc_and_audio_controls_but_restarts_the_machine() {
    let original = rtc_runtime();
    let snapshot = original.snapshot().expect("snapshot");
    let mut restored = GameBoyRuntime::blank(Model::Dmg);
    restored.restore(&snapshot).expect("restore");
    for source in [&original, &restored] {
        for model in Model::ALL {
            let mut next = source
                .replacement(model, &FirmwareSet::new())
                .expect("replacement");
            assert_eq!(next.model(), model);
            assert_eq!(next.native_frame_ticks(), model.frame_ticks());
            assert_eq!(next.machine().expect("machine").peek(0xc000), 0);
            assert_eq!(next.machine().expect("machine").cpu().pc, 0x100);
            assert_eq!(next.audio_controls(), source.audio_controls());
            assert_persistent_state(&mut next);
            next.reset(ResetKind::Hard);
            assert_persistent_state(&mut next);
        }
    }
    assert_eq!(original.machine().expect("machine").peek(0xc000), 0x77);
    assert_eq!(
        original
            .machine()
            .expect("machine")
            .cartridge()
            .read_rom(0x4000),
        3
    );
}
