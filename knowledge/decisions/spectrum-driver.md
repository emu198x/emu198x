# Decision: `SpectrumDriver` — one run loop across the Spectrum family

**Date:** April 2026 (Phase 0.6)

## The decision

Inside the Spectrum family, every machine shares the same per-frame cadence through a provided-method trait: `SpectrumDriver` in `common-sinclair-zx-spectrum::driver`. Each machine impls ~8 short hooks that expose its specific chips, and picks up `run_frame` for free.

This does **not** contradict [System-specific run loops](system-specific-run-loops.md). That decision is about cross-system universality (Spectrum vs C64 vs NES vs Amiga). Within the Spectrum family — 11 variants on three ULAs plus Pentagon/Scorpion clones — the cadence is genuinely the same, and duplicating it seven ways was actively harmful to accuracy fixes.

## Why

Before Phase 0.6, seven `machine-*` crates held near-identical 30-50 line `run_frame` implementations that differed only in:

1. Which `TIMING_*` constant they used.
2. Whether they gated on `cpu_clock_active()` (48K / 128K / Plus / Timex) or unconditionally ticked the CPU (Pentagon / Scorpion).
3. Whether they had an AY-3-8912 to tick every 8 half-cycles.

Every cadence fix (tape-advance timing, AY phase, IRQ line feeding) had to land in all seven places. `SpectrumDriver` lifts the cadence into one canonical kernel and forces each machine to expose its chip set through short, auditable hooks.

## The hook surface

```rust
pub trait SpectrumDriver {
    fn hc(&self) -> u32;
    fn hc_mut(&mut self) -> &mut u32;
    fn frame_hc(&self) -> u32;
    fn contended(&self) -> bool { true }  // Pentagon/Scorpion override

    fn tick_ula(&mut self);
    fn cpu_clock_active(&self) -> bool { true }
    fn tick_cpu_and_bus(&mut self);
    fn feed_irq(&mut self);
    fn on_tstate(&mut self, hc: u32);     // 3.5 MHz: tape + AY + EAR audio
    fn tick_peripherals(&mut self) {}     // waiting for Phase 0.7 consumers
    fn end_frame_ula(&mut self);

    fn run_frame(&mut self) { /* provided */ }
}
```

The provided `run_frame` iterates half-cycles, calls `tick_ula` on each even beat, tick-the-CPU-if-allowed, feeds IRQ, and fires `on_tstate` on every T-state boundary. Pentagon and Scorpion override `contended() = false`; machines with an AY extend their `on_tstate` with an AY tick at `hc % 8 == 2`.

## `#[inline(always)]` is load-bearing

Without `#[inline(always)]` on every hook method, LLVM does not aggressively inline through the generic trait dispatch and the 48K `run_frame` regresses by ~8% versus the old hand-rolled version. With the hints, the bench came back to +1.7% (1.81 → 1.84 ms/frame on M2 Air, zeroed ROM, ~11× realtime).

This is a warning for any future trait-based refactor on the Spectrum hot path: the inliner needs help, and we measured it.

## Drift triggers

Two failure modes this decision was written to prevent: duplicating `run_frame` across machines again, and dropping the `#[inline(always)]` hints.

**Code patterns to reject:**

- Duplicating `run_frame` implementations in individual `machine-*` crates (the whole point of this trait is to stop that)
- Removing `#[inline(always)]` from any `SpectrumDriver` hook method
- `Box<dyn SpectrumDriver>` or `&mut dyn SpectrumDriver` on the hot path (defeats the inlining)
- Adding new chips to the Spectrum family without threading them through the `SpectrumDriver` hooks
- Landing a cadence fix (tape advance, AY phase, IRQ feeding) in one machine crate only — it should be in the shared `run_frame`

**Phrases that signal drift:**

- "Let me just duplicate the run loop for this variant, it's easier"
- "`#[inline(always)]` is ugly, let's remove it"
- "Dynamic dispatch through the trait is cleaner"
- "The 8% regression was probably a one-off, we don't need the inline hints now"
- "Each machine should have its own run loop for clarity"
- "This new chip doesn't fit the hook pattern, let's handle it outside the trait"

**Architectural framings to reject:**

- Treating `SpectrumDriver` as generic over all systems — it isn't, see [system-specific-run-loops.md](system-specific-run-loops.md)
- Refactoring the trait to reduce the hook count "for simplicity"
- Moving cadence logic out of the provided `run_frame` into per-machine impls

**What to do when triggered:** the `#[inline(always)]` hints are *load-bearing*. The 8% regression was measured on real hardware (M2 Air, zeroed ROM, 48K `run_frame`), not theoretical. If I think the hints can be removed, I need to benchmark and show the numbers before proposing the change. Same for any refactor of the hook surface — every hook was added because a specific Spectrum variant needed it, and removing one has implications across all 11 variants.

## Related

- [System-specific run loops](system-specific-run-loops.md) — the cross-system boundary that this decision sits inside.
- [ULA-drives model](ula-drives-model.md) — the timing invariant the shared loop preserves.
- [Half-cycle signals](half-cycle-signals.md) — the granularity the loop runs at.
