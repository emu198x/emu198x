# Emulator Suite Coherent Development Plan

**Status:** Working plan  
**Date:** 2026-04-12

## Purpose

Turn the current documentation set into one executable plan for building a suite of cycle-accurate vintage computer and console emulators in Rust.

This plan assumes the existing repository is a **reference and planning archive**, not a trustworthy snapshot of a current implementation. Several documents describe prior attempts, aspirational end states, or superseded architecture. This file resolves those conflicts.

## Planning Assumptions

- The implementation should start from a **fresh Rust workspace**.
- Old codebases are reference material only. Carry forward **knowledge, tests, fixtures, ROM handling policy, and documentation**, not code.
- The project has **two separate scopes**:
  - **Product scope:** the first families that must shape the platform and reach a polished user-facing release.
  - **Preservation scope:** the long-term catalog of families and variants the suite may eventually cover.
- Accuracy is a release criterion, not a backlog item.

## Source Of Truth Hierarchy

When documents disagree, use this order:

1. this plan
2. `wiki/decisions/`
3. `docs/testing-policy.md`
4. current per-family planning docs and registry docs
5. older architecture, inventory, roadmap, status, and handoff notes as historical context only

Practical effect:

- `docs/status.md`, `docs/inventory.md`, `docs/roadmap.md`, and `docs/adding-a-system.md` contain useful ideas, but their claims of current completeness are **not authoritative**.
- `wiki/decisions/` is the current decision layer for architecture.
- Any future replacement for this plan should be another dated plan file, not an undated edit scattered across the archive.

## Decisions This Plan Revises

This plan intentionally challenges several earlier positions.

- Earlier documents that describe major families as already complete or production-ready are treated as **historical overstatement**, not current state.
- The broader "wave 1 must prove every scheduler bucket immediately" idea is narrowed. The suite should start with the four anchor families, not a larger showcase set that dilutes execution.
- The earlier tendency to treat `OCS`, `ECS`, and `AGA` as one near-term Amiga deliverable is rejected. The coherent path is `A500 OCS` first, `ECS` second, `AGA` later.
- Any earlier willingness to permit temporary emulation shortcuts for storage, media, or peripherals is rejected. If a component is not yet modeled accurately, the profile stays incomplete rather than faking the path with a convenience implementation.
- Product ambitions such as native UI, WASM, and launcher polish are demoted behind core correctness and shared control-surface stability.
- Schedule language from earlier roadmap material is treated as planning pressure, not permission to lower the accuracy bar or promote under-verified systems.

## Non-Negotiable Engineering Rules

These are the stable decisions that should define the new implementation:

- **Rust everywhere** for cores, shared tooling, automation, and WASM-capable targets.
- **Fresh implementation only.** Old code is reference, not source.
- **Tick from the master oscillator or authoritative hardware clock tree**, not from CPU-frequency batching.
- **Pin-level CPU interfaces only.** No `Bus` trait, no callback-driven CPU memory API.
- **System-specific run loops.** Shared tooling may call `run_frame()` or `run_until()`, but each family owns its internal timing model.
- **All major system components are modeled directly, not shortcut around.** CPUs, video, audio, DMA, storage, media controllers, bus arbitration, and peripherals should be emulated as accurately as current evidence allows; if they are not ready, the system remains at a lower support tier.
- **Headless, deterministic cores.** Filesystems, windows, audio devices, sockets, and UI loops stay outside the core.
- **One binary per family** with shared support crates where reuse is real. A launcher may exist on top; it is not the architectural center.
- **Capabilities are discovered, not assumed.** Sprite extraction, scanline stepping, BASIC injection, mouse support, and similar features are per-family and per-profile.
- **All external artifacts are versioned.** Save states, traces, media manifests, capability schemas, and teaching artifacts must have explicit versions.
- **Serde + postcard** is the default snapshot wire format.
- **Primary sources and measured behavior beat emulator folklore.** External emulator behavior is a fallback oracle, not primary truth.

## What The Suite Is Actually Building

The suite should be treated as five layers:

| Layer | Responsibility |
|---|---|
| Reference layer | docs, ROM policy, source provenance, validation fixtures, research notes |
| Core crates | CPU, chip, peripheral, and format crates with isolated tests |
| Machine crates | concrete system-family implementations with hardware-accurate run loops |
| Shared platform crates | registry, control surface, capture, trace, validation, manifests, persistence |
| Family products | per-family binaries, headless runners, native frontends, optional unified launcher |

The shared layer should stay narrow. Share:

- machine/profile registry
- control and inspection surface
- media manifests and writeback policy
- trace and capture infrastructure
- validation harnesses
- snapshot envelope/versioning
- host input/audio/video plumbing

