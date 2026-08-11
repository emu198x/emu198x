//! The contention gate, transcribed from the HDL and scored against FUSE.
//!
//! Two frame-wide differentials have now put the engine's I/O contention
//! next to FUSE's and shown it wrong, and two attempts to fix the gate
//! failed — one made it worse, one changed nothing at all. Both were
//! scored through a whole machine, one frame at a time, which is the wrong
//! resolution: the gate works in half-cycles and the disagreement lives
//! there.
//!
//! So this file steps down a level. It transcribes the contention block of
//! `opencores.org/projects/zx_ula` — vendored at
//! `198x/emulators/zx-spectrum/zx_ula/`, `fpga_version/rtl/ula.v`, and
//! named by Smith's Chapter 18 §7 as the authority for the Issue 3 6C001
//! topology — and runs it as an executable model.
//!
//! ```verilog
//! wire ioreq_n = a[0] | iorq_n;
//! wire Nor1 = (~(a[14] | ~ioreq_n))
//!           | (~(~a[15] | ~ioreq_n))
//!           | (~(hc[2] | hc[3]))
//!           | (~Border_n | ~ioreqtw3 | ~CPUClk | ~mreqt23);
//! wire Nor2 = (~(hc[2] | hc[3])) | ~Border_n | ~CPUClk | ioreq_n | ~ioreqtw3;
//! wire CLKContention = ~Nor1 | ~Nor2;
//! always @(posedge clk7) begin
//!   if (CPUClk && !CLKContention) CPUClk <= 0; else CPUClk <= 1;
//! end
//! always @(posedge CPUClk) begin
//!   ioreqtw3 <= ioreq_n;
//!   mreqt23  <= mreq_n;
//! end
//! ```
//!
//! ## Why a transcription is trustworthy here
//!
//! It isn't, on its own — a transcription is just another reading, and a
//! misread is exactly what put two failed attempts in the tree. What makes
//! it usable is that it has an **independent acceptance test**: it must
//! reproduce FUSE's four-way port table, `C:1 C:3` / `C:1 C:1 C:1 C:1` /
//! `N:1 C:3` / `N:4`, for every arrival phase, under a single rotation.
//! Until it does, a disagreement means the transcription is wrong. Only
//! after it does is it an oracle worth scoring the engine against.
//!
//! Nothing here touches `FerrantiUla`. That is deliberate: the model has
//! to be established before it can judge anything.
//!
//! ## What it is and is not sensitive to
//!
//! Established by mutation, because a gate nobody has tried to break is
//! not evidence. It **catches** every defect found through it so far, and
//! all three were in the Z80's pins rather than in the ULA:
//!
//! - the CPU advancing on one clock edge instead of two;
//! - `/IORQ` released a T-state early (end of `TW`, not end of `T3`);
//! - `/IORQ` asserted half a T-state early (on `T2`'s edge rather than
//!   half a clock after the address is stable, which is Zilog's wording).
//!
//! The last two each name `uncontended, ULA` as the failing class, which
//! is exactly the class the engine gets wrong.
//!
//! It **cannot distinguish** `Nor1`'s `IORQ` short-circuit from `Nor2`.
//! Deleting either alone still reproduces the table; only deleting both
//! breaks it. They are redundant paths to the same result for these four
//! classes, so an earlier claim that the `Nor1` short-circuit is "the
//! whole answer to the port classes" was too strong — `Nor2` reaches the
//! same answer on its own. Separating them needs a fifth class, and none
//! of the obvious ones does it.
//!
//! ```sh
//! cargo test -p ferranti-ula-6c001e --test hdl_gate_reference -- --nocapture
//! ```

// The gate expressions below mirror the Verilog line for line, including
// its double negations. Simplifying them would break the correspondence
// that makes this transcription auditable against its source, which is the
// only reason it is trustworthy at all.
#![allow(clippy::nonminimal_bool)]

// The model itself lives in the crate, not here. A second transcription
// in this file is what it was written to avoid: two copies drift, and a
// drifting reference is worse than none.
use ferranti_ula_6c001e::hdl_model::{HdlGate, Pins};

