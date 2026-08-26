//! End-to-end keyboard test: drive the session by host key *name* and confirm
//! the runtime's key map, POKEY keyboard hardware, OS conversion, and BASIC
//! all line up to evaluate a typed expression.
//!
//! Gated behind the local XL OS + BASIC ROM bundle.

use std::path::PathBuf;

use emu198x_shell::{HeadlessSession, InputEvent, MediaSet};
use runtime_atari_800xl::{Atari800xlRuntime, Atari800xlSessionQueryProvider, Model};

const FRAME_TICKS_NTSC: u64 = 262 * 228;

fn rom(name: &str) -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    std::fs::read(
        PathBuf::from(home)
            .join(".emu198x/roms/atari-800xl")
            .join(name),
    )
    .ok()
}

fn key(name: &str, pressed: bool) -> InputEvent {
    InputEvent::Key {
        name: name.to_owned().into(),
        pressed,
    }
}

#[test]
#[ignore = "FIXTURE: requires local OS + BASIC ROMs at ~/.emu198x/roms/atari-800xl/"]
fn typing_print_expression_evaluates() {
    let (Some(os), Some(basic)) = (rom("atarixl.rom"), rom("ataribas.rom")) else {
        emu198x_test_skip::skip!("skipping: ROMs not present");
    };

    let runtime = Atari800xlRuntime::new(Model::A800xlNtsc, Some(os), Some(basic), None, true)
        .expect("runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        FRAME_TICKS_NTSC,
        Atari800xlSessionQueryProvider,
    );
    session.prepare(&MediaSet::new(), &[]).expect("prepare");

    // Boot to the BASIC READY prompt.
    session.run_frames(600).expect("boot");

    // Type `PRINT 6*7` + RETURN by host key name — exercising the runtime's
    // name → POKEY scan code map.
    for name in ["P", "R", "I", "N", "T", "space", "6", "*", "7", "Return"] {
        session.queue_input(key(name, true));
        session.run_frames(3).expect("hold");
        session.queue_input(key(name, false));
        session.run_frames(6).expect("settle");
    }
    session.run_frames(30).expect("evaluate");

    // Read screen RAM via the display-list LMS and look for "42"
    // (display codes '4'=$14, '2'=$12).
    let machine = session.machine().machine().expect("machine present");
    let ram = machine.ram();
    let dlist = u16::from(ram[0x0230]) | (u16::from(ram[0x0231]) << 8);
    let screen = first_lms_target(ram, dlist).expect("LMS in display list");
    let found = (0..40 * 24 - 1).any(|j| ram[screen + j] == 0x14 && ram[screen + j + 1] == 0x12);
    assert!(
        found,
        "typing `PRINT 6*7` by key name did not yield `42` — the name → scan \
         code → BASIC path is broken"
    );
}

fn first_lms_target(ram: &[u8], dlist: u16) -> Option<usize> {
    let mut p = dlist as usize;
    for _ in 0..64 {
        let b = ram[p];
        let mode = b & 0x0F;
        let lms = b & 0x40 != 0;
        if lms && mode >= 0x02 {
            return Some(usize::from(ram[p + 1]) | (usize::from(ram[p + 2]) << 8));
        }
        match mode {
            0x01 => return None,
            _ if lms => p += 3,
            _ => p += 1,
        }
    }
    None
}