Do **not** prematurely share:

- CPU/bus abstractions across unrelated processors
- one universal run loop
- one universal mapper API
- one generic PPU/video trait
- one generic cartridge or disk controller abstraction

## Support Tiers

Every family profile should move through the same support tiers:

- `research`: clock tree, memory map, register map, firmware requirements, source provenance, and validation inputs are assembled
- `boots`: firmware, ROM monitor, or baseline diagnostic path runs
- `usable`: representative software path works with known gaps documented
- `teaching`: control, inspection, capture, scripting, and snapshot workflows are stable enough for guided use
- `reference`: verification ladder is complete, provenance is recorded, and remaining uncertainty is explicit

No profile should be described as complete, production-ready, or teaching-ready without a verification matrix that matches `docs/testing-policy.md`.

## Program Structure

The work should proceed in five tracks that run in parallel but have clear priority.

### Track 0: Governance And Documentation Hygiene

Goal:

- stop documentation drift from reintroducing architecture drift

Work:

- create a fresh workspace and treat this repo as the planning/reference root until code lands
- mark superseded docs as historical where needed
- add a lightweight provenance vocabulary to every behavior claim:
  - `documented`
  - `measured`
  - `inferred`
  - `emulator-derived`
- require every new architectural decision to land in `wiki/decisions/`
- require every family profile to maintain:
  - a plan
  - a support tier
  - a verification matrix
  - an uncertainty list

Definition of done:

- no active document claims current implementation state without tests or a verification matrix
- future disagreements can be resolved by document order instead of memory

### Track 1: Shared Platform Substrate

Goal:

- build the smallest shared surface that all families can honestly share

Shared crates should cover:

- machine and profile registry
- capability discovery
- media manifests and writeback policy
- artifact versioning
- snapshot envelope
- trace/event schema
- validation harness
- frame/audio sinks
- input abstraction at the host boundary
- CLI/headless control surface

Recommended core boundary:

```rust
trait MachineCore {
    fn identity(&self) -> MachineProfile;
    fn reset(&mut self, kind: ResetKind);
    fn load_media(&mut self, media: &MediaSet) -> Result<(), MachineError>;
    fn run_until(&mut self, target: MachineTime, host: &mut HostIo) -> RunResult;
    fn snapshot(&self) -> Result<Vec<u8>, MachineError>;
    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError>;
    fn capabilities(&self) -> CapabilitySet;
}
```

The important constraint is not the exact method names. It is that the shared boundary stays **outside** the hardware loop.

Definition of done:

- one family can run headless through the shared control surface
- snapshots, trace events, and media manifests have explicit versions
- host tools do not need family-specific code for basic boot, run, capture, and inspect flows

### Track 2: Anchor Families

Goal:

- build the four families that should define the architecture and first release

The anchor set is:

1. `spectrum_family`
   - baseline: `48K PAL`
   - next: `128K PAL`
   - later in family: `+2`, `+3`
2. `commodore_64_family`
   - baseline: `C64 PAL breadbin`
   - next: `C64 NTSC`
   - later in family: `C128`
3. `famicom_nes_family`
   - baseline: `NES/Famicom NTSC`
   - next: PAL profile and wider mapper set
4. `amiga_family`
   - baseline: `Amiga 500 OCS PAL`
   - next: `A500+` / `A600` ECS
   - much later: AGA and CD variants

These four families are the release-shaping commitment. They are also the reference families for:

- keyboard-first 8-bit micros
- cartridge-first 8-bit consoles
- timing-sensitive raster and bus contention machines
- DMA-heavy 16-bit computers

#### Delivery order

Implement them in this order:

1. Spectrum
2. C64
3. NES
4. Amiga

Reason:

- Spectrum is the clearest starting point for the pin-level, clock-true architecture.
- C64 establishes the reusable 6502 path and contention-heavy computer model.
- NES reuses the 6502 path but forces cartridge and PPU discipline.
- Amiga is the capstone DMA/arbiter family and should not shape the whole substrate before the simpler families prove it.

#### Family-specific scope rules

Spectrum:

- `48K PAL` must reach `reference`
- `128K PAL` must reach at least `usable` before moving deep into +2/+3 variants
- clone-specific timing remains later-family work

C64:

- target the full architecture from day one: 6510, VIC-II, SID, CIA, IEC/1541 path
- do not substitute loader shortcuts, ROM hooks, or fake drive paths for real storage-device emulation
- do not label C64 teaching-ready until the storage path reflects real software expectations

NES:

- `NROM`, `UxROM`, `CNROM`, and `MMC1` define the baseline
- `MMC3` is required before the family is considered broadly teaching-ready
- `FDS` and expansion audio are family-expansion work, not baseline blockers