/// One Z80 T-state's worth of pins, as the gate sees them.
///
/// The Z80 changes its pins on half-cycle boundaries, so each T-state is
/// two entries — the gate is evaluated at that resolution and a signal
/// that moves mid-T-state (`/IORQ` does) cannot be represented otherwise.
#[derive(Clone, Copy)]
struct HalfCycle {
    a: u16,
    mreq_n: bool,
    iorq_n: bool,
}

const fn hc_pin(a: u16, mreq_n: bool, iorq_n: bool) -> HalfCycle {
    HalfCycle { a, mreq_n, iorq_n }
}

/// `IN A,(C)` as a half-cycle pin sequence: `M1`, `M1`, then the I/O cycle.
///
/// `/IORQ` falls halfway through `T2` and is held to the end of `T3` —
/// both established by the acceptance test below, and both places our own
/// Z80 currently differs.
fn in_a_c_pins(code_addr: u16, port: u16) -> Vec<HalfCycle> {
    let mut pins = Vec::new();
    m1_pins(code_addr, &mut pins);
    m1_pins(code_addr + 1, &mut pins);

    // I/O cycle: T1, T2, TW, T3. No /MREQ at any point.
    pins.push(hc_pin(port, true, true)); // T1
    pins.push(hc_pin(port, true, true));
    pins.push(hc_pin(port, true, true)); // T2 — /IORQ falls mid-T-state
    pins.push(hc_pin(port, true, false));
    pins.push(hc_pin(port, true, false)); // TW
    pins.push(hc_pin(port, true, false));
    pins.push(hc_pin(port, true, false)); // T3 — /IORQ held to the end
    pins.push(hc_pin(port, true, false));

    pins
}

/// One `M1` opcode fetch, four T-states, as half-cycle pins.
///
/// `/MREQ` falls halfway through `T1` and is released at the end of `T2`;
/// the refresh strobe then falls halfway through `T3` and is released
/// during `T4`. The refresh address is uncontended — `I` sits in the ROM
/// page after reset — so it cannot contend, but it still drives `MREQT23`,
/// which is what makes its phase matter at the *next* `T1`.
fn m1_pins(pc: u16, out: &mut Vec<HalfCycle>) {
    const REFRESH: u16 = 0x3F00; // I/R — uncontended

    out.push(hc_pin(pc, true, true)); // T1 — /MREQ falls mid-T-state
    out.push(hc_pin(pc, false, true));
    out.push(hc_pin(pc, false, true)); // T2 — low throughout
    out.push(hc_pin(pc, false, true));
    out.push(hc_pin(REFRESH, true, true)); // T3 — refresh strobe falls
    out.push(hc_pin(REFRESH, false, true));
    out.push(hc_pin(REFRESH, false, true)); // T4 — released
    out.push(hc_pin(REFRESH, true, true));
}

/// One memory read M-cycle, three T-states, as half-cycle pins.
///
/// `/MREQ` falls halfway through `T1`, the same as the opcode fetch, and
/// stays low to the end of the cycle — there is no refresh here to release
/// it early, which is exactly what distinguishes this from `M1` and why
/// the re-arm bug showed on `LD A,(HL)` and not on `NOP`.
fn mem_read_pins(addr: u16, out: &mut Vec<HalfCycle>) {
    out.push(hc_pin(addr, true, true)); // T1 — /MREQ falls mid-T-state
    out.push(hc_pin(addr, false, true));
    out.push(hc_pin(addr, false, true)); // T2
    out.push(hc_pin(addr, false, true));
    out.push(hc_pin(addr, false, true)); // T3 — held to the end
    out.push(hc_pin(addr, false, true));
}

/// `LD A,(HL)` — `M1` plus one contended memory read.
///
/// The two-M-cycle case the `MREQT23` re-arm bug was originally diagnosed
/// on: the read was charged twice, once before the access committed and
/// again immediately after, because `/MREQ` deasserts while the contended
/// address is still on the bus.
fn ld_a_hl_pins(code_addr: u16, data_addr: u16) -> Vec<HalfCycle> {
    let mut pins = Vec::new();
    m1_pins(code_addr, &mut pins);
    mem_read_pins(data_addr, &mut pins);
    pins
}

