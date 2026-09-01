//! PET `.prg` loading through the shared `MediaSet` path (#347).
//!
//! Before this the PET had no media path at all — `media_slots` was empty and
//! `load_media` a no-op — so the only way to get code in was to type it.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use runtime_commodore_pet::{Model, PetRuntime};

/// `10 POKE32768,1` — writes screen code 1 ("A") to the top-left cell, which is
/// observable without decoding the display. PET BASIC text starts at $0401.
fn poke_screen_prg() -> Vec<u8> {
    let body: &[u8] = &[
        0x0E, 0x04, // link -> $040E, the end-of-program marker
        0x0A, 0x00, // line 10
        0x97, // POKE
        b'3', b'2', b'7', b'6', b'8', b',', b'1', 0x00, // "32768,1", end of line
        0x00, 0x00, // end of program
    ];
    let mut prg = vec![0x01, 0x04]; // load address $0401
    prg.extend_from_slice(body);
    prg
}

fn synthetic_runtime() -> PetRuntime {
    let mut kernal = vec![0xEAu8; 4 * 1024];
    kernal[0x0FFC] = 0x00;
    kernal[0x0FFD] = 0xF0;
    PetRuntime::new(
        Model::Pet40Col,
        kernal,
        vec![0u8; 8 * 1024],
        vec![0u8; 2 * 1024],
        vec![0u8; 4 * 1024],
    )
    .expect("synthetic ROM set builds")
}

fn run_frames(runtime: &mut PetRuntime, ticks: u64) {
    runtime
        .run_until(
            MachineTime::new(ticks),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("runs");
}

#[test]
fn autoload_sets_the_basic_pointers_and_queues_run() {
    // Rev 3 ROM addresses, which are not the C64/VIC-20 ones: TXTTAB $28,
    // VARTAB $2A, ARYTAB $2C, STREND $2E, keyboard buffer $026F, count $009E.
    let mut runtime = synthetic_runtime();
    let prg = poke_screen_prg();
    runtime.autoload_prg(&prg).expect("valid PRG");

    let machine = runtime.machine().expect("machine");
    let word = |addr: u16| u16::from(machine.peek(addr)) | (u16::from(machine.peek(addr + 1)) << 8);

    assert_eq!(word(0x0028), 0x0401, "TXTTAB points at the load address");
    let end = 0x0401 + (prg.len() as u16 - 2);
    for (name, addr) in [
        ("VARTAB", 0x002Au16),
        ("ARYTAB", 0x002C),
        ("STREND", 0x002E),
    ] {
        assert_eq!(word(addr), end, "{name} points just past the program");
    }

    assert_eq!(
        machine.peek(0x0401),
        0x0E,
        "the program body landed at $0401"
    );
    assert_eq!(machine.peek(0x009E), 4, "RUN\\r is queued");
    let queued: Vec<u8> = (0x026Fu16..0x0273).map(|a| machine.peek(a)).collect();
    assert_eq!(queued, b"RUN\r", "the PET's keyboard buffer is at $026F");
}

#[test]
fn a_short_image_is_rejected_at_the_slot_boundary() {
    let mut runtime = synthetic_runtime();
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "program-1",
        MediaKind::Program,
        &[0x01, 0x04],
    ));
    let error = runtime.load_media(&media).expect_err("header only");
    assert!(matches!(
        error,
        emu198x_shell::MachineError::InvalidMedia { ref slot, .. } if slot == "program-1"
    ));
}

fn rom(name: &str) -> Option<Vec<u8>> {
    let home = env::var("HOME").ok()?;
    let path = PathBuf::from(home)
        .join(".emu198x/roms/commodore-pet")
        .join(name);
    path.exists().then(|| fs::read(path).expect("read PET ROM"))
}

#[test]
#[ignore = "FIXTURE: needs PET kernal/basic/editor/char ROMs — run with --ignored"]
fn a_basic_program_loads_and_runs_itself() {
    let (Some(kernal), Some(basic), Some(editor), Some(char_rom)) = (
        rom("kernal.rom"),
        rom("basic.rom"),
        rom("editor.rom"),
        rom("char.rom"),
    ) else {
        panic!("PET ROM set not found at ~/.emu198x/roms/commodore-pet/");
    };
    let mut runtime =
        PetRuntime::new(Model::Pet40Col, kernal, basic, editor, char_rom).expect("build runtime");

    let mut media = MediaSet::new();
    let prg = poke_screen_prg();
    media.push(MediaImage::new("program-1", MediaKind::Program, &prg));
    runtime.load_media(&media).expect("PRG is accepted");

    // Boot, inject at frame 120, then let the editor pick RUN out of the buffer.
    run_frames(&mut runtime, 20_000 * 260);

    let machine = runtime.machine().expect("machine");
    assert_eq!(
        machine.peek(0x8000),
        0x01,
        "the loaded program should have POKEd screen code 1 to $8000"
    );
}
