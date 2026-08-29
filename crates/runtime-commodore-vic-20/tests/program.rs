//! VIC-20 `.prg` loading through the shared `MediaSet` path (#363).

use std::path::PathBuf;

use emu198x_shell::{HeadlessSession, MediaImage, MediaKind, MediaSet};
use runtime_commodore_vic_20::{Model, Vic20Runtime, Vic20SessionQueryProvider};

fn rom(name: &str) -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home)
        .join(".emu198x/roms/commodore-vic-20")
        .join(name);
    path.exists()
        .then(|| std::fs::read(path).expect("read VIC-20 ROM"))
}

#[test]
#[ignore = "FIXTURE: needs VIC-20 ROM set — run with --ignored"]
fn expanded_basic_prg_selects_8k_loads_and_queues_run() {
    let (Some(kernal), Some(basic), Some(char_rom)) =
        (rom("kernal.rom"), rom("basic.rom"), rom("char.rom"))
    else {
        panic!("VIC-20 ROM set not found at ~/.emu198x/roms/commodore-vic-20/");
    };
    let runtime =
        Vic20Runtime::new(Model::Vic20Pal, kernal, basic, char_rom).expect("build runtime");
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, 71 * 312, Vic20SessionQueryProvider);
    let prg = [
        0x01, 0x12, 0x0B, 0x12, 0x0A, 0x00, 0x9E, b'4', b'6', b'2', b'4', 0,
    ];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("program-1", MediaKind::Program, &prg));
    session.prepare(&media, &[]).expect("PRG is accepted");

    assert_eq!(session.machine().ram_expansion_kb(), 8);
    session.run_frames(151).expect("boot and delayed autoload");
    let machine = session.machine().machine().expect("machine");
    assert_eq!(machine.peek(0x1201), 0x0B);
    assert_eq!(machine.peek(0x1205), 0x9E);
}