/// `LD BC,(nn)` — `ED` prefix, opcode, two operand fetches, two reads.
///
/// Six M-cycles, the longest case in the per-instruction oracle and the
/// one whose error scaled with M-cycle count rather than instruction
/// length.
fn ld_bc_nn_pins(code_addr: u16, data_addr: u16) -> Vec<HalfCycle> {
    let mut pins = Vec::new();
    m1_pins(code_addr, &mut pins);
    m1_pins(code_addr + 1, &mut pins);
    mem_read_pins(code_addr + 2, &mut pins);
    mem_read_pins(code_addr + 3, &mut pins);
    mem_read_pins(data_addr, &mut pins);
    mem_read_pins(data_addr + 1, &mut pins);
    pins
}

/// Cost of a sequence of M-cycles from frame T-state `t0`, per FUSE —
/// contention charged once at the start of each.
fn fuse_mcycle_cost(t0: u32, mcycles: &[u32]) -> u32 {
    let mut t = t0;
    for length in mcycles {
        t += fuse_delay(t);
        t += length;
    }
    t - t0
}

/// A run of `count` `NOP`s — nothing but back-to-back `M1` fetches out of
/// contended RAM.
///
/// This is the shape that isolates the `+1`. It has no I/O in it at all,
/// so `IOREQTW3` never moves and every term but the address decode and
/// `MREQT23` drops out of the gate.
fn nop_stream_pins(code_addr: u16, count: u16) -> Vec<HalfCycle> {
    let mut pins = Vec::new();
    for i in 0..count {
        m1_pins(code_addr.wrapping_add(i), &mut pins);
    }
    pins
}

/// Run the pin sequence through the gate and return its cost in T-states.
///
/// The CPU advances one half-cycle per `CPUClk` rise, which is what makes
/// a stall cost time: while the gate holds `CPUClk` high the sequence does
/// not move on, and the `clk7` edges spent waiting are the contention.
fn hdl_cost(hc0: u16, pins: &[HalfCycle], border_n: bool) -> u32 {
    let mut gate = HdlGate::new(hc0);
    let mut index = 0usize;
    let mut edges = 0u32;

    while index < pins.len() {
        let p = Pins {
            a: pins[index].a,
            mreq_n: pins[index].mreq_n,
            iorq_n: pins[index].iorq_n,
        };
        if gate.clk7_edge(p, border_n) {
            index += 1;
        }
        edges += 1;
        assert!(edges < 4096, "gate never released the clock");
    }
    // Two `clk7` edges per T-state when nothing contends.
    edges / 2
}

// ---------------------------------------------------------------------
// FUSE's model, for the acceptance test. Transcribed in
// `machine-sinclair-zx-spectrum-48k/tests/io_contention_oracle.rs`; the
// window here is the bare 8-T-state pattern, since only the shape within
// a contended line matters for comparing phases.
// ---------------------------------------------------------------------

