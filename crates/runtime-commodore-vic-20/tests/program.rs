//! VIC-20 `.prg` loading through the shared `MediaSet` path (#363).

use std::path::PathBuf;

use emu198x_shell::{HeadlessSession, MediaImage, MediaKind, MediaSet};
use runtime_commodore_vic_20::{Model, Vic20RamExpansion, Vic20Runtime, Vic20SessionQueryProvider};

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

    assert_eq!(session.machine().ram_expansion(), Vic20RamExpansion::EXP_8K);
    session.run_frames(151).expect("boot and delayed autoload");
    let machine = session.machine().machine().expect("machine");
    assert_eq!(machine.peek(0x1201), 0x0B);
    assert_eq!(machine.peek(0x1205), 0x9E);
}

#[test]
#[ignore = "FIXTURE: needs VIC-20 ROM set — run with --ignored"]
fn expanded_basic_prg_executes_its_sys_entry_point() {
    let (Some(kernal), Some(basic), Some(char_rom)) =
        (rom("kernal.rom"), rom("basic.rom"), rom("char.rom"))
    else {
        panic!("VIC-20 ROM set not found at ~/.emu198x/roms/commodore-vic-20/");
    };
    let runtime =
        Vic20Runtime::new(Model::Vic20Pal, kernal, basic, char_rom).expect("build runtime");
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, 71 * 312, Vic20SessionQueryProvider);

    // 10 SYS4624, followed at $1210 by LDA #$2A / STA $0340 / RTS.
    let mut prg = vec![
        0x01, 0x12, 0x0B, 0x12, 0x0A, 0x00, 0x9E, b'4', b'6', b'2', b'4', 0, 0, 0,
    ];
    prg.resize(2 + (0x1210 - 0x1201), 0);
    prg.extend_from_slice(&[0xA9, 0x2A, 0x8D, 0x40, 0x03, 0x60]);

    let mut media = MediaSet::new();
    media.push(MediaImage::new("program-1", MediaKind::Program, &prg));
    session.prepare(&media, &[]).expect("PRG is accepted");
    session.run_frames(240).expect("boot, autoload, and RUN");

    let machine = session.machine().machine().expect("machine");
    assert_eq!(
        machine.peek(0x0340),
        0x2A,
        "SYS entry point did not execute"
    );
}

#[test]
#[ignore = "FIXTURE: needs VIC-20 ROM set — run with --ignored"]
fn expanded_basic_prg_loads_through_the_top_of_the_block_1_expansion() {
    // #1349: $1201 inference selected 3 KiB low + 5 KiB high, so RAM stopped at
    // $33FF and every byte of a larger program above it read back as open bus.
    const MARKER_ADDR: u16 = 0x3495;
    const MARKER: u8 = 0x5A;

    let (Some(kernal), Some(basic), Some(char_rom)) =
        (rom("kernal.rom"), rom("basic.rom"), rom("char.rom"))
    else {
        panic!("VIC-20 ROM set not found at ~/.emu198x/roms/commodore-vic-20/");
    };
    let runtime =
        Vic20Runtime::new(Model::Vic20Pal, kernal, basic, char_rom).expect("build runtime");
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, 71 * 312, Vic20SessionQueryProvider);

    // 10 SYS4624, a $0000 end-of-program link, then filler up to the marker.
    let mut prg = vec![
        0x01, 0x12, // load address $1201
        0x0B, 0x12, 0x0A, 0x00, 0x9E, b'4', b'6', b'2', b'4', 0x00, // 10 SYS4624
        0x00, 0x00, // end of program
    ];
    prg.resize(2 + usize::from(MARKER_ADDR - 0x1201), 0xAA);
    prg.push(MARKER);

    let mut media = MediaSet::new();
    media.push(MediaImage::new("program-1", MediaKind::Program, &prg));
    session.prepare(&media, &[]).expect("PRG is accepted");

    assert_eq!(
        session.machine().ram_expansion(),
        Vic20RamExpansion::EXP_8K,
        "$1201 needs BLK1, the full $2000-$3FFF block"
    );
    session.run_frames(151).expect("boot and delayed autoload");

    let machine = session.machine().machine().expect("machine");
    assert_eq!(machine.peek(0x1201), 0x0B, "program start");
    assert_eq!(
        machine.peek(MARKER_ADDR),
        MARKER,
        "byte above $33FF was dropped instead of loaded"
    );
}
