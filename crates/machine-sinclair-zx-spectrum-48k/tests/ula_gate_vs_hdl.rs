//! The engine's contention gate against the HDL, on the real machine.
//!
//! Two previous harnesses drove `FerrantiUla::tick` with synthetic pin
//! sequences and both got the driver's tick order wrong — by a half-cycle,
//! in opposite directions. A half-cycle is exactly the width of the window
//! `IOREQTW3` opens and closes in, so both produced confident verdicts on
//! the ULA-port classes that were wrong, and one of them disagreed with
//! the frame-wide differential without that being noticed.
//!
//! This one synthesises nothing. It runs the real machine, records what
//! the ULA is given and what it decides *from inside `tick`*, and replays
//! that recording through the model. There is no tick order to get wrong,
//! because the recording is taken at the only point where the question has
//! a definite answer.
//!
//! To keep the clock and raster in lockstep the model's `hc` and `CPUClk`
//! are forced from the recording each half-cycle. What is left free is
//! exactly what is under test: the combinational gate, and the two
//! latches, which the model evolves itself and which the recording also
//! carries — so the latches are cross-checked rather than assumed.
//!
//! ```sh
//! cargo test -p machine-sinclair-zx-spectrum-48k --test ula_gate_vs_hdl \
//!     --release -- --ignored --nocapture
//! ```

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::ula_engine::HDL_HC_LEAD_PIXELS;
use ferranti_ula_6c001e::hdl_model::{HdlGate, Pins};
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";
const CODE_BASE: u16 = 0x4000;
const CODE_END: u16 = 0x8000;

fn rom_bytes() -> Option<Vec<u8>> {
    std::fs::read(std::env::var(ROM_PATH_ENV).ok()?).ok()
}

/// A machine running `IN A,(C)` out of contended RAM, settled into the
/// display window.
fn prepare(port: u16, rom: &[u8]) -> Spectrum48k {
    let mut m = Spectrum48k::new();
    m.load_rom_bytes(rom).expect("48K ROM should load");
    m.reset();

    let mut addr = CODE_BASE;
    while addr < CODE_END {
        m.memory_mut().write(addr, 0xED);
        m.memory_mut().write(addr + 1, 0x78);
        addr += 2;
    }
    while m.tstate_in_frame() != 0 {
        m.advance_tstates(1);
    }
    m.z80_mut().regs.pc = CODE_BASE;
    m.z80_mut().regs.bc = port;
    // Well inside the display window, and past the settling transient.
    while m.tstate_in_frame() < 20_000 {
        m.advance_tstates(1);
    }
    m
}

struct Divergence {
    index: usize,
    pixel: u16,
    addr: u16,
    mreq: bool,
    iorq: bool,
    engine_stalled: bool,
    model_contends: bool,
}

/// The HDL's `hc` for one of our pixel counts.
///
/// The two counters run at the same rate and wrap at the same place, and
/// this harness used to hand one straight to the other. They do not share
/// an origin: the HDL presents display addresses to VRAM at `hc[3:0]` 8,
/// 9, 12, 13 and attributes at 10, 11, 14, 15, where ours fetches at
/// pixels 4–11. Four pixels, and every signal the HDL indexes by `hc`
/// inherits them — `hc[2] | hc[3]` above all, which is the whole
/// contention window.
///
/// Passing `t.pixel` through unconverted made the model disagree with the
/// engine across the entire fetch group and read as a gate defect. It was
/// an assumption in the harness. See `HDL_HC_LEAD_PIXELS`.
/// The recording carries `pixel & 0x0F`, and `hc[2]`/`hc[3]` are the only
/// bits the gate reads, so the conversion stays inside one 16-pixel cycle.
fn hdl_hc(pixel: u16) -> u16 {
    debug_assert!(pixel < 16, "the recorder stores a phase, not a counter");
    (pixel + HDL_HC_LEAD_PIXELS as u16) & 0x0F
}

