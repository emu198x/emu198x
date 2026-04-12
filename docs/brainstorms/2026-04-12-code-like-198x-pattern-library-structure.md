# Code Like It's 198x Pattern Library Structure

**Status:** Draft
**Date:** 2026-04-12

## Purpose

Define how the `Code Like It's 198x` pattern library should sit between:

- free-form lesson content on the website
- working code and artifacts outside the lesson prose
- the blessed emulator path with richer inspection and capture
- the long-term multi-family preservation platform

The pattern library is where reusable technique should live.
It is not the same thing as the lesson content, and it should not force the curriculum into a rigid tutorial-engine shape.

Related planning files:

- [Code Like It's 198x platform requirements](./2026-04-11-code-like-198x-platform-requirements.md)
- [Code Like It's 198x architecture decisions](./2026-04-11-code-like-198x-architecture-decisions.md)
- [Code Like It's 198x commercial technique taxonomy](./2026-04-12-code-like-198x-commercial-technique-taxonomy.md)
- [Code Like It's 198x pattern backlog](./2026-04-12-code-like-198x-pattern-backlog.md)
- [Code Like It's 198x historical exemplar registry](./2026-04-12-code-like-198x-historical-exemplar-registry.md)

## Core Principles

- Lessons remain prose-first, self-directed, and emulator-agnostic.
- Students should be able to use other emulators or real hardware where practical.
- The pattern library carries reusable technique, working code, buildable artifacts, historical context, and machine variants.
- The blessed emulator path adds inspection, stepping, capture, overlays, and automation, but must not become the only way to follow the material.
- Working code should live outside lesson prose and be referenced, not duplicated casually inside lessons.

## Portable Path And Blessed Path

Every pattern entry should support two paths.

### Portable Path

This is the real curriculum contract.

- source code
- build instructions
- standard artifacts where relevant: tape, disk, cartridge, executable, snapshot, symbol file
- expected outcomes
- machine or profile assumptions

The portable path must be usable with third-party emulators where the artifact format and machine assumptions allow it.

### Blessed Path

This is where `Code Like It's 198x` adds value.

- browser or native embeds
- frame, scanline, or instruction stepping where supported
- memory, register, bus, and device inspection
- sprite, tile, audio-channel, or bitplane extraction where meaningful
- controlled code injection or scripted setup
- capture, trace, and replay tooling

The blessed path should be additive. A pattern must not require exclusive platform features in order to be understood.

## Relationship To Lessons

Lessons should do the narrative work:

- why the technique matters
- when to use it
- what tradeoffs it solves
- what exercise or game context motivates it

Patterns should do the reusable technical work:

- the technique explanation
- machine-specific implementations
- working code
- buildable artifacts
- observability hooks
- related historical exemplars

The same lesson can point to several patterns, and the same pattern can appear in many lessons.

## Pattern Entry Model

Each pattern entry should eventually record:

- `PatternId`
- `TechniqueId`
- `Name`
- `Summary`
- `CommercialPurpose`
- `Difficulty`
- `FamiliesSupported`
- `ProfilesCovered`
- `PortableAssets`
- `BuildInstructions`
- `ObservabilityHooks`
- `BlessedPathFeatures`
- `RelatedPatterns`
- `HistoricalExemplars`
- `ValidationNotes`

## What A Pattern Entry Should Contain

### 1. Concept Overview

- concise explanation of the technique
- why commercial developers used it
- common failure modes

### 2. Machine Variants

- one shared concept page
- family-specific implementations
- notes on where the technique changes shape sharply between families

### 3. Working Code

- code referenced from the sample library, not embedded as the only source of truth in lesson prose
- one or more runnable implementations per supported family
- sample sizes appropriate to `intro`, `shipping`, `advanced`, or `flagship` depth

### 4. Buildable Outputs

- the standard artifacts a student can run
- optional debug or symbol outputs for the blessed path
- any machine-profile assumptions

### 5. Observability Hooks

Pattern entries should declare what the blessed path can expose:

- registers and memory watches
- screen or beam checkpoints
- sprite, tile, attribute, raster, or Copper views
- audio-channel or waveform views
- device-traffic or bus traces

### 6. Commercial Context

- what kind of shipped games used the technique
- what problem it solved in production
- when it was optional versus expected

### 7. Historical Context

- exemplar titles
- confidence notes
- follow-up systems that deepen the idea

## Pattern Types

The first library should cover these broad pattern families.

### Foundation Patterns

- fixed or machine-timed update structure
- input polling
- movement and collision
- state machines
- score and progression systems

### Rendering Patterns

- text and character-cell presentation
- software sprite redraw
- hardware sprites and metasprites
- scrolling and camera systems
- split-screen and raster-time UI
- palette and color-restriction handling
- bitplane and blitter-oriented composition

### Audio Patterns

- SFX triggering and prioritization
- simple music driver structure
- channel arbitration
- tracker or sample workflows where applicable

### Data And Content Patterns

- level and room formats
- bank switching and content paging
- compression
- save or password systems
- startup and loader flows

### Production And Polish Patterns

- title and options flow
- attract mode
- loading presentation
- cutscenes and transitions
- debugging and validation instrumentation

## Initial Pattern Backlog Anchors

The initial product scope should start with patterns that land on all four first families where possible.

| Pattern Group | Initial Anchor Families | Why It Belongs Early |
|---|---|---|
| input abstraction | `Spectrum`, `NES`, `C64`, `Amiga` | every family forces different input assumptions |
| main loop and frame structure | `NES`, `C64`, `Spectrum`, `Amiga` | exposes machine timing and lesson instrumentation needs immediately |
| player movement and collision | `Spectrum`, `NES`, `C64` | core game feel pattern with portable exercises |
| software sprite redraw | `Spectrum` first, then `Amiga` and early micros where relevant | teaches redraw strategy before hardware sprites hide the cost |
| hardware sprites and composition | `C64`, `NES` | canonical console and computer-side contrast |
| scrolling camera | `NES` first, then `Amiga`, `SMS`, `Mega Drive`, `SNES` later | key commercial-quality differentiator |
| split-screen and raster-time presentation | `C64`, `NES`, later `Atari 2600`, `SNES`, `Amiga` | teaches timing as a presentation tool |
| audio identity | `C64`, `NES`, `Amiga`, `Spectrum` | forces family-specific audio approaches without losing cross-machine intent |
| startup, loading, and persistence | `Spectrum`, `C64`, `NES`, `Amiga` | essential commercial reality rather than optional polish |
| tooling and asset conversion | all initial families | needed to bridge from tutorial samples to shippable content |

## How Other Target Systems Deepen Existing Patterns

The broader catalog should mostly deepen existing patterns instead of creating isolated new silos.

| System Or Group | Patterns It Deepens |
|---|---|
| `ZX80` / `ZX81` | text-first UX, minimal-memory loop design, visible display-cost patterns |
| `PET` / `VIC-20` | character-set animation, keyboard-first software flow, tiny-memory packaging |
| `C16` / `Plus/4` | low-cost commercial polish, `TED` color and audio patterns |
| `Atari 2600` | beam-driven rendering, scanline-budgeted gameplay, frame-kernel debugging |
| `Apple II` | artifact-color rendering, mixed text and graphics presentation |
| `MSX` | VDP portability, slot-aware media and memory patterns |
| `SG-1000` / `Master System` / `Game Gear` | tile-and-sprite console portability outside NES assumptions |
| `Game Boy` / `Game Boy Color` | handheld readability, battery-save expectations, low-resolution UX |
| `PC Engine` | richer console scrolling, presentation layering, CD-transition concerns |
| `Mega Drive` / `Genesis` | larger action-game scrolling, FM plus PSG sound identity, production-scale asset work |
| `SNES` | DMA and HDMA display patterns, multi-layer polish, sample-audio pipeline patterns |
| `Spectrum clones` and `Spectrum Next` | compatibility testing, clone-aware timing, enhancement-vs-baseline teaching |
| `Atari ST` and later `Amiga` models | larger-machine UI, disk-based content scale, advanced production workflows |

## Pattern Structure In Practice

Each pattern should eventually have:

- one overview page
- one or more machine-variant pages
- one or more code-sample directories
- build instructions
- artifact references
- blessed-path observation recipes
- validation notes
- related lesson links

That structure can exist without forcing lessons into a rigid schema.

## Authoring Rules

- Do not duplicate large canonical code bodies inside lesson prose.
- Do link to the relevant pattern entry and code sample from the lesson.
- Do keep tiny inline snippets when they are pedagogically useful.
- Do keep machine-specific build or artifact truth near the code sample or pattern entry, not buried in narrative text.
- Do treat observability recipes as optional enhancements, not required lesson steps.

## Implications For The Emulator Platform

- The blessed path should understand pattern-level concepts like recommended checkpoints, suggested watches, and relevant visual or audio inspection views.
- Those hints should remain optional metadata, not a requirement for following the pattern on another emulator.
- Pattern entries are a better place than lessons to attach reusable inspection presets for `Spectrum` attributes, `NES` nametables, `C64` raster state, or `Amiga` Copper state.
- Future sprite, tile, music, or map editors should attach naturally to pattern entries because the library already names the relevant assets and observation points.

## Next Planning Step

Turn this structure into:

- an initial pattern backlog for the first four families
- a naming convention for cross-machine pattern IDs
- a lightweight way to attach pattern metadata to sample code and artifacts without constraining lesson prose