Amiga:

- start with `A500 OCS PAL` only
- require a real bus-arbitrated chipset model: Agnus, Denise, Paula, and CIA behavior at the correct timing granularity
- defer ECS extensions until OCS is stable
- defer AGA, CDTV, CD32, IDE, and accelerator-heavy profiles until after the OCS/ECS family is solid

### Track 3: Product And Tooling Layer

Goal:

- make the cores useful without polluting them with product code

Must-have platform features for the first release of the anchor families:

- headless execution
- screenshot capture
- audio capture
- deterministic scripted input playback
- snapshot/restore
- trace capture
- capability-driven inspection
- minimal debugger surfaces
- MCP/automation adapter

Important constraint:

- these features run on the shared control surface
- none of them justify weakening the timing model

Frontend policy:

- cores remain headless Rust libraries
- thin family runners are acceptable early
- native frontends are the long-term product path
- a unified launcher is optional on top of per-family products, not a blocker for core correctness

### Track 4: Catalog Expansion

Goal:

- grow the suite by leverage, not by whim

After the anchor families are stable, expand in waves:

#### Wave 2: Family Deepening And Adjacent Reuse

- Spectrum late-family work: `+2`, `+3`, disk-backed variants
- C64 family expansion: `C128`
- NES family expansion: `MMC3`, `FDS`, expansion audio, clearer NTSC/PAL separation
- Amiga family expansion: ECS `A500+` and `A600`
- handheld/adjacent reuse where the substrate is ready:
  - `Game Boy`
  - `Master System`
  - `Game Gear`

#### Wave 3: High-Leverage Flagship Families

Prefer families that compound existing CPU or chip work:

- `MSX`
- `BBC Micro`
- `Amstrad CPC`
- `Apple II`
- `Atari 8-bit`
- `CoCo/Dragon`
- `ColecoVision`
- `Atari ST`
- `Mega Drive`
- `SNES`

#### Wave 4 And Beyond

- advanced non-dynarec families after the architecture is proven
- dynarec/event-heavy late systems only after a separate `D`-bucket substrate is intentionally designed

Rule:

- do not start `D`-bucket systems while the `B`, `T`, `S`, and `H` foundations are still unstable

## Validation Strategy

Use the testing ladder in `docs/testing-policy.md` as a release gate, not a suggestion.

For every reusable crate:

- contract tests
- functional tests
- timing tests
- machine integration confirmation
- reference-program or differential confirmation

For every family:

- keep a family-level verification matrix
- record the source of each behavior claim
- document disagreements between sources explicitly
- promote support tiers only when the corresponding verification evidence exists

For every bug fix:

- add a regression test or record why one is not practical

## Immediate Execution Plan

The next implementation cycle should do this in order:

1. create the fresh Rust workspace and crate skeletons
2. define the machine/profile registry schema and support-tier schema
3. define the minimal shared control surface
4. stand up the validation harness and artifact versioning
5. implement Spectrum `48K PAL` as the first full reference family
6. implement C64 `PAL` as the first 6502 and drive/media-heavy family
7. implement NES `NTSC` with the baseline mapper set
8. implement Amiga `A500 OCS PAL` as the first DMA/arbiter capstone family
9. harden shared tooling across those four families
10. only then promote the roadmap into family expansion waves

## What This Plan Explicitly Rejects

- treating old implementation claims as current fact
- porting old code into the new workspace
- lowering the accuracy bar to hit a milestone
- forcing all families through one run loop, one bus abstraction, or one universal chip API
- making AGA or CD Amiga support part of the first Amiga baseline
- using a shortcut implementation in place of an accurately modeled component
- starting long-tail families before the anchor families and shared substrate are stable
- starting dynarec-era platforms before the non-dynarec suite is mature

## Canonical References For This Plan

- `wiki/decisions/fresh-start-rationale.md`
- `wiki/decisions/cpu-bus-interface.md`
- `wiki/decisions/no-bus-trait.md`
- `wiki/decisions/system-specific-run-loops.md`
- `wiki/decisions/save-state-format.md`
- `wiki/decisions/product-roadmap.md`
- `docs/testing-policy.md`
- `docs/brainstorms/2026-04-11-code-like-198x-architecture-decisions.md`
- `docs/brainstorms/2026-04-11-code-like-198x-platform-requirements.md`
- `docs/brainstorms/2026-04-11-emulator-suite-wave-1.md`
- `docs/brainstorms/2026-04-11-emulator-suite-roadmap-waves-2-6.md`
- `docs/brainstorms/2026-04-11-emulator-suite-system-registry.md`
