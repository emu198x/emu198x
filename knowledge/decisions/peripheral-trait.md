# Decision: `Peripheral` trait — static dispatch, no peripheral bus struct

**Date:** April 2026 (Phase 0.7)

## The decision

Spectrum edge-connector devices (Beta disk, µPD765A, and every Phase 2 add-on — Multiface, DivMMC, Kempston Mouse, ZX Printer, Currah µSpeech, Specdrum, Fuller Audio Box, Interface 1, Interface 2, …) implement the `Peripheral` trait in `common-sinclair-zx-spectrum::peripheral`. Each machine keeps its peripherals as **typed fields** and dispatches to them explicitly.

No `PeripheralBus` struct. No `Vec<Box<dyn Peripheral>>`. No runtime registration.

## The trait

```rust
pub trait Peripheral {
    fn claims_port(&self, port: u16) -> bool { false }
    fn read(&mut self, port: u16) -> u8 { 0xFF }
    fn write(&mut self, port: u16, val: u8) {}
    fn on_m1(&mut self, addr: u16) {}
    fn tick(&mut self, hc: u32) {}
}
```

All methods default to no-op / idle-bus so implementors override only what they need. A joystick overrides `claims_port` + `read`. A disk controller adds `on_m1` for ROM paging traps. A mouse adds `tick` for delta decay.

## Why not dynamic dispatch

The project's plan document originally specified a `PeripheralBus` struct holding `Vec<Box<dyn Peripheral>>`. Static dispatch won instead, for three reasons:

1. **The hot path is performance-sensitive.** Phase 0.6's `SpectrumDriver` refactor needed `#[inline(always)]` on every hook method to avoid an 8% regression — see [SpectrumDriver decision](spectrum-driver.md). A `Vec<Box<dyn Peripheral>>` on the I/O path adds indirection LLVM cannot inline through.
2. **Every peripheral is known at compile time.** Pentagon always has Beta disk. +3 always has Upd765a. +2A / +2B share the SpectrumPlus struct but carry a disabled FDC (via `enabled: bool`). There is no "user plugs Multiface in at runtime" story — that would be a configuration change at machine construction, not a runtime bus rewrite.
3. **The trait is a *vocabulary*, not a container.** Its value is: documented shape, proven against real peripherals (Beta disk, Upd765a), ready for Phase 2 consumers to plug in without API churn. The shape is what matters; the storage is per-machine.

If Phase 2 or a later phase genuinely needs dynamic registration, wrapping the existing trait in a `Vec<Box<dyn Peripheral>>` later is a mechanical change. Introducing it now would be speculative.

## What the trait does *not* cover

- **Memory-bus intercepts.** Beta disk's TR-DOS ROM read-override, Interface 1's shadow ROM, Multiface's banked RAM / ROM. These are memory-bus hooks, not I/O hooks, and the trait deliberately doesn't model them. Pentagon and Scorpion keep their existing `beta.trdos_paged` / `memory.read_trdos_rom` checks machine-side. A second peripheral that wants the same hook will justify adding `read_memory` to the trait.
- **Core machine chips.** The ULA (port `$FE`), the AY-3-8912 (`$FFFD` / `$BFFD` on stock 128K, `$F5` / `$F6` on Timex), and the `$7FFD` / `$1FFD` memory-paging ports are not peripherals — they're integral to each machine and stay hand-decoded inside `io_read` / `io_write`.
- **The Kempston joystick.** A `u8` field with a one-line port read. Making it a `Peripheral` would be over-engineering the simplest possible case. It stays as-is.

## Disabled peripherals

The Upd765a carries a `pub enabled: bool` field. `SpectrumPlus::new(model)` sets `fdc.enabled = (model == Model::Plus3)` at construction. On +2A / +2B the FDC sits inert on the bus — `claims_port` returns false unconditionally, so the port-mask decoding costs a single boolean test per I/O cycle and no machine-side model check is needed. Future peripherals that are physically present but electrically disconnected (e.g. a configurable-off Kempston Mouse) should follow the same pattern.

## Drift triggers

This decision explicitly rejected the originally-planned dynamic dispatch approach. The static-dispatch choice was measured (8% regression in Phase 0.6) and deliberate. If I'm about to propose any of these, stop.

**Code patterns to reject:**

- `Vec<Box<dyn Peripheral>>` anywhere in a machine struct
- `PeripheralBus { peripherals: Vec<...> }` as a struct
- `peripheral_bus.register(Box::new(...))` or any runtime registration API
- `fn add_peripheral(&mut self, p: Box<dyn Peripheral>)` methods
- `impl Peripheral for Ula` / `impl Peripheral for Ay38912` — core chips are NOT peripherals, they stay hand-decoded inside `io_read` / `io_write`
- `impl Peripheral for Kempston` — the Kempston joystick is explicitly excluded as too simple
- Removing `#[inline(always)]` from trait methods "for readability"

**Phrases that signal drift:**

- "Let's make peripherals a `Vec` of trait objects, it's more flexible"
- "Runtime peripheral registration would let users configure their hardware"
- "The ULA should also implement `Peripheral` for consistency"
- "`#[inline(always)]` is ugly, LLVM will inline anyway"
- "Dynamic dispatch would simplify the machine structs"
- "We can flatten the memory-bus intercepts into the `Peripheral` trait"
- "Making every peripheral a trait object would enable hot-swap"

**Architectural framings to reject:**

- Treating `Peripheral` as a *container* rather than a *vocabulary*
- Justifying dynamic dispatch with "future flexibility" or "Phase 2 will need it"
- Unifying core chips and peripherals under the same abstraction
- Moving memory-bus intercepts (Beta disk TR-DOS, Multiface banked RAM, Interface 1 shadow ROM) into the I/O trait

**What to do when triggered:** the trait's value is the *shape*, not the storage. If dynamic dispatch later becomes a genuine need, wrapping the existing trait in a `Vec<Box<dyn Peripheral>>` is a mechanical change. Introducing it speculatively is the drift. Also: the 8% regression measurement from Phase 0.6 is real and load-bearing — see [spectrum-driver.md](spectrum-driver.md). Do not remove the inline hints.

## Related

- [SpectrumDriver decision](spectrum-driver.md) — where `tick_peripherals` is hooked into the run loop.
- [No Bus trait](no-bus-trait.md) — the CPU↔machine boundary uses signals, not a trait; this peripheral decision handles the *machine↔device* boundary with a different choice.
