//! Cartridge expansion audio reaches the console's mix: a 6502 program
//! running on the machine programs each chip, and the machine's audio
//! buffer carries the result. The chips' own pitch, volume and timing are
//! tested in `nintendo-nes-cartridge`; this checks the wiring from the
//! cartridge pin to the output.

use common_nintendo_nes::Nes;
use nintendo_nes_cartridge::{Fme7, Mapper, Namco163, Vrc6, Vrc7};

/// A 32 KiB PRG image whose last 8 KiB, at `$E000`, holds a program that
/// performs `writes` in order, pausing after each so a slow chip (the
/// VRC7's synthesiser) sees every one, then loops forever.
fn prg_with_writes(writes: &[(u16, u8)]) -> Vec<u8> {
    let mut code = Vec::new();
    for &(addr, value) in writes {
        code.extend([0xA9, value]); // LDA #value
        code.extend([0x8D, addr as u8, (addr >> 8) as u8]); // STA addr
        code.extend([0xA2, 0x20]); // LDX #$20
        code.extend([0xCA, 0xD0, 0xFD]); // DEX / BNE -3
    }
    let loop_at = 0xE000 + code.len() as u16;
    code.extend([0x4C, loop_at as u8, (loop_at >> 8) as u8]); // JMP self
    let mut prg = vec![0u8; 32 * 1024];
    let base = 24 * 1024;
    prg[base..base + code.len()].copy_from_slice(&code);
    prg[0x7FFC] = 0x00;
    prg[0x7FFD] = 0xE0;
    prg
}

/// The audio buffer's peak-to-peak swing after a few frames.
fn swing(mapper: Box<dyn Mapper>) -> f32 {
    let mut nes = Nes::new(mapper);
    for _ in 0..6 {
        nes.run_frame();
    }
    let audio = nes.take_audio_buffer();
    let tail = &audio[audio.len() / 2..];
    let max = tail.iter().copied().fold(f32::MIN, f32::max);
    let min = tail.iter().copied().fold(f32::MAX, f32::min);
    max - min
}

fn assert_sounds(name: &str, silent: Box<dyn Mapper>, playing: Box<dyn Mapper>) {
    let quiet = swing(silent);
    let loud = swing(playing);
    assert!(
        loud > quiet + 0.01,
        "{name}: playing swing {loud} should clear the silent swing {quiet}"
    );
}

#[test]
fn vrc6_pulse_reaches_the_mix() {
    let writes = [(0x9000, 0x7F), (0x9001, 0xFF), (0x9002, 0x81)];
    assert_sounds(
        "VRC6",
        Box::new(Vrc6::new(prg_with_writes(&[]), vec![0; 8192], false)),
        Box::new(Vrc6::new(prg_with_writes(&writes), vec![0; 8192], false)),
    );
}

#[test]
fn sunsoft_5b_tone_reaches_the_mix() {
    let writes = [
        (0xC000, 0),
        (0xE000, 0x80),
        (0xC000, 7),
        (0xE000, 0x3E),
        (0xC000, 8),
        (0xE000, 15),
    ];
    assert_sounds(
        "5B",
        Box::new(Fme7::new(prg_with_writes(&[]), vec![0; 8192])),
        Box::new(Fme7::new(prg_with_writes(&writes), vec![0; 8192])),
    );
}

#[test]
fn namco_163_wave_reaches_the_mix() {
    let mut writes = vec![(0xF800, 0x80)];
    writes.extend([0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0].map(|b| (0x4800, b)));
    writes.push((0xF800, 0x80 | 0x78));
    writes.extend([0x00, 0x00, 0x00, 0x00, 0xF1, 0x00, 0x00, 0x0F].map(|b| (0x4800, b)));
    assert_sounds(
        "N163",
        Box::new(Namco163::new(prg_with_writes(&[]), vec![0; 8192])),
        Box::new(Namco163::new(prg_with_writes(&writes), vec![0; 8192])),
    );
}

#[test]
fn vrc7_note_reaches_the_mix() {
    let writes = [
        (0x9010, 0x30),
        (0x9030, 0x10),
        (0x9010, 0x10),
        (0x9030, 0xAC),
        (0x9010, 0x20),
        (0x9030, 0x18),
    ];
    assert_sounds(
        "VRC7",
        Box::new(Vrc7::new(prg_with_writes(&[]), vec![0; 8192])),
        Box::new(Vrc7::new(prg_with_writes(&writes), vec![0; 8192])),
    );
}
