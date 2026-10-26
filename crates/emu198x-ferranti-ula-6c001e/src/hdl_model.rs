//! The ULA contention gate as the HDL builds it, runnable.
//!
//! A transcription of the contention block of
//! `opencores.org/projects/zx_ula` — vendored at
//! `198x/emulators/zx-spectrum/zx_ula/`, `fpga_version/rtl/ula.v`, and
//! named by Smith's Chapter 18 §7 as the authority for the Issue 3 6C001
//! topology.
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
//! It lives in the crate rather than in a test so there is exactly one
//! copy: the acceptance tests establish it against FUSE, and the
//! machine-level differential scores the engine against it. Two
//! transcriptions would drift, and a drifting reference is worse than
//! none.
//!
//! Not part of the public API — `#[doc(hidden)]`, for tests only.

// The expressions below mirror the Verilog line for line, including its
// double negations. Simplifying them would break the correspondence that
// makes this transcription auditable against its source, which is the only
// reason it is trustworthy at all.
#![allow(clippy::nonminimal_bool)]

/// `hc` is 9 bits and wraps at 447 — 448 `clk7` cycles per line, the same
/// rate as the engine's own pixel counter.
pub const HC_PER_LINE: u16 = 448;

/// The Z80 pins the gate looks at, in the HDL's active-low convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pins {
    pub a: u16,
    pub mreq_n: bool,
    pub iorq_n: bool,
}

/// The HDL's contention block, as state.
///
/// Register names and polarities are the HDL's, not ours. `ioreqtw3` and
/// `mreqt23` hold *active-low* signals, so "high" means "no request was
/// latched" — the opposite sense to `UlaEngine::mreq_t23`. Keeping the
/// HDL's polarity is the point: a transcription that silently renames
/// things is where a misread hides.
#[derive(Clone, Copy, Debug)]
pub struct HdlGate {
    pub hc: u16,
    pub cpu_clk: bool,
    pub ioreqtw3: bool,
    pub mreqt23: bool,
}

impl HdlGate {
    #[must_use]
    pub fn new(hc: u16) -> Self {
        Self::with_clock(hc, false)
    }

    /// The initial `CPUClk` phase is a free choice, and not a harmless
    /// one: it decides whether a `posedge` falls before or after `/IORQ`
    /// goes low, and so whether `IOREQTW3` latches in time to cancel the
    /// contention `T2` is supposed to charge.
    #[must_use]
    pub fn with_clock(hc: u16, cpu_clk: bool) -> Self {
        Self {
            hc,
            cpu_clk,
            ioreqtw3: true,
            mreqt23: true,
        }
    }

    /// `CLKContention`, combinational.
    #[must_use]
    pub fn contention(&self, p: Pins, border_n: bool) -> bool {
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
    /// machine and its pins move on each `CPUClk` transition. While the
    /// gate holds `CPUClk` high there is no transition, and that is what a
    /// stall costs.
    pub fn clk7_edge(&mut self, p: Pins, border_n: bool) -> bool {
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
