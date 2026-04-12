# Decision: System-Specific Run Loops

**Date:** April 2026

## The decision

No universal run loop pattern. Each system gets the run loop that matches how its actual hardware operates. The shared infrastructure provides building blocks (audio output, frame capture, input handling) but never dictates the tick loop.

## Why

Different hardware works differently:

- **Spectrum**: ULA owns the clock, gates the CPU via `cpu_clock_active()`. Contention = CPU slot skipped.
- **C64**: VIC-II asserts BA line during badlines and sprite fetches, halting the CPU. Different mechanism from Spectrum.
- **NES**: PPU drives timing. CPU and PPU advance on different clock ratios (PPU = 3x CPU).
- **Amiga**: Agnus arbitrates the bus. DMA slots are fixed in the scanline. CPU gets whatever slots are left.
- **Atari 2600**: CPU races the beam. No video chip gating — the programmer must count cycles to stay in sync. Fundamentally different model.
- **ZX80**: NMI-driven display. CPU runs normally, NMI fires to trigger display output. Neither clock-gating nor bus arbitration.

Forcing these into one pattern would be inaccurate. The project's core principle is that accuracy comes from matching the hardware, not from fitting hardware into our abstractions.

## The system trait boundary

`run_frame()` is the shared boundary. The shell calls it and receives:
- A framebuffer (palette-indexed `u8` or RGBA, system's choice)
- An audio buffer
- Frame metadata (timing, events)

What happens inside `run_frame()` is the system's business. The shared infrastructure doesn't know or care whether the frame was produced by clock-gating, bus arbitration, beam-racing, or any other mechanism.

## Drift triggers

The temptation here is universality. Every time I propose "one pattern for all systems," I'm proposing to repeat the failure that drove the fresh start.

**Code patterns to reject:**

- `trait UniversalRunLoop` or any generic trait intended to cover all systems
- Shared generic `run_frame` implementations across system families
- `impl RunLoop for Spectrum` + `impl RunLoop for C64` + `impl RunLoop for NES` with shared bodies
- Extending [spectrum-driver.md](spectrum-driver.md)'s `SpectrumDriver` trait to non-Spectrum systems
- Abstracting clock-gating, bus arbitration, beam-racing, and NMI-driven display under one interface
- A `Machine` trait with `tick_frame()` as a universal hook

**Phrases that signal drift:**

- "Let's have one run loop that handles all systems"
- "A universal emulation loop would be cleaner"
- "We can abstract the differences between clock-gating and bus arbitration"
- "The Spectrum pattern should work for everything"
- "Isn't there a way to share this logic across C64 and Spectrum?"
- "These all look similar, let's factor them out"
- "`SpectrumDriver` but generic over the timing chip"

**Architectural framings to reject:**

- Treating all systems as variations of one model
- Forcing beam-racing (Atari 2600) or NMI-driven display (ZX80) into a ULA-style clock-gating abstraction
- Any "shared core" that takes ownership of the tick loop rather than providing building blocks
- `SpectrumDriver` generalized for any reason

**What to do when triggered:** the shared boundary is `run_frame()` on the `System` trait, which returns a framebuffer + audio + metadata. Everything inside that function is the system's business. If I'm proposing to share code *below* that boundary, I'm proposing to re-create the "fit hardware into our abstractions" problem that started this rewrite. Accuracy comes from matching the hardware, not from abstraction.

## Related

- [ULA-drives model](ula-drives-model.md) — the Spectrum-specific pattern (not universal)
