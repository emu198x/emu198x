//! VIC-20 cartridge loading and KERNAL autostart through `MediaSet` (#360).

use std::path::PathBuf;

use emu198x_shell::{HeadlessSession, MachineCore, MediaImage, MediaKind, MediaSet, ResetKind};
use runtime_commodore_vic_20::{Model, Vic20Runtime, Vic20SessionQueryProvider};

fn rom(name: &str) -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home)
        .join(".emu198x/roms/commodore-vic-20")
        .join(name);
    path.exists()
        .then(|| std::fs::read(path).expect("read VIC-20 ROM"))
}

/// A raw 8 KiB BLK5 ROM whose cold-start writes a sentinel then loops.
fn autostart_rom() -> Vec<u8> {
    let mut rom = vec![0xEA; 0x2000];
    rom[0..4].copy_from_slice(&[0x09, 0xA0, 0x09, 0xA0]);
    rom[4..9].copy_from_slice(&[0x41, 0x30, 0xC3, 0xC2, 0xCD]);
    // $A009: LDA #$42; STA $0200; JMP $A00E.
    rom[9..17].copy_from_slice(&[0xA9, 0x42, 0x8D, 0x00, 0x02, 0x4C, 0x0E, 0xA0]);
    rom
}

#[test]
#[ignore = "FIXTURE: needs VIC-20 ROM set — run with --ignored"]
fn kernal_cold_starts_a_blk5_cartridge_loaded_as_standard_media() {
    let (Some(kernal), Some(basic), Some(char_rom)) =
        (rom("kernal.rom"), rom("basic.rom"), rom("char.rom"))
    else {
        panic!("VIC-20 ROM set not found at ~/.emu198x/roms/commodore-vic-20/");
    };
    let mut runtime =
        Vic20Runtime::new(Model::Vic20Pal, kernal, basic, char_rom).expect("build runtime");
    let cartridge = autostart_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "cartridge-1",
        MediaKind::Cartridge,
        &cartridge,
    ));
    runtime.load_media(&media).expect("cartridge is accepted");
    runtime.reset(ResetKind::Hard);
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, 71 * 312, Vic20SessionQueryProvider);
    session.run_frames(2).expect("KERNAL probes cartridge");

    let machine = session.machine().machine().expect("machine");
    assert_eq!(machine.peek(0x0200), 0x42, "cartridge cold-start ran");
    assert_eq!(machine.peek(0xA004), 0x41, "BLK5 remains mapped");
}