const PATTERN: [u32; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

fn fuse_delay(t: u32) -> u32 {
    PATTERN[(t % 8) as usize]
}

fn port_page_contended(port: u16) -> bool {
    (0x4000..0x8000).contains(&port)
}

fn port_from_ula(port: u16) -> bool {
    port & 1 == 0
}

fn fuse_in_a_c_cost(t0: u32, port: u16) -> u32 {
    let mut t = t0;
    for _ in 0..2 {
        t += fuse_delay(t);
        t += 4;
    }
    // `ula_contend_port_early`
    if port_page_contended(port) {
        t += fuse_delay(t);
    }
    t += 1;
    // `ula_contend_port_late`
    if port_from_ula(port) {
        t += fuse_delay(t);
        t += 2;
    } else if port_page_contended(port) {
        t += fuse_delay(t);
        t += 1;
        t += fuse_delay(t);
        t += 1;
        t += fuse_delay(t);
    } else {
        t += 2;
    }
    t += 1;
    t - t0
}

struct Class {
    name: &'static str,
    port: u16,
    shape: &'static str,
}

fn classes() -> Vec<Class> {
    vec![
        Class {
            name: "contended, ULA",
            port: 0x40FE,
            shape: "C:1 C:3",
        },
        Class {
            name: "contended, odd",
            port: 0x40FF,
            shape: "C:1 C:1 C:1 C:1",
        },
        Class {
            name: "uncontended, ULA",
            port: 0xC0FE,
            shape: "N:1 C:3",
        },
        Class {
            name: "uncontended, odd",
            port: 0xC0FF,
            shape: "N:4",
        },
    ]
}

/// The acceptance test, and the whole reason this file exists.
///
/// The HDL's `hc` and FUSE's frame T-state have no shared origin — that
/// alignment is the same seam the engine's own pixel phase sits on, and
/// nothing here fixes it. So the model is scored the only way that does
/// not smuggle in an assumption: across every rotation, looking for one
/// that reconciles **all four classes at once**. Four classes with
/// different shapes cannot be rotated into agreement by luck.
///
/// It prints the full phase table either way, so a mismatch says *where*
/// rather than just *that*.
#[test]
fn the_hdl_model_reproduces_fuse_four_way_table() {
    let code = 0x4000u16; // contended, so both M1 fetches contend

    println!("\nHDL cost by hc phase (T-states), Border_n = 1");
    print!("{:<20} {:<16}", "class", "shape");
    for hc0 in 0..16u16 {
        print!("{hc0:>4}");
    }
    println!();
    println!("{}", "-".repeat(36 + 64));

    let mut hdl = Vec::new();
    for class in classes() {
        let pins = in_a_c_pins(code, class.port);
        let row: Vec<u32> = (0..16u16).map(|hc0| hdl_cost(hc0, &pins, true)).collect();
        print!("{:<20} {:<16}", class.name, class.shape);
        for v in &row {
            print!("{v:>4}");
        }
        println!();
        hdl.push((class.name, class.port, row));
    }

    println!("\nFUSE cost by frame T-state phase (T-states)");
    print!("{:<20} {:<16}", "class", "shape");
    for phase in 0..8u32 {
        print!("{phase:>4}");
    }
    println!();
    println!("{}", "-".repeat(36 + 32));

    let mut fuse = Vec::new();
    for class in classes() {
        let row: Vec<u32> = (0..8u32).map(|p| fuse_in_a_c_cost(p, class.port)).collect();
        print!("{:<20} {:<16}", class.name, class.shape);
        for v in &row {
            print!("{v:>4}");
        }
        println!();
        fuse.push((class.name, row));
    }

    // Look for one rotation reconciling every class. `hc` runs at twice
    // FUSE's rate, so a T-state boundary is every second `hc` — both
    // parities are tried, because which one the CPU starts on is itself a
    // convention rather than something established.
    println!("\nrotations that reconcile all four classes:");
    let mut found = Vec::new();
    for parity in 0..2u16 {
        for rot in 0..8u16 {
            let agrees = hdl.iter().zip(fuse.iter()).all(|((_, _, h), (_, f))| {
                (0..8usize).all(|p| {
                    let hc0 = ((p as u16 + rot) % 8) * 2 + parity;
                    h[hc0 as usize] == f[p]
                })
            });
            if agrees {
                println!("  parity {parity}, rotation {rot}");
                found.push((parity, rot));
            }
        }
    }
    if found.is_empty() {
        println!("  none");
    }

    // Per-class rotations, so a near-miss is legible rather than a bare
    // "no". If three classes agree at one rotation and the fourth does
    // not, that names which branch of the transcription is wrong.
    println!("\nper-class rotations (parity, rotation):");
    for ((name, _, h), (_, f)) in hdl.iter().zip(fuse.iter()) {
        let mut ok = Vec::new();
        for parity in 0..2u16 {
            for rot in 0..8u16 {
                if (0..8usize).all(|p| {
                    let hc0 = ((p as u16 + rot) % 8) * 2 + parity;
                    h[hc0 as usize] == f[p]
                }) {
                    ok.push(format!("({parity},{rot})"));
                }
            }
        }
        println!(
            "  {name:<20} {}",
            if ok.is_empty() {
                "none".to_string()
            } else {
                ok.join(" ")
            }
        );
    }

    assert!(
        !found.is_empty(),
        "the HDL transcription does not reproduce FUSE's four-way table under \
         any single rotation — which means the transcription is wrong, not the \
         engine. The per-class rotations above name the suspect: a class \
         agreeing at a rotation the others share is fine, one agreeing nowhere \
         is the broken branch. Both failures found this way so far were in the \
         pin sequence rather than the gate — the CPU advancing on one clock \
         edge instead of two, and /IORQ released a T-state early."
    );
}

/// Half-cycle trace of the I/O cycle, for the class the table rejects.
#[test]
#[ignore = "diagnostic"]
fn trace_the_failing_class() {
    for port in [0x40FEu16, 0x40FF] {
        let pins = in_a_c_pins(0x4000, port);
        let mut gate = HdlGate::new(3);
        let mut index = 0usize;
        println!("\n=== port {port:#06x} ===");
        println!(" edge  idx  addr   mreq_n iorq_n | hc%16 win tw3 t23 clk  cont  advance");
        for edge in 0..80 {
            if index >= pins.len() {
                break;
            }
            let p = Pins {
                a: pins[index].a,
                mreq_n: pins[index].mreq_n,
                iorq_n: pins[index].iorq_n,
            };
            let win = (gate.hc & 0x04 != 0) | (gate.hc & 0x08 != 0);
            let cont = gate.contention(p, true);
            let (tw3, t23, clk, hcm) = (gate.ioreqtw3, gate.mreqt23, gate.cpu_clk, gate.hc % 16);
            let adv = gate.clk7_edge(p, true);
            println!(
                "{edge:>5} {index:>4} {:#06x} {:>6} {:>6} | {hcm:>5} {:>3} {:>3} {:>3} {:>3} {:>5} {:>8}",
                p.a,
                p.mreq_n as u8,
                p.iorq_n as u8,
                win as u8,
                tw3 as u8,
                t23 as u8,
                clk as u8,
                cont as u8,
                adv as u8
            );
            if adv {
                index += 1;
            }
        }
    }
}

/// Cost of `count` back-to-back `NOP`s from frame T-state `t0`, per FUSE.
///
/// FUSE models an `M1` as one `contend_read( pc, 4 )` — contention charged
/// once at the start of the M-cycle, then four T-states.
fn fuse_nop_stream_cost(t0: u32, count: u16) -> u32 {
    let mut t = t0;
    for _ in 0..count {
        t += fuse_delay(t);
        t += 4;
    }
    t - t0
}

/// The `+1`, isolated.
///
/// Every configuration of the contention fix so far leaves the `N:4` port
/// classes exactly one T-state high whenever `MREQT23` is wired in. Those
/// classes have no I/O contention at all, so the residual is pure `M1`
/// memory contention and can be reproduced without an I/O cycle anywhere
/// near it. That is what this runs: nothing but back-to-back opcode
/// fetches out of contended RAM.
///
/// The question it answers is which side owns the `+1`. If the HDL and
/// FUSE agree here under the same rotation the I/O table found, then the
/// reference and the reference emulator are consistent on memory
/// contention and the residual is ours — and, given that both `/IORQ`
/// defects turned out to be pin *phase*, `/MREQ`'s phase is the obvious
/// next suspect. If they disagree, the two authorities differ about memory
/// contention and that is a much larger finding than a stray T-state.
#[test]
fn the_hdl_model_reproduces_fuse_m1_contention() {
    const RUN: u16 = 8;
    let pins = nop_stream_pins(0x4000, RUN);

    let hdl: Vec<u32> = (0..16u16).map(|hc0| hdl_cost(hc0, &pins, true)).collect();
    let fuse: Vec<u32> = (0..8u32).map(|t| fuse_nop_stream_cost(t, RUN)).collect();

    println!("\n{RUN} back-to-back NOPs out of contended RAM");
    print!("{:<22}", "HDL by hc phase");
    for v in &hdl {
        print!("{v:>4}");
    }
    println!();
    print!("{:<22}", "FUSE by T-state phase");
    for v in &fuse {
        print!("{v:>4}");
    }
    println!();

    let mut found = Vec::new();
    for parity in 0..2u16 {
        for rot in 0..8u16 {
            if (0..8usize).all(|p| hdl[(((p as u16 + rot) % 8) * 2 + parity) as usize] == fuse[p]) {
                found.push((parity, rot));
            }
        }
    }
    println!("\nrotations that reconcile: {found:?}");

    // The rotation the I/O table settled on. If memory contention
    // reconciles at a *different* one, the two are not describing the same
    // alignment and one of the pin sequences is wrong — which is exactly
    // how both `/IORQ` defects surfaced.
    let io_rotation = (1u16, 1u16);
    println!(
        "I/O table's rotation {io_rotation:?} is {} here",
        if found.contains(&io_rotation) {
            "shared"
        } else {
            "NOT shared"
        }
    );

    // Where the disagreement sits at the I/O rotation, per phase.
    println!("\n{:>6} {:>8} {:>8} {:>7}", "phase", "HDL", "FUSE", "diff");
    for p in 0..8usize {
        let h = hdl[(((p as u16 + io_rotation.1) % 8) * 2 + io_rotation.0) as usize];
        let f = fuse[p];
        println!("{p:>6} {h:>8} {f:>8} {:>+7}", h as i64 - f as i64);
    }

    assert!(
        !found.is_empty(),
        "the HDL and FUSE do not agree on pure M1 contention under any \
         rotation. That is not a stray T-state — it means the gate-level \
         source and the reference emulator disagree about memory \
         contention, and the engine has been scored against FUSE all along."
    );
}

// =====================================================================
// The engine, driven by the same pins.
//
// Everything above establishes the model. This scores `FerrantiUla`
// against it, at the same half-cycle resolution and with the same pin
// sequences — which is the whole reason the model was built.
//
// Doing it this way removes the guesswork that sank the two earlier
// attempts. Those were scored through a whole machine, so the engine's
// contention was entangled with the Z80's pin timing, and a defect in
// either looked identical from outside. Here the pins are *given*, taken
// from the same source the model uses, so any divergence is the gate's.
// =====================================================================

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use common_sinclair_zx_spectrum::ula::Ula;
use ferranti_ula_6c001e::{FerrantiUla, UlaRevision};

/// Memory that answers only the question the gate asks.
struct ContendedLower16K;

impl MemoryBus for ContendedLower16K {
    fn read(&self, _addr: u16) -> u8 {
        0
    }
    fn write(&mut self, _addr: u16, _value: u8) {}
    fn is_contended(&self, addr: u16) -> bool {
        (0x4000..0x8000).contains(&addr)
    }
}

/// Cost of a pin sequence through the engine, entered at ULA pixel phase
/// `pixel_phase`.
///
/// Only one alignment needs sweeping, not two: the pixel counter and
/// `z80_clock_high` both advance once per tick, so fixing the pixel phase
/// fixes the clock phase with it.
///
/// Returns `None` if the sequence ran out of the display window, where the
/// comparison would be meaningless — the model holds `Border_n` asserted
/// throughout and the engine cannot.
fn engine_cost(pixel_phase: u16, pins: &[HalfCycle]) -> Option<u32> {
    let mut ula = FerrantiUla::new(UlaRevision::Ferranti6C);
    let mem = ContendedLower16K;
    let mut fb = vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT];

    // Settle to the wanted phase, early enough in the contended window
    // that the sequence has room to finish inside it. Idle pins on an
    // uncontended address cannot contend, so the counters just advance.
    let mut settle = 0u32;
    loop {
        let (_, pixel, video, _, _) = ula.debug_raster();
        if video && pixel % 16 == pixel_phase && pixel < 128 {
            break;
        }
        ula.tick(&mem, 0x0000, false, false, false, &mut fb);
        settle += 1;
        if settle > 1_000_000 {
            return None;
        }
    }

    let mut index = 0usize;
    let mut ticks = 0u32;
    while index < pins.len() {
        let p = pins[index];
        ula.tick(&mem, p.a, !p.mreq_n, !p.iorq_n, false, &mut fb);
        if ula.cpu_clock_active() {
            index += 1;
        }
        ticks += 1;
        if ticks > 4096 {
            return None;
        }
    }

    let (_, _, video, _, _) = ula.debug_raster();
    if !video {
        return None; // ran past the window; not comparable
    }
    Some(ticks / 2)
}