/// Replay a recording through the model and return where they differ.
fn replay(trace: &[ferranti_ula_6c001e::UlaTick]) -> (Vec<Divergence>, usize) {
    let first = trace.first().expect("recording should not be empty");
    let mut gate = HdlGate::with_clock(hdl_hc(first.pixel), first.clock_high_before);
    let mut out = Vec::new();
    let mut latch_mismatches = 0usize;

    for (index, t) in trace.iter().enumerate() {
        // Lockstep on raster and clock; the gate and latches stay free.
        gate.hc = hdl_hc(t.pixel);
        gate.cpu_clk = t.clock_high_before;

        // The engine stores `mreq_t23` as the latched *active-high* pin;
        // the HDL register holds the active-low signal. Opposite senses of
        // the same latch, so this is a real cross-check rather than a
        // restatement.
        if gate.mreqt23 == t.mreq_t23_before {
            latch_mismatches += 1;
        }

        let p = Pins {
            a: t.addr,
            mreq_n: !t.mreq,
            iorq_n: !t.iorq,
        };
        let model_contends = gate.contention(p, t.video);
        let engine_stalled = !t.cpu_clock_after;

        if model_contends != engine_stalled {
            out.push(Divergence {
                index,
                pixel: t.pixel,
                addr: t.addr,
                mreq: t.mreq,
                iorq: t.iorq,
                engine_stalled,
                model_contends,
            });
        }

        gate.clk7_edge(p, t.video);
    }
    (out, latch_mismatches)
}

/// Where the engine and the HDL disagree, half-cycle by half-cycle.
///
/// Report-only: the engine is known to disagree, and the value is in
/// *which* half-cycles and on what pin state.
#[test]
#[ignore = "FIXTURE: differential harness; needs EMU198X_SPECTRUM_48K_ROM"]
fn engine_gate_against_the_hdl_on_the_real_machine() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };

    for (name, port) in [
        ("contended, ULA", 0x40FEu16),
        ("contended, odd", 0x40FF),
        ("uncontended, ULA", 0xC0FE),
        ("uncontended, odd", 0xC0FF),
    ] {
        let mut m = prepare(port, &rom);
        m.ula_mut().debug_trace_start();
        m.advance_tstates(200);
        let trace = m.ula_mut().debug_trace_take();
        let (divs, latch_mismatches) = replay(&trace);

        println!(
            "\n{name} ({port:#06x}): {} half-cycles, {} divergences, {latch_mismatches} latch mismatches",
            trace.len(),
            divs.len()
        );
        if !divs.is_empty() {
            println!(
                "  {:>5} {:>6} {:>7} {:>5} {:>5} {:>8} {:>7}",
                "idx", "pixel", "addr", "mreq", "iorq", "engine", "model"
            );
            for d in divs.iter().take(8) {
                println!(
                    "  {:>5} {:>6} {:>#7x} {:>5} {:>5} {:>8} {:>7}",
                    d.index,
                    d.pixel % 16,
                    d.addr,
                    d.mreq as u8,
                    d.iorq as u8,
                    if d.engine_stalled { "stall" } else { "run" },
                    if d.model_contends { "stall" } else { "run" }
                );
            }
        }
    }
}

/// The recorder has to be recording the right thing before its readings
/// mean anything.
///
/// Checks the recording against facts established elsewhere: it must cover
/// the requested span, the raster must advance one pixel per half-cycle,
/// and a stalled half-cycle must be one where the engine withheld the
/// clock. If any of these slip, the differential above is measuring the
/// recorder.
#[test]
#[ignore = "FIXTURE: needs EMU198X_SPECTRUM_48K_ROM"]
fn the_recorder_records_what_it_claims() {
    let Some(rom) = rom_bytes() else {
        panic!("set {ROM_PATH_ENV} to the 48K ROM to run this harness");
    };
    let mut m = prepare(0xC0FE, &rom);
    m.ula_mut().debug_trace_start();
    m.advance_tstates(50);
    let trace = m.ula_mut().debug_trace_take();

    assert!(!trace.is_empty(), "recorder produced nothing");
    assert_eq!(
        trace.len(),
        100,
        "50 T-states should be 100 ULA half-cycles; got {}",
        trace.len()
    );
    for pair in trace.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        assert_eq!(
            b.pixel,
            (a.pixel + 1) % 16,
            "the recorded phase must advance exactly one step per half-cycle"
        );
    }
    assert!(
        trace.iter().any(|t| !t.cpu_clock_after),
        "no half-cycle stalled across 50 T-states in contended RAM — the \
         recording cannot be of the code under test"
    );
}
