# Preserve fractional +3 read-byte timing

Lane: Spectrum launch-hardening. Owns part of #1831, after #1833.

The existing controller rounds each 32 us MFM byte up to 114 CPU ticks (FM:
64 us to 228). At the emulated 3,546,895 Hz this accumulates error. The NEC March 1979
application note p.1 distinguishes nominal byte intervals from worst-case
interrupt intervals; its 13 us figure is not an exact measured overrun cutoff.
Do not implement an early cutoff from that figure. Physical jitter, silicon
revision and request latency remain unresolved. Current nominal-slot overrun
policy remains an approximation, not a conformance claim.

1. Add public-register tests in `crates/nec-upd765a/tests/read_pacing.rs`:
   absolute first/next request boundaries for all three read commands, FM/MFM,
   full payload and both immediate and late service. Demonstrate baseline red.
2. In `crates/nec-upd765a/src/lib.rs`, compute successive read boundaries as
   differences of rounded absolute times. Derive phase from existing per-field
   byte position; retain serialized fields/version and fixed arrival timing.
   Keep write/scan/format timing outside this change.
3. Update snapshot boundary tests in
   `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs` and native-state
   replay tests to exercise the alternating intervals without adding state.
4. Run controller/runtime/machine tests, Clippy/fmt and normal Spectrum release
   build. Repeat cold-ROM read regression twice, with wrong-signature control.
   Preserve source identities, captured mastering distinction and safe evidence.
5. Record measured outcome, remaining exact-service uncertainty, push and open
   an owning PR. Track normal v0.38.0 publication separately. Clean task builds
   and worktree after commits/evidence are retained remotely.

Primary sources held in umbrella reference/by-topic/fdc-765:
NEC December 1978 datasheet p.12 (13/27 us); NEC March 1979 application note
p.1 (16/32 us nominal, 13/29 us worst case), p.3 (8 MHz clock). The figures
are at the documented standard-drive rate, not direct +3 service deadlines.
+3 service schematic p.19 shows the SED9420 clock path. MAME specpls3.cpp
configures 16 MHz / 4; MAME's shared FIFO and Fuse's coarse timeout are not
exact silicon timing oracles. Existing captured observations remain separate.