/// Score the engine's gate against the model, pin for pin.
///
/// **Unsound as written — do not trust its verdict on the engine.**
/// `driver.rs` calls `tick_ula()` *before* `tick_cpu_and_bus()`, so the
/// real gate always sees the pins left by the previous CPU tick. This
/// feeds it the current entry instead, which shifts the comparison by a
/// half-cycle. That is exactly the width of the window `IOREQTW3` opens
/// and closes in, so it changes the verdict on the two ULA-port classes
/// and leaves the rest looking right — which is how it disagreed with the
/// frame-wide differential while appearing to pass.
///
/// The model-against-FUSE tests above are unaffected: no engine, no
/// driver, no offset. Only the engine-scoring tests need this fixed, and
/// naively lagging the pin feed is not the fix — it was tried and made
/// every case worse, because during a stall the CPU does not tick and the
/// lag must not accumulate.
///
/// Both produce a 16-entry table indexed by their own phase counter, and
/// neither counter's origin is fixed by anything here — so, as everywhere
/// else in this file, the comparison is a search for **one rotation shared
/// by every case**. `NOP` and the four port classes together pin it: five
/// tables with four distinct shapes cannot be rotated into agreement by
/// accident.
///
/// Report-only. The engine is known to disagree — that is the open work —
/// and the value is in *where*, which the per-case rotations give.
#[test]
#[ignore = "diagnostic; scores the engine against the model"]
fn the_engine_gate_against_the_hdl_model() {
    struct Case {
        name: &'static str,
        pins: Vec<HalfCycle>,
    }

    let mut cases = vec![Case {
        name: "NOP x2",
        pins: nop_stream_pins(0x4000, 2),
    }];
    for class in classes() {
        cases.push(Case {
            name: class.name,
            pins: in_a_c_pins(0x4000, class.port),
        });
    }

    println!("\ncost by phase — model (m) against engine (e)");
    for case in &cases {
        let model: Vec<u32> = (0..16u16)
            .map(|hc0| hdl_cost(hc0, &case.pins, true))
            .collect();
        let engine: Vec<Option<u32>> = (0..16u16)
            .map(|phase| engine_cost(phase, &case.pins))
            .collect();

        print!("\n{:<18} m", case.name);
        for v in &model {
            print!("{v:>4}");
        }
        println!();
        print!("{:<18} e", "");
        for v in &engine {
            match v {
                Some(x) => print!("{x:>4}"),
                None => print!("{:>4}", "-"),
            }
        }
        println!();

        // Only every second phase is reachable. A Z80 T-state is two ULA
        // ticks, so the CPU always enters an M-cycle on the same parity —
        // the odd column here is a state the real machine never occupies,
        // and the engine's flat "no contention" reading there is an
        // artefact of driving it into one. Judging on all sixteen would
        // condemn the gate for something it is never asked to do.
        let diffs: Vec<i64> = (0..16usize)
            .step_by(2)
            .filter_map(|p| engine[p].map(|e| e as i64 - model[p] as i64))
            .collect();
        let uniform = diffs
            .first()
            .copied()
            .filter(|d| diffs.iter().all(|x| x == d));
        println!(
            "{:<18}   reachable phases: {}",
            "",
            match uniform {
                Some(0) => "exact".to_string(),
                Some(d) => format!("uniformly {d:+} T-states"),
                None => format!("varies: {diffs:?}"),
            }
        );
    }
}

