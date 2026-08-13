# Z80 machines should share a cadence driver, denominated in half-cycles

**Status:** Proposal, 2026-08-13. Written after the nine half-speed machines
were fixed and measured, per the order of work in
[`z80-validation-surface.md`](z80-validation-surface.md) — with twelve working
loops to generalise from rather than a guess.

## The defect was a unit mismatch, not nine typos

`Z80::tick` advances **one half-cycle**. Every hand-rolled machine loop counts
**T-states**. The conversion between the two is written by hand, once per
machine, in a function whose name asserts the wrong unit — `tick_tstate`
containing `self.cpu.tick()` is a T-state-denominated function driving a
half-cycle-denominated device, and the factor of two lives nowhere except in
the author's head.

Twelve Z80 machines in the workspace hand-roll that conversion. Nine got it
wrong. That is not a run of unrelated mistakes; it is the same mistake, because
every one of them was asked the same question and given no help answering it.

The three that got it right got it right by hand, and one of them —
`machine-jupiter-ace` — carries a comment that diagnoses the whole class
correctly and was never propagated. Diagnosing it once was not enough.

## The Spectrum family already solved it

Twelve Spectrum-family machines share `SpectrumDriver::tick_one_halfcycle`
(48K, 16K, 128K, +, +2, +2A, +2B, +3, Pentagon 128, Scorpion ZS-256, TC2048,
TS2068). None of them has this bug. The reason is structural, not diligence:

- **The driver's unit is the master half-cycle** (`hc`), not the T-state.
- **The T-state is derived from it**, never the other way round:
  `advance_tstates(n)` is `advance_halfcycles(n * cpu_divisor)`.
- **No machine calls `cpu.tick()`.** Machines implement `tick_cpu_and_bus`;
  the driver decides how often to call it.
- **The invariant is asserted, in the tree, today:**
  `debug_assert!(divisor >= 2, "CPU divisor must provide two half-cycle phases")`.
  That is precisely the check the nine machines lacked.
- **Pins are fed before the tick**, once, in one place.

A machine cannot lose the factor of two here, because it is never given the
opportunity to write it down.

## What to build

A `common-z80-machine` crate carrying **the cadence and nothing else**, modelled
on `SpectrumDriver` with the ULA and contention specifics removed:

```rust
pub trait Z80Machine {
    /// Master half-cycles per CPU T-state. Must be >= 2.
    fn cpu_divisor(&self) -> u32 { 2 }
    fn hc(&self) -> u64;
    fn hc_mut(&mut self) -> &mut u64;

    /// Drive every interrupt pin. Called immediately before each CPU tick.
    fn feed_interrupt_pins(&mut self);
    /// Exactly one `Z80::tick`, then service the bus. Called twice per T-state.
    fn tick_cpu_and_bus(&mut self);
    /// Chips on the CPU clock. Called once per T-state.
    fn tick_chips(&mut self) {}
    /// Chips finer than a T-state. Called once per master half-cycle.
    fn tick_chips_halfcycle(&mut self) {}
    /// Contention and other CPU-clock gating.
    fn cpu_clock_active(&self) -> bool { true }

    // provided: tick_one_halfcycle, advance_halfcycles, advance_tstates
}
```

**Deliberately not included: chip scheduling.** The twelve machines schedule
their chips three genuinely different ways — an integer 3:2 dot accumulator
(TMS9918 at 3.58 MHz), an exact-Hz accumulator (Einstein and MTX at 4 MHz), and
a ÷2 phase toggle (AY on MSX, SVI and Einstein). Nothing is shared there, and a
declarative rate table would be an abstraction for a future nobody has asked
for. Chips stay per-machine; only the cadence moves.

## Fix `Z80Stepper::step_tick` at the same time

The trait says:

> Advance the whole machine by one Z80 half-cycle — tick the CPU and every chip
> on its clock (**exactly one `Z80::tick`**).

Every implementation maps it to the machine's T-state function, which after
this campaign is two `Z80::tick`s. The contract has been false for as long as
the Jupiter Ace has been correct, and the CPU-rate sweep's instrument depends
on the false reading — it counted `step_tick`s per retired `NOP` and called
4.000 correct, which is only right if `step_tick` is a T-state.

This is not cosmetic. `step_instruction` documents its return value as
half-cycles, and debug and MCP single-stepping are built on both. Adopting the
driver changes `step_tick`'s meaning for twelve machines, so the migration must
decide this deliberately rather than discover it.

## What this does and does not buy

The per-machine `cpu_rate` gates added in this campaign already **detect** the
defect on all twelve machines. The driver **prevents** it. That is the whole
argument for doing the work, and it should be stated plainly rather than
dressed up: the bug is currently caught, and the case for the layer is that
nobody should have to be caught by it again. [`../../RULES.md`](../../RULES.md)
rule 30 is the governing rule — the same shape hand-rolled independently in
twelve binaries is exactly its named tell.

Keep the per-machine gates after the migration. They are three lines of
construction each and they pin the *machine*, not the driver; a shared driver
with a per-machine `cpu_divisor` can still be handed the wrong divisor.

## Open question worth a separate measurement

RULES rule 5 says the 68000 ticks at half-cycle granularity too, and the three
Amiga machines call `cpu.tick()` directly from their own loops. The same unit
mismatch is available to them, and nothing in the workspace currently compares
a 68000 instruction's cost against a known figure. A `cpu_rate`-shaped gate on
a known-cost 68000 instruction is cheap and would answer it either way. This is
a measurement to take, not a claim being made — the Z80 sweep's whole lesson is
that a single `cpu.tick()` per tick function proves nothing on its own.
