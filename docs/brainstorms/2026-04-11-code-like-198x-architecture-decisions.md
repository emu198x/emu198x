# Code Like It's 198x Architecture Decisions

**Status:** Draft
**Date:** 2026-04-11

## Purpose

Record the architecture decisions that matter early, before implementation starts fragmenting around local conveniences.

These decisions are written for:

- the initial `Code Like It's 198x` product scope: `Spectrum`, `NES`, `C64`, `Amiga`
- the long-term preservation scope: every machine, every variant, every region

Related planning files:

- [Code Like It's 198x platform requirements](./2026-04-11-code-like-198x-platform-requirements.md)
- [Emulator suite wave 1 plan](./2026-04-11-emulator-suite-wave-1.md)
- [Emulator suite system registry](./2026-04-11-emulator-suite-system-registry.md)
- [Code Like It's 198x commercial technique taxonomy](./2026-04-12-code-like-198x-commercial-technique-taxonomy.md)
- [Code Like It's 198x pattern library structure](./2026-04-12-code-like-198x-pattern-library-structure.md)
- [Code Like It's 198x pattern backlog](./2026-04-12-code-like-198x-pattern-backlog.md)
- [Code Like It's 198x historical exemplar registry](./2026-04-12-code-like-198x-historical-exemplar-registry.md)

## Decision Status

- `Accepted`: intended as the working default unless a later note replaces it
- `Open`: direction is clear, but the exact policy still needs narrowing
- `Deferred`: important, but not needed before the first implementation passes

## Decisions

### CLI198X-AD-001: Primary implementation language is `Rust`

- Status: `Accepted`
- Decision:
  - Use `Rust` for family cores, shared support crates, tooling, orchestration, and WASM targets.
- Reason:
  - The project is a platform, not just a collection of emulator cores.
  - The dominant cost is long-term maintainability across variants, regions, peripherals, capture, scripting, and remote control.
  - `Rust` is the safer base for versioned schemas, capability surfaces, concurrency, and WASM delivery.
- Consequence:
  - Core code should still be written in a hardware-first style.
  - The team should resist abstraction-heavy Rust patterns that obscure the machine model.

### CLI198X-AD-002: One binary per family, shared support crates where reuse is real

- Status: `Accepted`
- Decision:
  - Ship one binary or executable target per family.
  - Share crates for time, media, tracing, capture, validation, control, inspection, and host services where they materially reduce duplication.
- Reason:
  - Family binaries keep product boundaries and host startup paths clear.
  - Shared support crates keep instrumentation and education features coherent.
- Consequence:
  - Do not build one umbrella emulator application as the architectural default.

### CLI198X-AD-003: Family cores are headless and deterministic

- Status: `Accepted`
- Decision:
  - Cores do not own windows, audio devices, filesystems, sockets, or UI loops.
  - Cores execute against explicit host services and deterministic input streams.
- Reason:
  - This is required for scripting, MCP, automation, capture, validation, and WASM.
- Consequence:
  - All host interaction must cross a narrow boundary.

### CLI198X-AD-004: Product scope and preservation scope are separate planning axes

- Status: `Accepted`
- Decision:
  - The first `Code Like It's 198x` product scope is `Spectrum`, `NES`, `C64`, and `Amiga`.
  - The long-term suite scope remains the whole catalog.
- Reason:
  - A teaching product needs a coherent first release.
  - A preservation platform cannot let first-release choices distort the long-term catalog model.
- Consequence:
  - Product roadmaps and catalog roadmaps should reference each other, but not collapse into one list.

### CLI198X-AD-005: One shared control and inspection model, many adapters

- Status: `Accepted`
- Decision:
  - CLI tools, scripts, desktop tools, WASM embeds, and MCP all sit on top of the same control and inspection surfaces.
- Reason:
  - Divergent control paths would become impossible to keep consistent.
- Consequence:
  - MCP is an adapter, not a special execution path.

### CLI198X-AD-006: Capabilities are discovered, not assumed

- Status: `Accepted`
- Decision:
  - Features like sprite extraction, scanline stepping, BASIC injection, per-channel audio control, or printer support must be advertised per family and profile.
- Reason:
  - Not every machine has meaningful sprites, channels, scanlines, disks, or text-entry workflows.
- Consequence:
  - UI, scripts, and MCP must inspect capabilities before assuming a feature exists.

### CLI198X-AD-007: The machine and profile database is first-class

- Status: `Accepted`
- Decision:
  - Machine families, profiles, variants, regions, firmware requirements, media slots, and capability metadata live in a structured registry.
- Reason:
  - The project scope eventually includes every machine, variant, and region.
- Consequence:
  - Model metadata must not live in scattered booleans across binaries.

### CLI198X-AD-008: Support tiers and provenance are product features

- Status: `Accepted`
- Decision:
  - Every profile should carry a support tier and provenance notes for timing quirks, guessed behavior, unsupported features, and source quality.
- Reason:
  - Preservation and education both require honesty about what is known and what is not.
- Consequence:
  - The platform should never imply that every supported profile is equally complete or equally verified.

### CLI198X-AD-009: Capture and presentation are outside the core

- Status: `Accepted`
- Decision:
  - Cores emit raw machine output.
  - Screenshots, video export, audio export, and CRT or LCD filters live in host-side support layers.
- Reason:
  - The core should model hardware, not presentation preferences.
- Consequence:
  - Captures should record whether they came from raw output or filtered presentation output.

### CLI198X-AD-010: Asset extraction is a family-specific inspection feature

- Status: `Accepted`
- Decision:
  - Sprite, tile, pattern-table, bitplane, and audio-channel extraction live under family-specific inspection surfaces exposed through common capability rules.
- Reason:
  - The concept exists on some machines and not on others.
- Consequence:
  - Build the extraction surfaces now so future sprite or music editors do not require a redesign later.

### CLI198X-AD-011: Teaching flows are family-specific adapters on a common platform

- Status: `Accepted`
- Decision:
  - `Spectrum` and `C64` can expose BASIC-oriented lesson flows.
  - `NES` can expose assembly-oriented lesson flows.
  - `Amiga` can expose family-appropriate scripting or code-loading flows.
  - All of these still use the same control, inspection, and reusable pattern-library surfaces.
- Reason:
  - There is no honest universal "inject code" path across these machines.
- Consequence:
  - The platform needs family adapters for teaching flows, not one fake generic loader.

### CLI198X-AD-012: Input is modeled as host input, virtual controls, and machine adapters

- Status: `Accepted`
- Decision:
  - Separate host devices from virtual controls, and virtual controls from machine-local line or matrix changes.
- Reason:
  - This is required for keyboard matrices, gamepads, keyboard-to-joystick mappings, region-specific layouts, and automation playback.
- Consequence:
  - Host key bindings must not be hardcoded inside the family cores.

### CLI198X-AD-013: Peripheral modeling is descriptor-driven and service-based

- Status: `Accepted`
- Decision:
  - Printers, link devices, storage devices, network devices, and other peripherals expose descriptors, capabilities, and traceable traffic.
- Reason:
  - The platform eventually needs to host many reasonable peripherals across many families.
- Consequence:
  - Peripherals should attach through explicit service layers, not one-off host hacks.

### CLI198X-AD-014: Multi-machine orchestration starts local-first

- Status: `Accepted`
- Decision:
  - The first networking and orchestration work targets multiple local emulator instances under one orchestrator.
  - Remote-host orchestration comes later, on top of the same model.
- Reason:
  - Local lockstep and shared-host orchestration are easier to validate than distributed timing from day one.
- Consequence:
  - Remote links and network devices should be layered on the orchestration plane, not embedded into the core contract.

### CLI198X-AD-015: WASM is a first-class target, not a porting afterthought

- Status: `Accepted`
- Decision:
  - Initial-family cores and shared control surfaces must be designed so they can run under WASM.
- Reason:
  - `Code Like It's 198x` expects embeddable browser lessons.
- Consequence:
  - Filesystem, threading, and socket assumptions must stay outside the cores.

### CLI198X-AD-016: All external data formats are versioned

- Status: `Accepted`
- Decision:
  - Version snapshots, trace streams, media manifests, teaching artifacts, capability schemas, and remote-control payloads.
- Reason:
  - The project is intended to live for a long time and cover many machines.
- Consequence:
  - Backward compatibility policy must be explicit rather than accidental.

### CLI198X-AD-017: Determinism policy is strict inside a build, explicit across builds

- Status: `Open`
- Decision:
  - Treat deterministic replay within the same build, profile, and media set as a hard requirement.
  - Treat cross-version replay guarantees as an explicit policy question rather than an implicit promise.
- Reason:
  - Validation, teaching replays, and networking need a strong determinism floor.
  - Cross-version guarantees can be expensive and may need migration tools.
- Consequence:
  - Record enough metadata to explain what environment produced a trace, snapshot, or lesson artifact.

### CLI198X-AD-018: Security boundaries are explicit

- Status: `Accepted`
- Decision:
  - Cores never get arbitrary host filesystem or network access.
  - Browser, MCP, scripting, and remote-control surfaces run through explicit capability gates.
- Reason:
  - This platform will load untrusted media, scripts, lessons, and remote commands.
- Consequence:
  - Trust boundaries must be part of the design, not left to frontend code alone.

### CLI198X-AD-019: Editors are deferred, extraction APIs are not

- Status: `Accepted`
- Decision:
  - Sprite, tile, music, and asset editors are not required for the first release.
  - The extraction and inspection surfaces they depend on are in scope now.
- Reason:
  - The platform should not lock itself out of those tools later.
- Consequence:
  - "Not in v1" does not mean "ignored in the data model."

### CLI198X-AD-020: Authentic emulation and teaching conveniences are separate layers

- Status: `Accepted`
- Decision:
  - Keep the machine model and the teaching-assistance layer separate.
  - Paste, overlays, symbolic hints, guided stepping, rewind, and other conveniences are host-side features layered on top of the core.
- Reason:
  - Preservation and education both need clarity about what came from the original machine and what came from the platform.
- Consequence:
  - Captures, traces, and lessons should record whether convenience features were active.

### CLI198X-AD-021: Accessibility and localization are first-class product concerns

- Status: `Accepted`
- Decision:
  - Build keyboard-only workflows, scalable inspection views, locale-aware input handling, and explicit region or character-set differences into the product model.
- Reason:
  - This is an education platform, not just a specialist debugger.
- Consequence:
  - UI and lesson tooling should not assume one keyboard layout, one language, or one display mode.

### CLI198X-AD-022: Verification artifacts are core infrastructure

- Status: `Accepted`
- Decision:
  - Treat golden screenshots, trace baselines, audio artifacts, scripted inputs, and other regression assets as first-class infrastructure.
- Reason:
  - Preservation claims, support tiers, and teaching confidence all depend on reproducible evidence.
- Consequence:
  - Validation data should be versioned and tied to profile and media metadata.

### CLI198X-AD-023: Curriculum is emulator-agnostic; pattern artifacts are portable across frontends

- Status: `Accepted`
- Decision:
  - Lesson prose and code samples must remain usable outside the blessed emulator path.
  - Pattern-library artifacts and platform-specific teaching aids should run across native, headless, and WASM frontends where the required capabilities exist.
- Reason:
  - The curriculum predates the custom emulator path and should not become dependent on it.
  - Teaching content and working code should outlive any single UI shell or frontend.
- Consequence:
  - Platform-specific artifacts need stable metadata, but the lesson model itself should remain content-first rather than tool-first.

### CLI198X-AD-024: Collaboration is artifact-first, live sessions later

- Status: `Accepted`
- Decision:
  - Shared patterns, traces, captures, checkpoints, runnable examples, and reproducible runs are the first collaboration unit.
  - Live classroom or multi-user session control is a later layer on top of those artifacts.
- Reason:
  - Artifact-based collaboration is easier to validate, archive, and support across native and web environments.
- Consequence:
  - Do not let live-session design distort the initial core, lesson, and pattern-library model.

### CLI198X-AD-025: Legal status and provenance must be explicit

- Status: `Accepted`
- Decision:
  - Record whether an asset is redistributable, user-supplied, uncertain, or legally restricted.
  - Tie hardware claims and media manifests to provenance metadata where possible.
- Reason:
  - Preservation work without provenance becomes folklore.
  - Teaching products need a clean line between what can ship and what users must supply.
- Consequence:
  - Packaging, downloads, and lesson bundles must respect legal status rather than assuming everything can be shipped together.

### CLI198X-AD-026: Open exports are preferred

- Status: `Accepted`
- Decision:
  - Screenshots, captures, traces, pattern artifacts, and extracted assets should use open or well-documented formats where practical.
- Reason:
  - Preservation and education both benefit when artifacts survive outside the platform.
- Consequence:
  - Internal formats should not become the only durable representation of user work.

### CLI198X-AD-027: Extension boundaries are explicit and versioned

- Status: `Accepted`
- Decision:
  - Third-party peripherals, tooling, and integrations attach through explicit, versioned boundaries rather than by reaching into internal modules.
- Reason:
  - The platform will eventually attract family-specific tools and peripherals.
- Consequence:
  - Internal module layout is not the plugin API.

### CLI198X-AD-028: Research uncertainty should be visible

- Status: `Accepted`
- Decision:
  - When timing, device behavior, or support quality is uncertain, surface that uncertainty in metadata and UI rather than hiding it.
- Reason:
  - Honesty about unknowns is part of preservation and education quality.
- Consequence:
  - Support tiers and provenance notes should be queryable through the same inspection or metadata surfaces as other machine facts.

### CLI198X-AD-029: Source-aware teaching views are additive overlays

- Status: `Accepted`
- Decision:
  - Labels, symbols, disassembly helpers, and BASIC-aware views are allowed where a family can support them, but they remain overlays on top of raw machine state.
- Reason:
  - Educational clarity matters, but raw state must stay visible and authoritative.
- Consequence:
  - Family tooling can be richer without forcing all families into one symbolic model.

### CLI198X-AD-030: The pattern library is a first-class layer between lessons and emulators

- Status: `Accepted`
- Decision:
  - Reusable techniques should live in a pattern library with machine variants, working code, artifacts, and observability hooks.
  - Lessons reference patterns, but do not become the canonical store of reusable technical implementation.
- Reason:
  - Free-form curriculum and reusable technical craft are different concerns.
  - A pattern library scales across many families more cleanly than lesson-local duplication.
- Consequence:
  - The platform should support pattern-linked inspection presets and artifacts without forcing the lesson prose into a rigid schema.

## Open Questions

- How structured should pattern-library metadata be before it starts constraining the lesson authoring style?
- How strong should cross-version replay guarantees be for educational content?
- Which remote orchestration transport should be canonical: one custom protocol, MCP-first, or both?
- How much symbolic or assembler-aware inspection belongs in the base platform versus family-specific tooling?
- Should the first teaching release target only baseline profiles, or also one signature expansion profile per family?
- What is the minimum live-collaboration model worth supporting after the artifact-first foundation exists?
- Which export formats should be mandatory versus best-effort for each artifact class?
- How much provenance metadata is required before a machine profile can claim `teaching` or `reference` status?