/// The last coverage gap: memory read M-cycles that are not `M1`.
///
/// `MREQT23` exists for these. An `M1` escapes the re-arm bug because what
/// follows its access is the refresh, whose address is uncontended — so
/// `NOP` agreeing proves nothing about the latch. These are the cases the
/// bug was diagnosed on, and the model has to be established on them
/// before the engine can be judged against it.
#[test]
fn the_hdl_model_reproduces_fuse_multi_mcycle_contention() {
    struct Case {
        name: &'static str,
        pins: Vec<HalfCycle>,
        mcycles: &'static [u32],
    }

    let cases = vec![
        Case {
            name: "LD A,(HL)",
            pins: ld_a_hl_pins(0x4000, 0x5000),
            mcycles: &[4, 3],
        },
        Case {
            name: "LD BC,(nn)",
            pins: ld_bc_nn_pins(0x4000, 0x5000),
            mcycles: &[4, 4, 3, 3, 3, 3],
        },
    ];

    // The rotation the I/O and M1 tables both settled on. A third and
    // fourth case agreeing at the same one is what makes it an alignment
    // rather than a fitted constant.
    const SHARED: (u16, u16) = (1, 1);

    for case in &cases {
        let hdl: Vec<u32> = (0..16u16)
            .map(|hc0| hdl_cost(hc0, &case.pins, true))
            .collect();
        let fuse: Vec<u32> = (0..8u32)
            .map(|t| fuse_mcycle_cost(t, case.mcycles))
            .collect();

        println!("\n{} {:?}", case.name, case.mcycles);
        print!("{:<22}", "HDL by hc phase");
        for v in &hdl {
            print!("{v:>4}");
        }
        println!();
        print!("{:<22}", "FUSE by T-state phase");
        for v in &fuse {
            print!("{v:>4}");
        }
        println!();

        let mut found = Vec::new();
        for parity in 0..2u16 {
            for rot in 0..8u16 {
                if (0..8usize)
                    .all(|p| hdl[(((p as u16 + rot) % 8) * 2 + parity) as usize] == fuse[p])
                {
                    found.push((parity, rot));
                }
            }
        }
        println!("  rotations: {found:?}");
        print!("  vs FUSE at {SHARED:?}:");
        for p in 0..8usize {
            let h = hdl[(((p as u16 + SHARED.1) % 8) * 2 + SHARED.0) as usize];
            print!(" {:+}", h as i64 - fuse[p] as i64);
        }
        println!();

        assert!(
            found.contains(&SHARED),
            "{}: the HDL and FUSE do not agree at the rotation every other \
             case shares. Either the memory-read pin sequence is wrong, or \
             the two authorities differ on multi-M-cycle contention — and \
             that would matter far more than a stray T-state.",
            case.name
        );
    }
}

