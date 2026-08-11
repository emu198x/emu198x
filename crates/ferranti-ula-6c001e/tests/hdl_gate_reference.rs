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

/// `hc` is 9 bits and wraps at 447 — 448 `clk7` cycles per line, which is
/// the same rate as our own `pixel` counter.
const HC_PER_LINE: u16 = 448;

/// The HDL's contention block, as state.
///
/// Register names and polarities are the HDL's, not ours. `ioreqtw3` and
/// `mreqt23` hold *active-low* signals, so "high" means "no request was
/// latched" — the opposite sense to `UlaEngine::mreq_t23`. Keeping the
/// HDL's polarity is the point: a transcription that silently renames
/// things is where a misread hides.
#[derive(Clone, Copy, Debug)]
struct HdlGate {
    hc: u16,
    cpu_clk: bool,
    ioreqtw3: bool,
    mreqt23: bool,
}

/// The Z80 pins the gate looks at, in the HDL's active-low convention.
#[derive(Clone, Copy, Debug)]
struct Pins {
    a: u16,
    mreq_n: bool,
    iorq_n: bool,
}

impl HdlGate {
    fn new(hc: u16) -> Self {
        Self {
            hc,
            cpu_clk: false,
            ioreqtw3: true,
            mreqt23: true,
        }
    }

    /// `CLKContention`, combinational.
    fn contention(&self, p: Pins, border_n: bool) -> bool {
        let ioreq_n = (p.a & 1 != 0) | p.iorq_n;
        let ula_io = !ioreq_n;
        let a14 = p.a & 0x4000 != 0;
        let a15 = p.a & 0x8000 != 0;
        let window = (self.hc & 0x04 != 0) | (self.hc & 0x08 != 0);

        // `~Nor1` — every term negated and ANDed. The `ula_io` disjuncts
        // are the short-circuit: when the ULA answers the port, both
        // address conditions hold whatever the address is.
        let nor1 = (a14 || ula_io)
            && (!a15 || ula_io)
            && window
            && border_n
            && self.ioreqtw3
            && self.cpu_clk
            && self.mreqt23;

        // `~Nor2` — the same, minus the address terms and `mreqt23`,
        // requiring the ULA to be answering.
        let nor2 = window && border_n && self.cpu_clk && ula_io && self.ioreqtw3;

        nor1 || nor2
    }

    /// One `posedge clk7`. Returns whether `CPUClk` *changed*, which is
    /// when the Z80 advances.
    ///
    /// Both edges, not just the rise: the Z80 is a half-cycle state
    /// machine, and its pins move on each `CPUClk` transition. Advancing
    /// only on the rise makes every T-state take two, which is exactly
    /// what the acceptance test caught the first time this was written.
    /// While the gate holds `CPUClk` high there is no transition, and that
    /// is what a stall costs.
    fn clk7_edge(&mut self, p: Pins, border_n: bool) -> bool {
        let contention = self.contention(p, border_n);

        let was_high = self.cpu_clk;
        // `if (CPUClk && !CLKContention) CPUClk <= 0; else CPUClk <= 1;`
        self.cpu_clk = !(was_high && !contention);
        self.hc = (self.hc + 1) % HC_PER_LINE;

        // `always @(posedge CPUClk)` — a derived clock, so it fires in the
        // same delta as the `clk7` edge that raised it, sampling the pins
        // as they stand.
        if !was_high && self.cpu_clk {
            self.ioreqtw3 = (p.a & 1 != 0) | p.iorq_n;
            self.mreqt23 = p.mreq_n;
        }
        was_high != self.cpu_clk
    }
}

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
/// `/MREQ` falls halfway through `T1` and the refresh address in `T3`/`T4`
/// is uncontended (`I` sits in the ROM page after reset). `/IORQ` falls
/// halfway through `T2` of the I/O cycle and is released at the end of
/// `TW` — traced on the engine and consistent with the Z80's own timing.
fn in_a_c_pins(code_addr: u16, port: u16) -> Vec<HalfCycle> {
    const REFRESH: u16 = 0x3F00; // I/R — uncontended
    let mut pins = Vec::new();

    // Two M1 fetches, four T-states each.
    for step in 0..2u16 {
        let pc = code_addr + step;
        // T1: /MREQ high, then low.
        pins.push(hc_pin(pc, true, true));
        pins.push(hc_pin(pc, false, true));
        // T2: /MREQ low throughout.
        pins.push(hc_pin(pc, false, true));
        pins.push(hc_pin(pc, false, true));
        // T3, T4: refresh — address uncontended, /MREQ low for the strobe.
        pins.push(hc_pin(REFRESH, false, true));
        pins.push(hc_pin(REFRESH, false, true));
        pins.push(hc_pin(REFRESH, true, true));
        pins.push(hc_pin(REFRESH, true, true));
    }

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
