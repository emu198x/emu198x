//! Per-instruction contention oracle: measure what the engine actually
//! costs, and diff it against the canonical delay pattern.
//!
//! The ZXSpectrum4.net timing suite grades a whole loop pass/fail. That is
//! enough to say "contention is wrong" and nothing at all about *which*
//! cycles are wrong, which is why several structural changes to the gate
//! were tried and reverted without ever being scored at a useful
//! granularity. This harness closes that gap: it runs one known
//! instruction out of contended RAM for exactly one frame and compares the
//! retired-instruction count against the count the canonical model
//! predicts.
//!
//! The canonical side is computed here rather than taken from an external
//! table, from the delay pattern and display geometry in
//! `reference/by-system/sinclair-zx-spectrum/ula-timing-expanded.md`:
//! `[6,5,4,3,2,1,0,0]` repeating over the first 128 T-states of each of the
//! 192 display lines. Contention is applied once at the start of each
//! M-cycle, per Smith's `CLKWAIT = (C3 OR C2) AND /Border AND A14 AND /A15
//! AND /MREQT23` (Chapter 18) — the wait holds until the access commits,
//! so an M-cycle is charged one delay, not one per T-state.
//!
//! The model is anchored by two instructions the engine is already known
//! to get exactly right — `NOP` and `INC BC`, both single-M-cycle — so a
//! divergence on the multi-M-cycle cases is a statement about the engine,
//! not about the model.
//!
//! Report-only by design: it prints a table and asserts only that every
//! case ran. Turning any individual case into a gate is a separate
//! decision, taken once the engine agrees with the oracle.
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-48k \
//!     --test contention_oracle -- --ignored --nocapture
//! ```

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";

/// T-states in a 48K frame.
const FRAME_TSTATES: u32 = 69888;
/// First T-state of the display area.
const FIRST_DISPLAY: u32 = 14336;
/// T-states per scan line.
const PER_LINE: u32 = 224;
/// Display lines that carry contention.
const DISPLAY_LINES: u32 = 192;
/// Contended T-states at the start of each display line.
const CONTENDED_PER_LINE: u32 = 128;
/// The canonical delay pattern across an 8-T-state contention slot.
const PATTERN: [u32; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

/// Contended RAM: the whole lower 16K.
const CODE_BASE: u16 = 0x4000;
const CODE_END: u16 = 0x8000;

/// Delay a contended M-cycle starting at frame T-state `t` incurs.
fn delay_at(t: u32) -> u32 {
    if t < FIRST_DISPLAY {
        return 0;
    }
    let into_display = t - FIRST_DISPLAY;
    if into_display / PER_LINE >= DISPLAY_LINES {
        return 0;
    }
    let in_line = into_display % PER_LINE;
    if in_line >= CONTENDED_PER_LINE {
        return 0;
    }
    PATTERN[(in_line % 8) as usize]
}

/// Instructions the canonical model completes in one frame, given an
/// instruction's M-cycle lengths — contention charged once per M-cycle.
fn canonical_per_frame(mcycles: &[u32]) -> u64 {
    let mut t = 0u32;
    let mut retired = 0u64;
    while t < FRAME_TSTATES {
        for length in mcycles {
            t += delay_at(t);
            t += length;
        }
        retired += 1;
    }
    retired
}

/// One instruction under test.
struct Case {
    name: &'static str,
    /// Bytes of the instruction, repeated to fill contended RAM.
    bytes: &'static [u8],
    /// M-cycle lengths, in order. Contention applies at each start.
    mcycles: &'static [u32],
    /// Register setup applied before the measured frame.
    setup: fn(&mut Spectrum48k),
}

fn cases() -> Vec<Case> {
    vec![
        // Anchors: single M-cycle, already known correct. If either of
        // these diverges the model is wrong, not the engine.
        Case {
            name: "NOP",
            bytes: &[0x00],
            mcycles: &[4],
            setup: |_| {},
        },
        Case {
            name: "INC BC",
            bytes: &[0x03],
            mcycles: &[6],
            setup: |_| {},
        },
        // Two M-cycles: fetch plus one contended read.
        Case {
            name: "LD A,(HL)",
            bytes: &[0x7E],
            mcycles: &[4, 3],
            setup: |m| m.z80_mut().regs.hl = 0x5000,
        },
        // Six M-cycles. This is the shape the failing suite cases use.
        // Operand bytes address $5000, itself contended.
        Case {
            name: "LD BC,(nn)",
            bytes: &[0xED, 0x4B, 0x00, 0x50],
            mcycles: &[4, 4, 3, 3, 3, 3],
            setup: |_| {},
        },
        // Same shape, writing rather than reading. BC is preloaded with
        // the two bytes already at $5000 so the write is value-neutral
        // and the instruction stream stays intact.
        Case {
            name: "LD (nn),BC",
            bytes: &[0xED, 0x43, 0x00, 0x50],
            mcycles: &[4, 4, 3, 3, 3, 3],
            setup: |m| m.z80_mut().regs.bc = 0x43ED,
        },
    ]
}

fn rom_bytes() -> Option<Vec<u8>> {
    let path = std::env::var(ROM_PATH_ENV).ok()?;
    std::fs::read(path).ok()
}

/// Run one case for exactly one frame and return instructions retired.
fn measure(case: &Case, rom: &[u8]) -> u64 {
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(rom).expect("48K ROM should load");
    machine.reset();

    // Fill all contended RAM with the instruction under test, so every
    // fetch and every operand access is contended.
    let mut addr = CODE_BASE;
    let mut index = 0usize;
    while addr < CODE_END {
        machine
            .memory_mut()
            .write(addr, case.bytes[index % case.bytes.len()]);
        index += 1;
        addr += 1;
    }

    // Align to a frame boundary so the measured window is exactly one
    // frame of contention, then aim the CPU at the filled region. The ROM
    // never runs, so IFF1 stays clear and no interrupt perturbs the count.
    while machine.tstate_in_frame() != 0 {
        machine.advance_tstates(1);
    }
    machine.z80_mut().regs.pc = CODE_BASE;
    (case.setup)(&mut machine);

    let before = machine.z80().instructions_retired();
    machine.advance_tstates(FRAME_TSTATES);
    machine.z80().instructions_retired() - before
}

#[test]
#[ignore = "diagnostic harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn contention_matches_the_canonical_model_per_instruction() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    println!(
        "\n{:<14} {:>10} {:>10} {:>9}  M-cycles",
        "instruction", "canonical", "measured", "excess"
    );
    println!("{}", "-".repeat(62));

    let mut ran = 0;
    for case in cases() {
        let canonical = canonical_per_frame(case.mcycles);
        let measured = measure(&case, &rom);
        // Fewer instructions retired means more time lost to waits.
        let excess = (canonical as f64 - measured as f64) / canonical as f64 * 100.0;
        println!(
            "{:<14} {:>10} {:>10} {:>8.1}%  {:?}",
            case.name, canonical, measured, excess, case.mcycles
        );
        ran += 1;
    }

    assert_eq!(ran, cases().len(), "every case should have been measured");
}
