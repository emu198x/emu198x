# Emulator Suite System Registry

**Status:** Draft
**Date:** 2026-04-11

## Purpose

Turn the suite discussion into something actionable:

- one shared registry format for candidate targets
- one scheduler bucket per hardware family
- one place to record edge cases before code gets written

The registry is in [emulator-suite-system-registry.csv](./emulator-suite-system-registry.csv).

Related planning files:

- [Wave 1 plan](./2026-04-11-emulator-suite-wave-1.md)
- [Wave 1 target list](./emulator-suite-wave-1.csv)
- [Wave 1 milestones](./2026-04-11-emulator-suite-wave-1-milestones.md)
- [Wave 1 ticket list](./emulator-suite-wave-1-tickets.csv)
- [Code Like It's 198x platform requirements](./2026-04-11-code-like-198x-platform-requirements.md)
- [Code Like It's 198x architecture decisions](./2026-04-11-code-like-198x-architecture-decisions.md)
- [Code Like It's 198x commercial technique taxonomy](./2026-04-12-code-like-198x-commercial-technique-taxonomy.md)
- [Code Like It's 198x pattern library structure](./2026-04-12-code-like-198x-pattern-library-structure.md)
- [Code Like It's 198x pattern backlog](./2026-04-12-code-like-198x-pattern-backlog.md)
- [Code Like It's 198x historical exemplar registry](./2026-04-12-code-like-198x-historical-exemplar-registry.md)
- [Waves 2-6 roadmap](./2026-04-11-emulator-suite-roadmap-waves-2-6.md)
- [Waves 2-6 milestones](./2026-04-11-emulator-suite-roadmap-waves-2-6-milestones.md)
- [Waves 2-6 ticket list](./emulator-suite-roadmap-waves-2-6-tickets.csv)

## Scope

- Coverage target: systems released in the 1970s, 1980s, 1990s, and 2000s
- Granularity: one row per hardware family, not one row per SKU
- Shipping model: one binary per system family, with shared support crates and tooling where they pay off
- Regional variants, minor board revisions, and close rebrands belong in `notable_members`
- Arcade boards, calculators, and single-game LCD handhelds are intentionally out of scope for now

## Buckets

- `B` = beam-driven
  - The display beam is effectively part of the execution model.
  - Best for machines like the Atari 2600 and other race-the-beam designs.
- `T` = tick-driven
  - One dominant clock, simple ratios, few bus masters.
  - Best for handhelds and simpler consoles/computers.
- `S` = slot/contention
  - CPU, video, and memory contention need explicit visible phases.
  - Best for machines like the Spectrum, C64, NES, and many 8-bit micros.
- `H` = DMA/arbiter hybrid
  - Still deterministic and hardware-led, but DMA/display/arbitration matter as much as the CPU.
  - Best for Amiga, SNES, PC Engine, Mega Drive, and similar systems.
- `D` = dynarec+events
  - Recompiled CPU blocks or coarser stepping plus timestamped synchronization points.
  - Best for late-1990s and 2000s systems with multiple clock domains and 3D pipelines.

## Scope Flags

- `core` = strong candidate for a first-class suite target
- `optional` = good candidate, but lower priority or lower historical reach
- `edge` = unusual or disproportionately expensive for the value
- `exclude` = released in scope decades, but not recommended for this retro-focused suite

## Registry Columns

- `system_id`: stable identifier for the family
- `decade`: planning bucket, based on first release year of the family
- `release_year`: first family release year
- `manufacturer`: original platform vendor
- `family`: human-readable family label
- `notable_members`: seed list of important variants, add-ons, or close siblings
- `bucket`: execution-model bucket (`B`, `T`, `S`, `H`, `D`)
- `scope`: one of `core`, `optional`, `edge`, `exclude`
- `cpu_model`: dominant CPU or CPU family
- `video_model`: summary of the video architecture
- `audio_model`: summary of the audio architecture
- `media`: primary software distribution media
- `notes`: architecture caveats or reasons the bucket matters

## How To Use It

- Use the registry to choose the scheduler shape before choosing implementation details.
- Keep one machine timestamp across the suite, even when different buckets use different stepping policies.
- Reuse tooling across binaries and buckets: debugger, tracing, rewind, save states, ROM metadata, test harnesses.
- Do not force every machine through the same execution or support strategy just because the registry is shared.

## Planning Read

- `B/T/S` cover most 1970s and 1980s targets cleanly.
- `H` is where many 16-bit and advanced late-1980s computers/consoles land.
- `D` becomes common in the late 1990s and dominates the 2000s.
- `exclude` in the 2000s mostly means "released then, but belongs to the next architectural era."
