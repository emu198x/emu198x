# Code Like It's 198x Platform Requirements

**Status:** Draft
**Date:** 2026-04-11

## Purpose

Define the platform requirements for the preservation, instrumentation, and education layer that sits around the emulator families.

This is not the same thing as the suite wave order.

- The long-term suite goal is every machine, every variant, every region.
- The initial `Code Like It's 198x` product scope is narrower: `spectrum_family`, `famicom_nes_family`, `commodore_64_128_family`, and `amiga_ocs_ecs_family`.
- These requirements should still be written so they survive contact with the eventual full catalog.

Related planning files:

- [Emulator suite system registry](./2026-04-11-emulator-suite-system-registry.md)
- [Emulator suite wave 1 plan](./2026-04-11-emulator-suite-wave-1.md)
- [Emulator suite wave 1 milestones](./2026-04-11-emulator-suite-wave-1-milestones.md)
- [Code Like It's 198x architecture decisions](./2026-04-11-code-like-198x-architecture-decisions.md)
- [Code Like It's 198x commercial technique taxonomy](./2026-04-12-code-like-198x-commercial-technique-taxonomy.md)
- [Code Like It's 198x pattern library structure](./2026-04-12-code-like-198x-pattern-library-structure.md)
- [Code Like It's 198x pattern backlog](./2026-04-12-code-like-198x-pattern-backlog.md)
- [Code Like It's 198x historical exemplar registry](./2026-04-12-code-like-198x-historical-exemplar-registry.md)

## Initial Product Scope

The first public teaching-facing scope should center on four families:

- `spectrum_family`
  - baseline profiles: `48K`, `128K`
  - follow-up profiles once the family shell is stable: `+2`, `+3`
- `famicom_nes_family`
  - baseline profile: `NES/Famicom` with a small mapper set
- `commodore_64_128_family`
  - baseline profile: `C64 PAL breadbin`
- `amiga_ocs_ecs_family`
  - baseline profile: `Amiga 500 OCS PAL`

The initial product scope does not change the long-term preservation goal. It only defines which families must shape the first platform surfaces.

## Release Gates

Before the first teaching-facing release, the platform should be able to:

- run the initial four families through one shared control and inspection model
- load their baseline media and firmware requirements
- capture screenshots, video, and audio
- step execution and inspect visible machine state
- inject beginner-friendly code or inputs into each family through a supported path
- run headless for automation and in-browser for embedded lessons

## Requirements Checklist

### 1. Catalog And Scope

- [ ] Support tiers exist for every profile: `research`, `boots`, `usable`, `teaching`, `reference`.
- [ ] The platform can expose incomplete or uncertain machines without pretending they are finished.
- [ ] Family, profile, region, firmware, and capability metadata live in a first-class machine database.
- [ ] Product scope and preservation scope stay separate, so the first teaching release is not blocked by eventual catalog breadth.

### 2. Deterministic Core Control

- [ ] Every family core runs headless and deterministic under a shared outer contract.
- [ ] The control surface supports `run`, `pause`, `reset`, `run_until`, and `snapshot` / `restore`.
- [ ] Stepping supports the finest level each family can reasonably expose: instruction, cycle, scanline, frame, or event.
- [ ] Emulation speed supports at least `10%`, `25%`, `50%`, `100%`, `200%`, `400%`, and `unlocked`.
- [ ] The control surface can report when a given stepping mode is unsupported for a specific family or profile.

### 3. Authenticity And Teaching Conveniences

- [ ] The platform distinguishes clearly between authentic machine behavior and host-side teaching conveniences.
- [ ] Teaching conveniences such as paste, rewind, overlays, symbolic labels, and guided stepping can be enabled or disabled per lesson or session.
- [ ] Convenience features do not silently alter the emulated machine state without recording that fact in session or lesson metadata.
- [ ] Users can tell whether a screenshot, video, trace, or replay came from raw emulation or from a teaching-enhanced presentation mode.

### 4. Inspection And Observability

- [ ] The inspection surface can read CPU registers, flags, memory, and machine identity.
- [ ] The inspection surface can read device-local state where it is meaningful: PPU/VDP/VIC/APU/CIA/custom-chip state.
- [ ] The platform can observe bus, disk, tape, cartridge, and peripheral activity as timestamped events.
- [ ] The platform can expose machine-local trace payloads without forcing a fake universal CPU schema.
- [ ] Breakpoints and watchpoints can target at least execution, memory access, and selected device events.

### 5. Media And Persistence

- [ ] The platform can load all supported media classes: cartridge, tape, floppy, snapshot, firmware, and optical media where relevant.
- [ ] Mutable media state has a clear writeback policy.
- [ ] Save-state envelopes are versioned and machine-aware.
- [ ] Media manifests record hashes, provenance, and required firmware.
- [ ] The platform can report unsupported or uncertain media formats explicitly rather than failing opaquely.

### 6. Input And Control Mapping

- [ ] Keyboard input can map to machine-native keys or matrices.
- [ ] Host keys can map to joystick directions and buttons.
- [ ] Gamepads can map to machine joysticks, pads, and menu controls.
- [ ] Input mappings can vary by family, profile, and region.
- [ ] Input recording and deterministic playback exist for validation and teaching.

### 7. Capture And Presentation

- [ ] Raw screenshots can be captured from every machine.
- [ ] Video capture can export either raw output or presentation output.
- [ ] Audio capture can export mixed output and per-channel output where the family exposes meaningful channels.
- [ ] Audio channels can be muted, soloed, or exported independently when supported by the family.
- [ ] CRT, LCD, and similar presentation filters live outside the emulation core.
- [ ] The platform can expose whether a capture came from raw output or filtered presentation output.

### 8. Graphics And Audio Extraction

- [ ] Families with tiles, sprites, pattern tables, or bitplanes can expose those assets directly for inspection.
- [ ] The platform can surface when a machine does not have a meaningful sprite or tile model.
- [ ] Audio-capable families can expose named channels or voices where the hardware model supports it.
- [ ] Asset extraction APIs are stable enough to support future editors without promising the editors in v1.

### 9. Teaching And Code Injection

- [ ] The platform can inject code or scripted input through family-appropriate paths.
- [ ] `Spectrum` and `C64` support BASIC-oriented teaching flows.
- [ ] `NES` supports assembly-oriented teaching flows and ROM or RAM-backed code injection appropriate to the profile.
- [ ] `Amiga` supports family-appropriate teaching flows without pretending it is a one-screen 8-bit BASIC machine.
- [ ] The teaching surface can pair code, media, breakpoints, expected outputs, annotations, and optional inspection presets where useful.
- [ ] Rewind, bookmarks, or replay checkpoints exist for classroom use.
- [ ] Where practical, the platform can surface source-aware teaching views such as BASIC line numbers, labels, symbols, or disassembly overlays on top of raw machine state.
- [ ] Source-aware views remain explicitly advisory and do not replace raw machine inspection.

### 10. Automation And MCP

- [ ] The same core control model drives CLI tools, scripts, desktop tooling, WASM embeds, and MCP.
- [ ] MCP can step machines, inspect state, load media, inject inputs, and subscribe to traces.
- [ ] Scripted automation uses the same capability checks as MCP and UI tooling.
- [ ] Automation can run multiple machines in parallel without sharing mutable core state accidentally.

### 11. Peripherals And Device Modeling

- [ ] The platform can describe peripherals through stable descriptors rather than family-specific ad-hoc flags.
- [ ] Reasonable virtual peripherals are in scope: printers, storage devices, input devices, and selected network or link devices.
- [ ] Peripheral traffic can be observed and traced.
- [ ] Unsupported peripherals can be declared and surfaced cleanly.

### 12. Multi-Machine And Networking

- [ ] The platform can run multiple emulator instances on the same host under one orchestrated session.
- [ ] The platform can model local link or network behavior where the families support it.
- [ ] Remote-host orchestration is possible without changing the family core API.
- [ ] Timing policy for linked machines is explicit: local lockstep first, remote orchestration later.

### 13. Web And WASM

- [ ] The initial four families are designed so their cores can run under WASM.
- [ ] Browser embeds can use the same control and inspection model as native tooling.
- [ ] Browser builds do not require direct filesystem or socket access inside the cores.
- [ ] The platform can degrade gracefully when a feature is not practical in-browser.

### 14. Accessibility And Localization

- [ ] The platform supports keyboard-only use for core teaching and inspection flows.
- [ ] Teaching overlays and debug views can scale cleanly and remain readable on different display sizes.
- [ ] Color-dependent overlays have color-blind-safe alternatives where practical.
- [ ] Audio-oriented teaching features can expose captions, transcripts, or machine-readable event summaries where reasonable.
- [ ] Host keyboard layouts, regional machine layouts, and character-set differences are modeled explicitly rather than treated as one default locale.

### 15. Data, Versioning, And Provenance

- [ ] Snapshots, traces, teaching artifacts, media manifests, and capability schemas are versioned.
- [ ] Hardware quirks, unsupported behavior, and guessed behavior can carry provenance notes.
- [ ] Validation artifacts keep enough metadata to reproduce a run later.
- [ ] The platform distinguishes between measured hardware behavior, documented behavior, and inferred behavior.
- [ ] Research uncertainty is visible in the product rather than buried in internal notes.

### 16. Verification And Reproducibility

- [ ] The platform maintains golden screenshots, audio artifacts, traces, or other family-appropriate regression baselines.
- [ ] Support tiers map to validation expectations, so `teaching` and `reference` mean something concrete.
- [ ] Captures, lessons, traces, and demos record enough metadata to explain how they were produced.
- [ ] Reproducible classroom or demo runs are a first-class use case rather than an incidental by-product.

### 17. Curriculum Independence, Pattern Portability, And Collaboration

- [ ] The curriculum remains usable with third-party emulators or real hardware where the lesson artifacts allow it.
- [ ] Reusable technique lives in a pattern library that sits outside the free-form lesson prose.
- [ ] Pattern entries and their associated artifacts can run across native, headless, and WASM frontends where the required capabilities exist.
- [ ] The platform can support artifact-based collaboration such as shared patterns, traces, captures, checkpoints, and lesson-linked runnable examples.
- [ ] Live collaborative or teacher-driven sessions can be layered on top of the same artifact and orchestration model later.

### 18. Legal, Archival, And Provenance Policy

- [ ] The platform has an explicit policy for ROMs, firmware, media images, manuals, fonts, and other copyrighted artifacts.
- [ ] Redistributable assets and user-supplied assets are distinguished clearly.
- [ ] Archival metadata records where media, firmware, and reference behavior claims came from.
- [ ] Preservation-oriented artifacts such as captures, traces, annotations, and student-created programs can be retained with provenance metadata.

### 19. Export Formats And Artifact Retention

- [ ] Screenshots, audio, video, traces, pattern artifacts, and extracted assets export through documented formats.
- [ ] Open or well-documented formats are preferred where practical.
- [ ] Exported artifacts retain enough metadata to remain useful outside the platform.
- [ ] User-created artifacts should be preservable even when the original frontend or teaching surface changes.

### 20. Extension And Plugin Policy

- [ ] Third-party peripherals, tools, and integrations attach through explicit versioned boundaries.
- [ ] The platform can describe which extension points are stable, experimental, or unsupported.
- [ ] Extensions do not get implicit access to core internals or unrestricted host resources.
- [ ] The first extension boundary should be designed intentionally, not inferred from internal module layout.

### 21. Security And Operational Safety

- [ ] Browser, MCP, and scripting surfaces respect explicit capability gates.
- [ ] Cores do not get arbitrary filesystem or network access.
- [ ] Untrusted media, lesson content, and teaching artifacts have a defined trust boundary.
- [ ] Remote orchestration and device forwarding do not assume a trusted local machine by default.

## Immediate Non-Goals

The first teaching-facing scope should not require:

- every model, clone, and region of the initial four families
- full editor suites for sprites, music, or disks
- full remote multi-host networking from day one
- live multi-user collaboration from day one
- every reasonable peripheral from the beginning
- `D`-bucket or 3D-era machine support

## Notes For The Initial Four Families

- `Spectrum` should force the platform to get keyboard matrices, tape flows, BASIC injection, and contention-aware stepping right.
- `NES` should force the platform to get controller mapping, assembly workflows, sprite or tile inspection, and cartridge-backed teaching flows right.
- `C64` should force the platform to get BASIC workflows, raster-visible inspection, drive or loader policy, and audio channel handling right.
- `Amiga` should force the platform to get richer device inspection, DMA-aware tracing, floppy workflows, and larger-machine teaching constraints right.
- The first four families should also force the platform to prove curriculum independence, pattern portability, authenticity-vs-convenience labeling, and provenance-aware captures before the broader catalog expands.
