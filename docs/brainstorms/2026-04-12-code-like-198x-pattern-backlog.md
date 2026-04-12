# Code Like It's 198x Pattern Backlog

**Status:** Draft
**Date:** 2026-04-12

## Purpose

Turn the commercial technique taxonomy into an implementable pattern-library backlog.

This backlog is not a lesson plan and not a machine-feature checklist. It is the first working queue for reusable technical patterns, sample code, artifacts, and optional blessed-path observability.

The machine-readable backlog is in [code-like-198x-pattern-backlog.csv](./code-like-198x-pattern-backlog.csv).

Related planning files:

- [Code Like It's 198x commercial technique taxonomy](./2026-04-12-code-like-198x-commercial-technique-taxonomy.md)
- [Code Like It's 198x pattern library structure](./2026-04-12-code-like-198x-pattern-library-structure.md)
- [Code Like It's 198x platform requirements](./2026-04-11-code-like-198x-platform-requirements.md)
- [Code Like It's 198x architecture decisions](./2026-04-11-code-like-198x-architecture-decisions.md)
- [Code Like It's 198x historical exemplar registry](./2026-04-12-code-like-198x-historical-exemplar-registry.md)

## Backlog Principles

- Patterns are reusable technical assets, not one-off lessons.
- The first backlog should favor patterns that unlock several lessons and several game projects.
- Every backlog item should name both a portable path and, where useful, a blessed-path observability story.
- Later systems should deepen existing patterns before they create entirely new silos.
- Historical exemplar work is adjacent to the backlog, but should not block pattern implementation.

## Priority Bands

### `P0`: Cross-Machine Foundation

These are the patterns that should exist before the first public teaching-facing wave feels coherent.

- input abstraction
- main loop and frame structure
- movement and collision
- state machines
- startup, loading, and persistence basics
- debugging and observability basics

### `P1`: Initial Family Anchor Patterns

These make the initial four families feel like themselves instead of like generic teaching targets.

- `Spectrum` software redraw and attribute-aware rendering
- `NES` NMI-driven frame work and metasprites
- `C64` hardware sprites, raster timing, and SID identity
- `Amiga` bitplanes, Copper, Blitter, and Paula-era production patterns

### `P2`: Commercial Polish And Content Scale

These move projects from toy examples toward credible shipping-era work.

- scrolling and camera systems
- audio-driver structure
- content paging and data-driven game structure
- title and options flow
- attract mode and transition polish

### `P3`: Deepening Systems

These are mostly later-family extensions to earlier patterns.

- `ZX80` / `ZX81`
- `PET` / `VIC-20`
- `C16` / `Plus/4`
- `Atari 2600`
- `Apple II`
- `MSX`
- `Game Boy`
- `SG-1000` / `Master System` / `Game Gear`
- `PC Engine`
- `Mega Drive`
- `SNES`
- `Spectrum` clones and `Spectrum Next`
- later `Amiga` and `Atari ST`

## Initial Backlog Shape

The first backlog should be split into four groups.

### Group A: Cross-Machine Foundations

- input abstraction and machine adapters
- frame or tick pipeline
- player movement and collision
- state machines and game flow
- startup and loading flow
- instrumentation and regression hooks

### Group B: Rendering Foundations

- software sprite redraw
- hardware sprite composition
- scrolling camera
- split-screen or raster-time presentation
- bitmap, bitplane, and mixed presentation models

### Group C: Audio, Data, And Persistence

- SFX and music-driver basics
- audio channel budgeting
- save, password, and persistent score handling
- bank switching or staged content loading
- asset packaging and conversion

### Group D: Commercial Polish

- title and options flow
- attract mode
- cutscene or transition structure
- loader presentation
- debugging and validation instrumentation

## Pattern Entry Expectations

Each backlog item should eventually produce:

- a pattern overview
- at least one working implementation
- portable build artifacts
- optional blessed-path observability hints
- references to historical exemplar candidates
- validation notes

## Recommended First Pass

If the initial product scope is `Spectrum`, `NES`, `C64`, and `Amiga`, the first pass should aim to complete:

1. all `P0` patterns
2. one or two `P1` anchor patterns per family
3. selected `P2` patterns that make the sample games feel commercially credible

Do not wait for all `P3` deepening systems before shipping the first useful library.

## Machine-Deepening Map

These systems should primarily extend existing patterns rather than create separate first-wave curricula.

| Deepening System | Extend These Pattern Areas |
|---|---|
| `ZX80` / `ZX81` | text-first flow, minimal-memory loop design, keyboard-first interaction |
| `PET` / `VIC-20` | character-set visuals, tiny-memory packaging, early loader UX |
| `C16` / `Plus/4` | low-cost color and audio patterns, budget commercial polish |
| `Atari 2600` | beam-driven rendering, frame-kernel debugging, scanline budgeting |
| `Apple II` | artifact color, mixed text and graphics, disk workflow |
| `MSX` | VDP portability, slot-aware media and memory patterns |
| `Game Boy` | low-resolution readability, battery save, handheld UX |
| `SG-1000` / `Master System` / `Game Gear` | console tile and sprite portability outside NES assumptions |
| `PC Engine` | scrolling and presentation layering, richer console action pacing |
| `Mega Drive` | larger scrolling worlds, FM plus PSG sound identity, faster action conventions |
| `SNES` | DMA and HDMA presentation, layered polish, sample-audio pipelines |
| `Spectrum` clones and `Spectrum Next` | compatibility testing, capability negotiation, enhancement-vs-baseline teaching |
| `Atari ST` and later `Amiga` | disk-era scale, larger UI, richer asset and audio pipelines |

## Backlog Themes For The Initial Four Families

### Spectrum

- `PAT-RENDER-SWSPR-001`
- `PAT-RENDER-ATTR-001`
- `PAT-AUDIO-BEEPER-001`
- `PAT-DATA-BANK-001`
- `PAT-STORAGE-TAPE-001`

### NES

- `PAT-RUNLOOP-NMI-001`
- `PAT-RENDER-METASPR-001`
- `PAT-RENDER-SCROLL-001`
- `PAT-AUDIO-APU-001`
- `PAT-DATA-MAPPER-001`

### C64

- `PAT-RENDER-HWSPR-001`
- `PAT-RENDER-RASTER-001`
- `PAT-AUDIO-SID-001`
- `PAT-STORAGE-DISK-001`
- `PAT-POLISH-TITLE-001`

### Amiga

- `PAT-RENDER-BITPLANE-001`
- `PAT-RENDER-COPPER-001`
- `PAT-RENDER-BLITTER-001`
- `PAT-AUDIO-SAMPLE-001`
- `PAT-STORAGE-FLOPPY-001`

## Notes

- Pattern IDs should stay stable even if lesson structure changes.
- Sample code can evolve independently of the lesson prose as long as the pattern contract remains stable.
- Historical exemplar work should begin immediately, but low-confidence entries should remain clearly provisional until they are researched properly.

## Next Planning Step

Promote this backlog into:

- concrete sample-code directories and naming rules
- a smaller `first 12 patterns` implementation wave
- family-specific validation expectations for each implemented pattern