/// The engine on the same multi-M-cycle sequences.
///
/// This is what settles `MREQT23`. If the engine over-charges these by a
/// contention window while `NOP` stays exact, the re-arm bug is real and
/// the latch is its fix; if it does not, the latch is solving a problem
/// the engine no longer has.
#[test]
#[ignore = "diagnostic; scores the engine against the model"]
fn the_engine_on_multi_mcycle_sequences() {
    let cases = [
        ("LD A,(HL)", ld_a_hl_pins(0x4000, 0x5000)),
        ("LD BC,(nn)", ld_bc_nn_pins(0x4000, 0x5000)),
        ("NOP x2", nop_stream_pins(0x4000, 2)),
    ];

    println!("\ncost by phase — model (m) against engine (e)");
    for (name, pins) in &cases {
        let model: Vec<u32> = (0..16u16).map(|hc0| hdl_cost(hc0, pins, true)).collect();
        let engine: Vec<Option<u32>> = (0..16u16).map(|phase| engine_cost(phase, pins)).collect();

        print!("\n{name:<14} m");
        for v in &model {
            print!("{v:>4}");
        }
        println!();
        print!("{:<14} e", "");
        for v in &engine {
            match v {
                Some(x) => print!("{x:>4}"),
                None => print!("{:>4}", "-"),
            }
        }
        println!();

        let diffs: Vec<i64> = (0..16usize)
            .step_by(2)
            .filter_map(|p| engine[p].map(|e| e as i64 - model[p] as i64))
            .collect();
        let uniform = diffs
            .first()
            .copied()
            .filter(|d| diffs.iter().all(|x| x == d));
        println!(
            "{:<14}   reachable phases: {}",
            "",
            match uniform {
                Some(0) => "exact".to_string(),
                Some(d) => format!("uniformly {d:+} T-states"),
                None => format!("varies: {diffs:?}"),
            }
        );
    }
}
