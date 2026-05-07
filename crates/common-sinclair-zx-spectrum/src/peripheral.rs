//! The `Peripheral` trait — a uniform shape for Spectrum add-on devices.
//!
//! The 48K Spectrum was designed from day one to accept third-party
//! hardware through its edge connector: Kempston joysticks, Sinclair
//! Interface 1 (Microdrive, ZX Net, RS-232), Interface 2 (Sinclair
//! joysticks, ROM cartridges), Multiface (snapshot / cheat freeze),
//! DivMMC (SD card), mice, printers, Currah µSpeech, Specdrum, Fuller
//! Audio Box, and dozens more. Each plugs into the same I/O port bus,
//! watches the same M1 fetch stream, and some run on their own clock.
//!
//! Before Phase 0.7, every machine that supported a peripheral added
//! it as a typed field and hand-wired the port decoding into its
//! `io_read` and `io_write`. When Pentagon and Scorpion both wanted
//! Beta disk integration, the ~30 lines of claims_port / check_m1 /
//! ROM paging logic got copy-pasted.
//!
//! This trait lifts the peripheral contract into one shape. A
//! peripheral implements only the hooks it needs (everything
//! defaults), and each machine decides which peripherals to own as
//! typed fields. No `Box<dyn Peripheral>`, no runtime dispatch — the
//! trait is a *vocabulary*, not a container. When Phase 2 adds
//! Multiface / DivMMC / Kempston Mouse, each becomes an `impl
//! Peripheral` with a field on the machines that accept it.
//!
//! ## What's NOT here
//!
//! - **Memory bus intercepts.** Beta disk's TR-DOS ROM read-override,
//!   Interface 1's shadow ROM, Multiface's banked RAM/ROM. These need
//!   a distinct hook that doesn't fit the port-oriented trait. Phase
//!   0.7 defers them; Pentagon / Scorpion keep their current machine-
//!   side `read_trdos_rom` checks until a second peripheral forces
//!   the abstraction.
//!
//! - **Core machine chips.** The ULA (port `$FE`), the AY-3-8912
//!   (ports `$FFFD`/`$BFFD` on stock 128K, `$F5`/`$F6` on Timex), and
//!   the `$7FFD`/`$1FFD` memory paging ports are not peripherals —
//!   they're integral to each machine and stay hand-decoded inside
//!   `io_read`/`io_write`.
//!
//! - ~~**Kempston joystick.**~~ **Now a peripheral.** Pre-2026-05-07
//!   the trait carved out Kempston as too simple to abstract — "a
//!   single byte and a one-line port read." The reasoning ignored
//!   *optionality*: a `pub kempston: u8` field is always present, so
//!   every machine emulated a permanently-attached interface. Real
//!   hardware: most rubber-key 48Ks didn't ship with one, and the
//!   Amstrad +2A / +2B / +3 broke the rear connector pinout in 1987
//!   so classic Kempston interfaces don't physically fit those
//!   machines. The peripheral pattern lets each machine declare
//!   whether it can host a Kempston by simply not owning one. See
//!   `peripheral-kempston-joystick` for the implementation and
//!   `wiki/decisions/spectrum-joystick-architecture.md` for the
//!   rationale trail.

/// Uniform trait implemented by every Spectrum edge-connector
/// peripheral. All methods default to a no-op or a neutral return, so
/// implementors override only what they need.
pub trait Peripheral {
    /// Does this peripheral currently claim the given I/O port?
    ///
    /// May be stateful — Beta disk only claims its ports when TR-DOS
    /// is paged in, a disconnected FDC returns false regardless of
    /// port, and so on. Machines consult this on every I/O cycle
    /// before their own core-port decoding, so the implementation
    /// must be cheap.
    fn claims_port(&self, _port: u16) -> bool {
        false
    }

    /// Read from a claimed I/O port. Only called after `claims_port`
    /// has returned `true` for this port. Default returns the idle
    /// bus value (`0xFF`) for peripherals that only write.
    fn read(&mut self, _port: u16) -> u8 {
        0xFF
    }

    /// Write to a claimed I/O port. Only called after `claims_port`
    /// has returned `true`. Default discards the write for
    /// peripherals that only read.
    fn write(&mut self, _port: u16, _val: u8) {}

    /// Observe an M1 opcode fetch.
    ///
    /// Called on every M1 cycle of the Z80 (once per executed
    /// instruction), regardless of `claims_port`. Used by peripherals
    /// that trap on specific address patterns:
    ///
    /// - **Beta disk** watches for fetches in `$3D00-$3DFF` while in
    ///   ROM space to toggle TR-DOS paging.
    /// - **Interface 1** watches for fetches at `$0008`, `$1708`, and
    ///   others to switch in its shadow ROM.
    /// - **Multiface** traps NMI vector fetches at `$0066`.
    ///
    /// Peripherals that need to know "am I in ROM space?" can compute
    /// `addr < 0x4000` themselves — every Spectrum variant has ROM in
    /// the first 16 KB slot.
    fn on_m1(&mut self, _addr: u16) {}

    /// Per-half-cycle tick.
    ///
    /// Called from `SpectrumDriver::tick_peripherals` once per
    /// T-state. Used by time-varying peripherals:
    ///
    /// - Disk drives for spindle rotation and head stepping timers.
    /// - Mice for delta decay.
    /// - Printers for bit-serial timing.
    /// - Microdrives for tape-head position.
    ///
    /// The `hc` argument is the machine's current half-cycle counter
    /// within the frame, useful for deriving a T-state number.
    /// Default no-op.
    fn tick(&mut self, _hc: u32) {}
}
