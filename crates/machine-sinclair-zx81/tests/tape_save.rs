//! `SAVE` produces a tape a `LOAD` can read back.
//!
//! The other half of `tape_load.rs`, and the half that exercises the ULA's
//! cassette *output*: the ROM's own `SAVE` drives the MIC line, the machine
//! records the transitions, and a second machine loads the result. Nothing
//! synthesises a waveform here — the only encoder involved is Sinclair's.
//!
//! # The lead-in is not optional
//!
//! `insert_tape` takes edge times relative to now, so the obvious way to
//! replay a recording is to subtract its first timestamp. That produces a tape
//! whose first pulse arrives immediately, and the ROM will not read it: it sits
//! in its `LOAD` loop for the whole tape and accepts **zero** bytes.
//!
//! Measured, and the check that it is the lead-in rather than anything about
//! the data: doing the same rebase to a `to_pulses` train that loads perfectly
//! well stops it loading too. That encoder starts its first pulse 1,500,000
//! master clocks in — a little under half a second — and that quiet is what
//! the loader needs to settle on. Everything else about the two waveforms is
//! the same, which was worth establishing the hard way: identical pulse widths
//! (492/483), identical inter-bit gaps (4872/5008), and bursts of 8 and 18
//! edges — four and nine pulses, the ZX81's 0 and 1 — matching burst for burst
//! through the system-variable header.

use machine_sinclair_zx81::{Zx81, Zx81Key};
use std::{env, fs};

/// What `Zx81Image::to_pulses` puts before its first pulse.
const LEAD_IN: u64 = 1_500_000;

fn tap(machine: &mut Zx81, key: Zx81Key) {
    machine.press_key(key);
    for _ in 0..25 {
        machine.run_frame();
    }
    machine.release_key(key);
    for _ in 0..120 {
        machine.run_frame();
    }
}

fn shifted(machine: &mut Zx81, key: Zx81Key) {
    machine.press_key(Zx81Key::Shift);
    machine.press_key(key);
    for _ in 0..25 {
        machine.run_frame();
    }
    machine.release_key(key);
    machine.release_key(Zx81Key::Shift);
    for _ in 0..120 {
        machine.run_frame();
    }
}

fn newline(machine: &mut Zx81) {
    machine.press_key(Zx81Key::Newline);
    for _ in 0..25 {
        machine.run_frame();
    }
    machine.release_key(Zx81Key::Newline);
}

fn booted(rom: Vec<u8>) -> Zx81 {
    let mut machine = Zx81::new(rom, 16384).expect("machine");
    for _ in 0..400 {
        machine.run_frame();
    }
    machine
}

#[test]
#[ignore = "needs an 8 KB ZX81 ROM — set EMU198X_ZX81_ROM"]
fn a_saved_program_loads_back() {
    let Ok(rom_path) = env::var("EMU198X_ZX81_ROM")
        .or_else(|_| env::var("HOME").map(|h| format!("{h}/.emu198x/roms/sinclair-zx81/zx81.rom")))
    else {
        emu198x_test_skip::skip!("no ZX81 ROM");
    };
    let Ok(rom) = fs::read(&rom_path) else {
        emu198x_test_skip::skip!("ZX81 ROM not staged at {rom_path}");
    };

    // Type `1 REM`, then SAVE it. The name matters: `SAVE ""` is report F,
    // invalid file name, and the machine goes back to the editor having
    // recorded nothing but its own idle keyboard scanning.
    let mut saver = booted(rom.clone());
    tap(&mut saver, Zx81Key::N1);
    tap(&mut saver, Zx81Key::E);
    newline(&mut saver);
    for _ in 0..120 {
        saver.run_frame();
    }
    tap(&mut saver, Zx81Key::S);
    shifted(&mut saver, Zx81Key::P);
    tap(&mut saver, Zx81Key::A);
    shifted(&mut saver, Zx81Key::P);

    saver.start_tape_recording();
    newline(&mut saver);
    for _ in 0..900 {
        saver.run_frame();
    }
    let edges = saver.take_tape_recording();
    assert_eq!(
        saver.peek(0x4000),
        0xFF,
        "the save should not have reported an error"
    );
    assert!(
        edges.len() > 10_000,
        "a save of this program is tens of thousands of transitions, not {}",
        edges.len()
    );

    // Trim the idle keyboard scanning either side of the waveform: the save
    // proper is where the 150 us pulse starts.
    let start = (0..edges.len() - 3)
        .find(|&i| {
            let gap = |k: usize| edges[i + k + 1] - edges[i + k];
            (480..500).contains(&gap(0)) && (480..500).contains(&gap(1))
        })
        .expect("a SAVE waveform among the recorded transitions");
    let tape: Vec<u64> = edges[start..]
        .iter()
        .map(|edge| edge - edges[start] + LEAD_IN)
        .collect();

    // LOAD "" into a fresh machine.
    let mut loader = booted(rom);
    tap(&mut loader, Zx81Key::J);
    shifted(&mut loader, Zx81Key::P);
    shifted(&mut loader, Zx81Key::P);
    newline(&mut loader);
    // Thread the tape only once the loader is listening; see `tape_load.rs`.
    for _ in 0..40 {
        loader.run_frame();
    }
    loader.insert_tape(&tape);
    let mut frames = 0;
    while loader.tape_remaining() > 0 && frames < 30_000 {
        loader.run_frame();
        frames += 1;
    }
    for _ in 0..200 {
        loader.run_frame();
    }

    let line_number = (u16::from(loader.peek(0x407D)) << 8) | u16::from(loader.peek(0x407E));
    let token = loader.peek(0x4081);
    assert_eq!(line_number, 1, "the saved line number should come back");
    assert_eq!(token, 0xEA, "and the REM token with it");
}
