//! A newly supported board survives the real postcard runtime boundary.
use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_nes::{Model, NesRuntime};

#[test]
fn selected_program_and_character_banks_survive_runtime_snapshot() {
    let mut bytes = vec![0xFF; 16 + 4 * 32768 + 4 * 8192];
    bytes[..16].fill(0);
    bytes[..4].copy_from_slice(b"NES\x1a");
    bytes[4] = 8;
    bytes[5] = 4;
    bytes[6] = 0x20;
    bytes[7] = 0x40;
    for bank in 0..4usize {
        bytes[16 + bank * 32768 + 0x1000] = 0x70 + bank as u8;
    }
    for model in [Model::NesNtsc, Model::NesPal] {
        let mut runtime = NesRuntime::blank(model);
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &bytes));
        runtime.load_media(&media).expect("GxROM loads");
        runtime
            .machine_mut()
            .expect("loaded machine")
            .poke(0x8000, 0x32);
        let saved = runtime.snapshot().expect("save selected banks");
        runtime
            .machine_mut()
            .expect("loaded machine")
            .poke(0x8000, 0);
        assert_eq!(
            runtime.machine().expect("loaded machine").peek(0x9000),
            0x70
        );
        let mut restored = NesRuntime::blank(model);
        restored
            .restore(&saved)
            .expect("fresh runtime restores board");
        assert_eq!(
            restored.machine().expect("restored machine").peek(0x9000),
            0x73
        );
        assert_eq!(restored.snapshot().expect("unchanged snapshot"), saved);
    }
}
