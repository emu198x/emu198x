---
title: "feat: Spectrum completeness — architecture hardening, persistence, peripherals, accuracy, debugger"
type: feat
date: 2026-04-07
status: superseded
superseded_by: docs/plans/2026-06-08-spectrum-100-percent-plan.md
review: docs/reviews/2026-04-07-architectural-review.md
---

> **SUPERSEDED (2026-06-08).** Kept only as a historical record of how the
> architecture was built (the Phase −1/0/1 foundations — `emu198x-shell`, the
> System trait, the persistence/config work — have all landed; the
> implementation lives in the code and `RULES.md`). For the **current** state
> and remaining road to 100%, see
> [`2026-06-08-spectrum-100-percent-plan.md`](2026-06-08-spectrum-100-percent-plan.md)
> and the system doc [`docs/systems/sinclair/zx-spectrum/index.md`](../systems/sinclair/zx-spectrum/index.md).
> Do not treat the status lines below as current.

# Spectrum Completeness Plan

A sequenced plan to take the Spectrum line from "11 variants boot" to "feature-complete enough that a serious user would pick it over Fuse." Driven by the 2026-04-07 architectural review.

## Executive summary

**Progress as of 2026-04-09:** Phase −1 (safety net) and Phase 0 (architectural foundations) are **complete end to end** — 0.1 through 0.15 have all landed, including the October must-haves (headless runner, capture pipeline, input scripting, MCP server). Phase 3.1 is substantially done as a side-effect of 0.8. **Phase 1.1 (save state format v1 + Alt+S/Alt+L quick-save) shipped 2026-04-09 in commit `4d5d8b0`**, including the cross-cutting `hotkey-modifier-policy` decision and the `paths` module under `emu198x-shell`. Phase 1.2–1.8 are reframed as *exporters* under the new "loaded source files are immutable" rule (`RULES.md` items 22–24). Per-phase status lines below link to the landing commit(s).

Nine phases, sequenced by dependency. Phase −1 is the safety net (CI, cross-platform build, performance baseline, RZX replay infrastructure) that makes everything after it safe to refactor. Phase 0 is foundational architecture work, now including the cross-system shell layer and the October must-haves (headless mode, capture pipeline, input scripting, MCP server). Phases 1-2 unlock the features users notice immediately (save states, modern peripherals). Phases 3-5 close accuracy and recording gaps. Phases 6-7 are polish and long-tail.

The single most important property of this plan is that **Phase −1 happens before everything else, and Phase 0 happens before every feature phase**. Without the safety net, the Phase 0 refactoring is a series of silent regressions waiting to happen. Without Phase 0, every subsequent feature is linearly more expensive to add. Doing both first turns refactoring from "risky" into "mechanical" and turns feature work from "linear cost" into "constant cost."

The single most important *feature* in this plan is **DivMMC support** (Phase 2), because it's the gateway to current-day Spectrum software preservation and homebrew. Most things released for the Spectrum scene since ~2015 ship as DivMMC images.

**Note on the crate layering.** Phase 0.2 introduces a new cross-project crate `emu198x-shell` alongside the existing `common-sinclair-zx-spectrum`. Following [crate-naming conventions](../../knowledge/decisions/crate-naming.md), `common-sinclair-zx-spectrum` grows upward to hold the Spectrum-family runtime layer (Machine enum, snapshot loader, audio mixer, trait impls), and `emu198x-shell` is a new category for cross-project shared infrastructure (System trait, capture pipeline, save state framework). Two sibling crates extend the cross-project category: `emu198x-mcp` for the MCP server (separate from the shell so agents can import it without pulling in the capture pipeline) and `emu198x-launcher` (future) for the unified system picker. The SDL2 runner bin `emu-sinclair-zx-spectrum` stays as the Spectrum-specific frontend, now much thinner.

## Scope decisions from the 2026-04-07 cross-system conversation

These commitments from [the cross-system brainstorm](../brainstorms/2026-04-07-cross-system-shell-requirements-brainstorm.md) affect the shape of the plan and are recorded here for reference:

- **Addressing is `u64`** throughout the `System` trait. Free future-proofing, zero runtime cost.
- **Register access is string-keyed with `u64` values.** Accommodates every retro CPU from Z80 through 68000.
- **Media kind enum is `Tape / Disk / Cartridge / Optical / Snapshot`.** `Optical` covers CD-ROM, GD-ROM, DVD, LaserDisc, and Blu-ray — one variant, format determined by file bytes.
- **Speed control is a frontend concern, not a trait concern.** Trait always runs at native speed. The SDL frontend owns the pacer, with audio-preserving time-stretching (via `rubato` or equivalent) as a day-one requirement — pitch is preserved at all speeds.
- **`emu198x-mcp` is a separate sibling crate** to `emu198x-shell`, not a module inside it.
- **Full cycle-perfect 68000 and 6502 emulation** — same accuracy bar as the Z80, same fresh-start approach, same test-suite rigour (Tom Harte 68000 and 6502 suites as the ground truth).
- **Full 1541 second-CPU emulation** is the C64 target, with simple ROM-hook intercept as an acceptable fallback if the timeline forces it.
- **NES mapper target:** NROM, MMC1, MMC3 as the minimum for October.
- **Amiga target:** OCS + ECS + AGA chipsets. Allowed to slip past October if full accuracy requires it — accuracy is non-negotiable, timeline is flexible.
- **C64 and Amiga ROMs are user-supplied** with open-source fallbacks shipping (OpenROMs-style for C64, AROS Kickstart for Amiga).

## Phase −1 — Regression and cross-platform safety net

**Goal:** make the Phase 0 refactoring *safe*. Every item in Phase 0 touches every machine crate; without the safety net below, each one is a "hope I didn't break Pentagon" exercise and regressions land silently. This phase has to happen before anything else.

### −1.1 — GitHub Actions CI (M)

**Status:** done — commit `46b8802`.

Today there is no `.github/workflows/` directory. There is no automated check on push, no cross-platform verification, no enforcement that `cargo test --workspace` stays green across PRs. This is the single biggest blocker to confident refactoring.

**Scope:**
- `.github/workflows/ci.yml` with a matrix over macOS / Ubuntu / Windows
- Jobs: `cargo fmt --check`, `cargo clippy -- -D warnings` (at least on the crates that are clippy-clean today), `cargo build --workspace`, `cargo test --workspace --lib`
- Separate job for the ignored integration tests gated on a `EMU198X_ROMS` secret (if we go the CI-ROMs route) or skipped in CI with a local-only note
- Cache `~/.cargo/registry` and `target` between runs — the workspace is big enough that uncached CI will be slow
- Branch protection on `main` requiring CI green

### −1.2 — Test ROM strategy for integration tests (S)

**Status:** done — commit `a82ed04` (`EMU198X_ROMS` env var approach).

Several tests are `#[ignore]` because they need real ROMs (`boot_to_copyright`, screen tests, the +3 boot test). CI can't run them without ROMs. Three options, ranked:

1. **Ship a permissive test ROM.** The [World of Spectrum](https://worldofspectrum.org) archive has the Amstrad-released Sinclair ROMs under a distributable license; the existing wiki note already claims "Yes (Amstrad permission)" for the 48K, 128K, +2, and +3 ROMs. Commit those into a `test-roms/` directory and check them in. Biggest repo footprint (~512 KB total), simplest CI.
2. **GitHub Actions secret with a download step.** CI downloads ROMs at the start of each run from a private artifact. Keeps the repo clean but adds a CI secret dependency and a download step.
3. **Leave integration tests local-only.** CI runs unit + library tests; integration tests are a local contributor discipline. Cheapest, least safe.

Recommend option 1 for the Amstrad-blessed ROMs and option 3 for anything with murkier licensing (Pentagon, Scorpion, TR-DOS).

### −1.3 — Cross-platform build verification (S)

**Status:** done — commit `ce3660d` (macOS / Ubuntu / Windows matrix).

The code has probably never built on anything but macOS. Before committing to a cross-platform October launch, verify:
- Ubuntu build (SDL2 via `libsdl2-dev`)
- Windows build (SDL2 via `vcpkg` or bundled DLLs)
- All 11 variant binaries run and boot on each platform

Almost certainly reveals one or two `cfg(target_os)` guards, missing dependency installs, or SDL quirks. One session's work, load-bearing for everything else.

### −1.4 — RZX as replay test infrastructure (M)

**Status:** done — commit `457f08e` (`format-sinclair-zx-spectrum-rzx` parser + writer). Replay harness integration and baseline captures remain as later work but the format crate is in place.

RZX is an input recording format originally designed for regression testing, later adopted by the speedrunning scene. The format records every input event with frame-accurate timing; replay is deterministic.

**Why it matters here:** Phase 0 is a series of refactors that each need "Signal Part 3 still works correctly" verification. Unit tests and boot tests don't catch a subtle contention regression that only manifests three minutes into the demo. RZX replay does.

**Scope:**
- `format-rzx` crate: reader and writer for RZX v0.13+
- `ReplayHarness` in `common-sinclair-zx-spectrum`: loads a starting snapshot, replays an RZX stream against a machine, compares a final state hash
- Record a baseline session of Signal Part 3 (or several acid tests) against the current `main` and commit the RZX plus its expected state hash
- CI job that replays all baseline RZX files and compares state hashes

This is the single most valuable investment in regression protection we can make. It pairs directly with −1.1 (CI) and turns Phase 0 from "risky refactor" into "mechanical refactor with strong safety net."

### −1.5 — Baseline performance benchmark for `run_frame` (S)

**Status:** done — commit `92d7a4c` (criterion benches for 48K and 128K). Baseline 48K: ~1.81 ms/frame (~11× realtime on M2 Air).

The Z80 crate already has a criterion bench for `tick` (`crates/zilog-z80/Cargo.toml:17`), which is great, but there's no equivalent for `run_frame` across machines. Before Phase 5 (rewind, recording — both add overhead) and before Phase 0.6 (shared driver — a hot-path refactor), we need a baseline.

**Scope:**
- `criterion` bench in each machine crate measuring `run_frame` throughput
- Report speed as a multiple of realtime (e.g. "14.2× realtime on M2 Air, cold cache")
- Capture the numbers in `knowledge/tests/spectrum.md` alongside the existing test results
- Run as a reportable (not gating) CI job so regressions surface

---

**Phase −1 deliverables:** CI runs on every push, integration tests work in CI, the project builds on three platforms, RZX replay catches Spectrum-level regressions, performance regressions are visible. Zero new features. Nobody sees the difference until the first time Phase 0 would have silently broken TC2048 and CI catches it instead.

## Constraints and principles

These sit above all phases and discipline how features land.

**The SDL2 frontend does not grow in-app widgets.** No in-window menus, preference panels, tape browsers, key-remap dialogs, debugger overlays, or roll-your-own list controls. This is a direct consequence of the [native UI strategy decision](../../knowledge/decisions/native-ui-strategy.md) — the previous attempt with egui was explicitly rejected ("felt like a game engine UI pretending to be an app"), and rolling our own in SDL2 lands in the same place.

**Everything the SDL2 frontend does must land as one of:** a keyboard shortcut, a CLI flag, a TOML config file entry, or a native system dialog (via `rfd`). If a feature can't be expressed that way, it belongs on the native-frontend track — which is a separate plan, scheduled post-launch.

**Native system dialogs are fine.** `rfd` (see 0.11) shells to the real `NSOpenPanel` / GTK dialog / Win32 common dialog — it's not a UI toolkit, it's a one-function call that hands back a path. This is consistent with the "no fake-native widgets" rule because there's no widget, just a system service.

## Phase 0 — Architectural foundations

**Goal:** make every subsequent feature cheap to add. No user-visible behaviour changes.

These items are sequenced because each one makes the next one easier.

### 0.1 — Add serde derives to Z80 and machine wrappers (S)

**Status:** done — commit `5489bb4` (shared with 0.5).

**Why:** the wiki's [save state decision](../../knowledge/decisions/save-state-format.md) requires serde on every struct from day one. Commit `d044041` added derives to chips and ULAs but missed the `Z80` struct and the seven machine wrapper structs (`Spectrum48K`, `Spectrum128K`, `SpectrumPlus`, `Pentagon128`, `ScorpionZS256`, `TimexTC2048`, `TimexTS2068`). Without these, save states are blocked.

**Code:** `crates/zilog-z80/src/z80.rs`, each `crates/machine-*/src/lib.rs`. Where `Vec<u8>` framebuffers and `[u8; N]` arrays don't serialize cleanly, use `#[serde(skip)]` and reconstruct on load.

### 0.2 — Extract `emu198x-shell` and grow `common-sinclair-zx-spectrum` (L)

**Status:** done — commit `8ceb545` (extract `runtime-sinclair-zx-spectrum` and `emu198x-shell`). Ultimately landed as a `runtime-sinclair-zx-spectrum` crate rather than growing `common-sinclair-zx-spectrum` upward — see the crate's lib.rs header for the rationale.

**Why:** `crates/emu-sinclair-zx-spectrum/` is a binary crate that fuses SDL2, OpenGL, the `Machine` enum, audio mixing, snapshot loading, and file routing into one 1,000-line blob. Three planned native frontends (per [native-ui-strategy](../../knowledge/decisions/native-ui-strategy.md)) means three duplications — *and* the [product roadmap](../../knowledge/decisions/product-roadmap.md) commits to a unified launcher and shared shell infrastructure across Spectrum, C64, NES, and Amiga for October. The extraction is a cross-system concern, not a Spectrum-only one.

Three layers, following [crate-naming conventions](../../knowledge/decisions/crate-naming.md):

**`emu198x-shell` — new cross-system shared infrastructure crate.**
- The generic `System` trait (see 0.3) that Spectrum, C64, NES, and Amiga will all implement
- Capture pipeline primitives: framebuffer accessors, PNG encode, input injection, headless step
- MCP server (see 0.15) that wraps any `System` implementation
- Save state framework (format header, model identity, compression)
- File routing and ZIP handling that's format-agnostic
- No SDL2, no OpenGL, no audio output — pure library, no frontend

This is a new category extending the crate-naming convention: *cross-project shared infrastructure*, distinct from `{system}-common` which covers a single system family. Naming aligns with the roadmap's "shell infrastructure" language.

**`common-sinclair-zx-spectrum` — existing Spectrum-family shared crate, grows upward.**

Currently holds chip-level shared infrastructure: `MemoryBus` trait, `Ula` trait, `UlaEngine`, `BeeperAudio`, `TapePlayer`, `FrameTiming`. Phase 0.2 grows it upward with the *runtime* layer that's shared across every Spectrum variant:

- The `Machine` enum wrapping every Spectrum variant
- The `SpectrumSystem` trait (see 0.3) and its `impl`s
- Spectrum-specific snapshot loader (`.z80`, `.sna`), tape dispatcher, disk dispatcher
- The beeper + AY audio mixer (currently hardcoded in `main.rs:395-410`)
- Spectrum keyboard matrix translation
- `impl System for Machine` (or per-variant impls) — the bridge between `common-sinclair-zx-spectrum` and `emu198x-shell`

Strictly a library, no SDL2, no OpenGL. The "common code" category in the naming convention is the right home — this is code common to every Spectrum machine implementation, as distinct from chip code (which belongs in chip crates) and frontend code (which belongs in the runner bin).

**`emu-sinclair-zx-spectrum` — existing SDL2 runner bin, unchanged name, much thinner contents.**
- SDL2 init, OpenGL context, CRT shader, event loop, framebuffer→RGBA conversion, audio queue, hotkey handling, `rfd` file dialogs
- Imports `common-sinclair-zx-spectrum` for the runtime and `emu198x-shell` for cross-system interop
- Drops from ~1,000 lines to ~250

Future native frontends would be new sibling runner bins, still following the convention: `emu-sinclair-zx-spectrum-swiftui` (or similar), each importing the same libraries.

**Why this layering, not fewer layers:** the roadmap's "shared crate that every system links against" is `emu198x-shell`. The Spectrum-family runtime is one layer above that, in `common-sinclair-zx-spectrum`. When C64 arrives it follows the same pattern: a `c64-common` crate holds the C64 runtime layer, implements `System`, and is consumed by `emu-commodore-c64`. The MCP surface, capture pipeline, and launcher all live in `emu198x-shell` and work uniformly across systems.

**ROM source sentinel in the constructor path.** While extracting the shell, define a small `RomSource` enum in `emu198x-shell`:

```rust
pub enum RomSource {
    Path(PathBuf),
    Fallback,  // use the bundled open-source fallback (AROS, OpenROMs, etc.)
}
```

Each system's `new(roms: &[(&str, RomSource)])` (or equivalent) accepts this and the shell resolves `Fallback` to a bundled asset inside the relevant runner crate. The config file can then say `kickstart = "fallback"` and the loading path is uniform — no "if ROM missing, special-case" branches in each system's init. Costs five lines now, saves the first-run UX work from being scattered across every system when we get to Phase 6. The actual UX (file picker, "use fallback?" prompt, "where to get real ROMs" helper) defers to Phase 6 or the native frontends.

### 0.3 — Define the `System` trait (cross-system) and `SpectrumSystem` extension (M, requires 0.2)

**Status:** done — commits `58e45af` (trait definition) + `1939973` (Spectrum impl). Final shape diverges from the sketch below in two places: `run_frame` returns `()` not `u64` (no caller used the return), and `SpectrumSystem` lives in `runtime-sinclair-zx-spectrum` rather than `common-sinclair-zx-spectrum`. See `docs/handoffs/2026-04-08-phase-0-3-system-trait.md` for the brainstorm that settled the open questions, and the trait file itself at `crates/emu198x-shell/src/system.rs` for the as-built shape.

**Why:** `Machine` in `machine.rs` is 526 lines of which ~200 lines are 7-arm match statements. Adding a variant means editing ~12 methods. The [no-Bus-trait decision](../../knowledge/decisions/no-bus-trait.md) is about CPU↔machine; the *frontend↔machine* boundary is a different question and a trait pays for itself there.

Critically, the trait is **designed against four consumers simultaneously**, not just the SDL frontend:

1. The SDL2 frontend (runs a loop, queries framebuffer/audio)
2. **Headless capture mode** (runs N frames, returns PNGs and audio WAVs)
3. **MCP server** (agents call tools, step the emulator, inspect state)
4. **Native frontends** (eventually — SwiftUI/GTK4/WinUI call across FFI)

Every method needs to be callable from all four. That's a constraint on the shape.

**Shape — split into a cross-system `System` in `emu198x-shell` and a Spectrum extension `SpectrumSystem` in `common-sinclair-zx-spectrum`:**

```rust
// emu198x-shell — implemented by C64, NES, Amiga, Spectrum

pub enum MediaKind {
    Tape,
    Disk,
    Cartridge,
    Optical,
    Snapshot,
}

pub trait System {
    /// Run one frame at native speed. Returns CPU cycles consumed.
    fn run_frame(&mut self) -> u64;

    /// Run N frames (headless/capture use — skips audio output).
    fn run_frames(&mut self, n: u32);

    /// Native frame duration, for real-time pacing.
    fn frame_duration_us(&self) -> u32;

    /// Raw framebuffer. Palette format depends on the system.
    fn framebuffer(&self) -> FrameView<'_>;

    /// Audio sample generation. Interleaved if channels > 1.
    fn audio_channels(&self) -> u8;
    fn audio_sample_rate(&self) -> u32;
    fn audio_samples(&mut self, out: &mut [f32]);

    /// Generic input injection: each system defines its own button/key set.
    fn inject_input(&mut self, event: InputEvent);
    fn key_name_to_code(&self, name: &str) -> Option<u32>;

    /// Memory access — u64 addressing for future-proofing.
    /// Z80/6502 systems ignore high bits; 68000 uses 24 bits; larger systems use more.
    fn read_memory(&self, addr: u64, len: u64) -> Vec<u8>;
    fn write_memory(&mut self, addr: u64, data: &[u8]) -> Result<(), String>;

    /// Register access — string-keyed so MCP can talk to any system.
    /// Values are u64 to accommodate 68000 32-bit registers (and 64-bit systems if we ever add them).
    fn get_register(&self, name: &str) -> Option<u64>;
    fn set_register(&mut self, name: &str, value: u64) -> Result<(), String>;
    fn registers(&self) -> &'static [&'static str];

    /// Media.
    fn accepted_media(&self) -> &'static [MediaKind];
    fn insert_media(&mut self, kind: MediaKind, slot: u8, data: Vec<u8>) -> Result<(), String>;
    fn eject_media(&mut self, kind: MediaKind, slot: u8);

    /// Save state serialisation (serde + postcard) with model identity.
    fn save_state(&self, out: &mut Vec<u8>) -> Result<(), String>;
    fn load_state(&mut self, data: &[u8]) -> Result<(), String>;

    /// System identification — for MCP tool schemas and save state matching.
    fn system_id(&self) -> SystemId;
    fn model_id(&self) -> &str;

    /// Deterministic state fingerprint (hash of CPU + memory + peripherals).
    /// Used by RZX replay verification and differential testing.
    fn state_hash(&self) -> u64;
}

// common-sinclair-zx-spectrum — Spectrum-family extensions
pub trait SpectrumSystem: System {
    fn keyboard_mut(&mut self) -> &mut [u8; 8];
    fn kempston_mut(&mut self) -> &mut u8;
    fn ay(&mut self) -> Option<&mut Ay3_8912>;
    fn beeper(&mut self) -> &mut BeeperAudio;
    fn load_tape(&mut self, data: &[u8], is_tzx: bool) -> Result<(), String>;
    fn tape_play(&mut self);
    fn apply_snapshot(&mut self, snap: &Z80Snapshot);
}
```

Note: `insert_disk` has moved out of `SpectrumSystem` because `insert_media` on the base trait covers the disk-insert use case generically. Spectrum's disk-specific handling (TRD vs DSK routing) happens inside the system's `insert_media` implementation, looking at the data bytes or the caller-supplied hint.

`Machine` becomes either `Box<dyn SpectrumSystem>` or an enum that delegates — but with trait dispatch instead of match arms. The SDL frontend holds `Box<dyn SpectrumSystem>`; the MCP server holds `Box<dyn System>`.

**Don't over-generalise.** The `System` trait has to be minimal enough that C64, NES, and Amiga can implement it without contortion — we genuinely don't know what an Amiga's `inject_input` looks like yet. Start with the methods that are obviously universal (`run_frame`, `framebuffer`, `save_state`, `state_hash`) and refine as the C64 and NES implementations come online. The `SpectrumSystem` extension is where the Spectrum-specific stuff lives for now.

### 0.4 — Push snapshot apply into machines (S, requires 0.3)

**Status:** done — commit `2a8f8e9`. Each of the seven machine crates now has an inherent `pub fn apply_snapshot(&mut self, &Z80Snapshot)`; the runtime's `Machine::load_snapshot` collapsed to a 7-arm delegation. `SpectrumSystem` grew an `apply_snapshot` trait method that routes through `Machine::load_snapshot`. The three macros (`apply_regs!` / `apply_ay!` / `write_48k_page!`) are deleted.

**Why:** `Machine::load_snapshot` is 130 lines of macros (`apply_regs!`, `apply_ay!`, `write_48k_page!`) that pokes into the internal layout of every machine type. Each new variant adds another match arm. Move it into per-machine `apply_snapshot` methods on the `SpectrumSystem` trait.

### 0.5 — Carry model identity in machines (S)

**Status:** done — commits `5489bb4` (initial model identity for most machines, shared with 0.1) and `8d4d0ce` (explicit `Variant::Sixteen / FortyEight` on Spectrum48K, replacing the `memory.is_16k()` proxy). No typed `SpectrumSystem::model()` method landed — `System::model_id() -> &str` is sufficient; a shared cross-machine enum was deferred until Phase 1.1 actually needs it.

**Why:** `Machine::S48` doesn't know whether it was constructed as 16K or 48K. Save states will silently round-trip a 16K state into a 48K machine and produce wrong behaviour. Each machine carries its `Model` and `SpectrumSystem::model()` returns it. Save state format records the model in the header so loading can reject mismatches.

### 0.6 — Shared `SpectrumDriver` run loop (M)

**Status:** done — commits `fc657b5` (0.6A — trait + Spectrum48K conversion, bench gate) and `a3c1e48` (0.6B — fanout to the other six machines). Trait lives at `crates/common-sinclair-zx-spectrum/src/driver.rs`. Each machine impls ~7 short `#[inline(always)]` hook methods (`hc`, `frame_hc`, `tick_ula`, `cpu_clock_active`, `tick_cpu_and_bus`, `feed_irq`, `on_tstate`, `end_frame_ula`) and picks up `run_frame` as a provided method. Pentagon / Scorpion override `contended() = false`. The `#[inline(always)]` hints were load-bearing: without them, the trait indirection regressed the 48K bench by ~8%; with them, the bench came back to 1.84 ms (+1.7% vs baseline). 128K actually got faster (~23%) because its old hand-rolled loop lacked inline hints and carried a dead `cpu_ticks` counter. A `tick_peripherals` hook sits as a default no-op waiting for 0.7's peripheral bus consumers.

**Why:** seven machine crates have nearly-identical 30-50 line `run_frame()` implementations. They differ only in: whether to gate on `cpu_clock_active()`, whether to advance an FDC/Beta interface, and which `TIMING_*` constant to use. Every accuracy fix has to be applied seven times.

**Shape:** a `SpectrumDriver<U: Ula>` in `common-sinclair-zx-spectrum` that owns the half-cycle cadence (ULA tick → optional CPU tick → tape advance every 4 hc → AY tick every 8 hc) and calls back into the machine for `handle_bus()` and `tick_peripherals()`. Each machine's `run_frame` collapses to ~10 lines.

This does **not** contradict the [system-specific-run-loops decision](../../knowledge/decisions/system-specific-run-loops.md) — that decision is about cross-system (Spectrum vs C64 vs NES) universality. Within the Spectrum family, one shared loop is correct.

### 0.7 — Peripheral bus (M)

**Status:** done — commit `8cfdee1`. Shape diverges from the sketch below in one important way: the final design is a **trait** (`common_sinclair_zx_spectrum::peripheral::Peripheral`), not a `PeripheralBus` struct holding `Vec<Box<dyn Peripheral>>`. Each machine keeps its peripherals as typed fields and dispatches explicitly. Reasons: static dispatch preserves perf (0.6 proved the hot path is sensitive), every machine knows its peripherals at compile time, and the plan's "list of devices" wording can be retroactively satisfied by wrapping the existing trait later. `BetaDisk` and `Upd765a` implement the trait; `Upd765a` gained an `enabled: bool` field so +2A / +2B carry a disabled FDC without a model check. Beta disk's memory-read intercept (TR-DOS ROM at `$0000-$3FFF`) is deliberately **out of scope** — it's a memory-bus hook, not an I/O hook, and Pentagon / Scorpion still handle it manually. A second peripheral that wants the same hook will justify adding `read_memory` to the trait.

**Why:** every machine has hand-rolled `io_read`/`io_write` with port-mask matches. Adding a peripheral (Multiface, DivMMC, mouse) means editing every machine that supports it. The Beta disk's `claims_port(port)` pattern is the right shape — generalise it.

**Shape:** a `PeripheralBus` that owns a list of port-claiming devices. Each device implements:

```rust
pub trait Peripheral {
    fn claims_port(&self, port: u16) -> bool;
    fn read(&mut self, port: u16) -> u8;
    fn write(&mut self, port: u16, val: u8);
    fn tick(&mut self, hc: u32) {} // Default: no per-tick work
    fn on_m1(&mut self, addr: u16) {} // Default: no M1 hook
}
```

Beta disk and µPD765A both already fit this shape. Joysticks, Multiface, mouse, DivMMC, ZX Printer all become `impl Peripheral` plus a configuration step in `Machine::new`.

### 0.8 — Wire `BoardIssue` through to tape EAR feedback (S)

**Status:** done — commit `4fcafd8`. `FerrantiUla` stores the `BoardIssue`, `ear_feedback_bit()` consults it, and `read_fe` overrides bit 6 accordingly. Tests at `crates/ferranti-ula-6c001e/src/lib.rs` cover both Issue 2 (MIC-or-EAR drives bit 6) and Issue 3 (EAR alone drives bit 6) behaviour. This also substantially closes Phase 3.1 — see the note there.

**Why:** `BoardIssue::Issue2`/`Issue3` is plumbed into `FerrantiUla::new` but never read. Issue 2 boards have observably different EAR feedback to bit 6 of port FE. Trapdoor: someone will trust the parameter exists and discover later that it does nothing.

**Code:** `crates/ferranti-ula-6c001e/src/lib.rs` — store `issue` and consult it in the EAR feedback path.

### 0.9 — Document the 5 FUSE failures per-test (S)

**Status:** done — commit `8dee9fb`. `knowledge/tests/spectrum.md` lists each of the 5 failures, the instruction involved, and what would constitute a real regression versus an expected noise-level difference.

**Why:** [tests/spectrum.md](../../knowledge/tests/spectrum.md) accepts 5 FUSE failures with the catch-all "Tom Harte disagrees, we side with Tom Harte." That's defensible but it's a black box. List the 5 tests, the instructions involved, and the nature of the disagreement, so a future regression touching one of them isn't silently masked.

### 0.10 — Add `rfd` for native file dialogs (S)

**Status:** done — commit `7f5f650`. `rfd` 0.15 is in the workspace dependency table and wired to `crates/emu-sinclair-zx-spectrum/Cargo.toml`. Actual file-picker hotkey handlers land in Phase 6.5 / 6.6.

**Why:** the [native UI strategy](../../knowledge/decisions/native-ui-strategy.md) forbids building in-app UI in the SDL2 frontend, but users still need to open ROMs, insert disks, save snapshots, and pick screenshot folders. `rfd` is a tiny Rust crate that shells to the real platform file picker (`NSOpenPanel` on macOS, GTK file chooser on Linux, Win32 common dialog on Windows). It is not a UI toolkit — it's a single function call that hands back a path.

Using `rfd` lets the SDL2 frontend support "open…", "insert disk…", "save state as…", and similar file operations with genuinely native dialogs while staying inside the no-widgets discipline. Every file-handling feature in Phases 1-6 depends on this.

**Code:** add `rfd = "0.14"` (or current stable) to `crates/emu-sinclair-zx-spectrum/Cargo.toml`. Use from hotkey handlers in the SDL2 main loop. No changes to the runner library.

### 0.11 — SDL3 migration evaluation and execution (M, depends on 0.2)

**Status:** done — commits `2cfee9a` (initial SDL3 + SDL_GPU migration), `6cfd084` (SDL_GPU pipeline with CRT shader), `c19ab97` (restore input + CLI loading). Gotchas documented in memory at `project_sdl3_gpu_pipeline.md` — read before adding another SDL_GPU frontend or touching shaders.

**Why:** SDL3 shipped stable in January 2025 and represents the first major-version bump in 12 years. Four things it brings that are directly relevant to this project:

1. **SDL_GPU** — a platform-abstracted GPU API that targets Metal, Vulkan, and D3D12 natively. This would let us drop the `gl` crate dependency and the hand-rolled OpenGL shader compilation in `crates/emu-sinclair-zx-spectrum/src/main.rs`, and run on Metal natively on Apple Silicon without going through the OpenGL compatibility shim (which Apple deprecated years ago and could drop at any time).
2. **HiDPI on macOS done properly** — SDL2's logical-vs-pixel coordinate handling is a source of friction on Retina displays. SDL3 draws a clear line and the API makes you pick which you want. Directly relevant to the integer-scaling CRT shader.
3. **Gamepad API overhaul** — better support for modern controllers (DualSense touchpad/adaptive triggers, Xbox Series X, Switch Pro), cleaner force feedback, more reliable hot-plugging. Directly feeds Phase 2.3 (joysticks) and Phase 2.4 (mouse).
4. **Wayland as a first-class citizen on Linux** — future-proofs the Linux build.

**Why this depends on 0.2:** once the runtime layer has moved into `common-sinclair-zx-spectrum` and the SDL frontend is a thin shell, swapping the windowing layer underneath it is a contained, one-crate operation rather than a cross-cutting refactor.

**Scope of the work:**

1. **Evaluate the `sdl3` Rust crate.** Check current version, recent activity, examples, and whether SDL_GPU bindings are mature. As of April 2026 this crate has had 15+ months to settle after SDL3 stable — it should be in much better shape than the "usable but rough" state from early 2025. If it isn't, defer the migration and revisit.
2. **Spike: render one frame through SDL3 + SDL_GPU.** Replace the OpenGL quad blit and CRT shader with the SDL3 equivalent. If this works cleanly, commit to the migration; if it's fighting the API, stop and reassess.
3. **Migrate the SDL2 frontend.** Rename API calls, update init, move the audio queue to the SDL3 audio stream API, update the gamepad handling. Expected one focused PR, ~1 week of polish.
4. **Drop the `gl` crate dependency.** After SDL_GPU is in place, `Cargo.toml` loses a dependency and `main.rs` loses the `compile_shader` / `link_program` helpers. Cleaner codebase.
5. **Verify HiDPI rendering on a Retina display.** The CRT shader should look right, and integer scaling should be honoured at physical-pixel resolution.

**Decision gate:** if any of steps 1-2 reveal that the Rust SDL3 ecosystem isn't ready (crate is stale, SDL_GPU bindings missing, no migration path for the audio queue), abort and stay on SDL2. The fallback is fine — SDL2 is in long-term maintenance upstream, not deprecated.

**Don't do this before 0.2.** The extract-runner work is prerequisite; migrating the current fused `main.rs` would touch far more code than migrating a thin shell.

### 0.12 — Headless mode (M, depends on 0.3)

**Status:** done — commit `82966c6`. `HeadlessRunner<S: System>` lives at `crates/emu198x-shell/src/headless.rs`, owns the frame counter + accumulated audio buffer, and exposes `step_frames(n)`, `run_until(predicate)`, and `frame_view()` as the per-frame contract. Determinism is covered by a round-trip test that runs two 60-frame sessions from the same starting state and asserts the byte-for-byte framebuffer match. Every subsequent 0.13/0.14/0.15 consumer builds on this one type.

**Why:** the [product roadmap](../../knowledge/decisions/product-roadmap.md) commits to a headless capture mode as an October must-have. Today the emulator only runs through `main.rs` with a live SDL window — there is no way to run the machine without opening a window, which blocks video capture, agent control, CI replay testing, and input scripting.

**Scope:**
- `emu198x-shell` gains a `HeadlessRunner<S: System>` that drives a machine without any window or audio output device
- `run_frame` returns a `FrameResult { framebuffer: &[u8], audio: &[f32], cycles: u32, frame_number: u64 }`
- The runner owns the frame count and timing; callers can step N frames, step to a specific frame, or step until a predicate matches
- Deterministic: two headless runs with the same starting state and same input sequence produce the same `state_hash()` at the end

This is the foundation every capture pipeline consumer (PNG export, video recorder, MCP server, RZX replay) builds on.

### 0.13 — Capture pipeline APIs (M, depends on 0.12)

**Status:** done — commits `ce41dfb` (0.13A — `encode_png`, `WavRecorder`, `CaptureSession` driven by a `HeadlessRunner`) and `fefe16c` (0.13B — `VideoRecorder` via an `ffmpeg` subprocess on PATH, graceful failure when absent). All five API shapes from the scope list landed in `crates/emu198x-shell/src/capture.rs`. The ffmpeg dependency is deliberately runtime-discovered rather than compile-time — agents and CI environments without ffmpeg still get PNG + WAV + framebuffer capture.

**Why:** the roadmap calls for "PNG screenshots, video capture, input scripting, MCP." Each of those is a consumer of the headless runner; each needs a clean API surface in `emu198x-shell`.

**Scope:**
- **`CaptureSession`** in `emu198x-shell`: wraps a `HeadlessRunner` and records frames at a chosen cadence
- **PNG encoder**: `Frame -> png::encode` using the `png` crate, output to a path or a `Vec<u8>`
- **WAV writer**: audio samples accumulated across frames, written via the `hound` crate
- **Video capture**: ffmpeg invocation via `std::process::Command` is the pragmatic option — spawn `ffmpeg` with `-f rawvideo` stdin, pipe frames as they're captured, ffmpeg encodes to MP4/WebM. No ffmpeg-rs dependency, just the binary on PATH. Document as an optional capability and let it fail gracefully if ffmpeg isn't present.
- **Screenshot-on-demand**: method that captures one frame and writes it to disk, for hotkey use

This item is not SDL-specific and contains no widgets — it's all file-descriptor plumbing. Fits the no-in-app-UI discipline.

### 0.14 — Input scripting (M, depends on 0.12)

**Status:** done — commits `e67b05a` (0.14A — Spectrum `inject_input` + `key_name_to_code` lookup table by physical key name, prerequisite for any script consumer) and `c3448b6` (0.14B — `InputScript` TOML loader + `ScriptedRunner` at `crates/emu198x-shell/src/scripting.rs`). Format is TOML (not RON) because `toml` is already in the workspace for config files. A single flat `[[events]]` array with optional fields was chosen over `#[serde(flatten)]` + tagged enum after the latter hit known fragility in the toml crate. Dynamic scripting languages were deferred — the user agreed TOML is sufficient for the October deadline.

**Why:** the roadmap calls this out explicitly: "input scripting" as an October must-have. Different from RZX (which *records* actual play sessions); input scripting is *authored* — a declarative description of what to press when, for reproducible captures and automated testing.

**Scope:**
- TOML or RON format for scripts: `{ frame: 120, event: KeyDown("Space") }`, `{ frame: 240, event: JoystickUp }`, etc.
- `InputScript` loader in `emu198x-shell`
- `ScriptedRunner` that wraps a `HeadlessRunner` and injects events at the right frame boundaries
- Example scripts committed alongside integration tests: "load Manic Miner, get past the first screen, capture PNG"

Useful for: capture pipeline (scripted demos), regression testing (replay an input sequence, verify state hash), documentation (reproducible screenshots for blog posts).

### 0.15 — MCP server (L, depends on 0.3, 0.12, 0.13, 0.14)

**Status:** done — commits `0c15ed4` (0.15A — scaffold, rmcp 1.3 integration, `info` tool, cross-family `SystemInfo` in `crates/emu198x-mcp`), `d67c93c` (0.15B — seven primitives: `step_frames`, `read_memory`, `write_memory`, `get_registers`, `set_register`, `reset`, `trigger_nmi`; also boxed `MachineInner` variants to stop the debug-mode stack from blowing up inside tokio futures), `9c64f78` (0.15C — input / media / screenshot: `press_key`, `hold_key`, `release_key`, `set_joystick`, `load_media`, `screenshot`), and **this commit** (0.15D — `type_string`, `run_until`). Final shape diverges from the sketch below: the server is **not** generic `McpServer<S: System>` because rmcp's proc macros don't cleanly handle generics and the convenience tools are inherently system-specific. Instead, cross-family types (`SystemInfo`) live in `crates/emu198x-mcp`, and the concrete stdio binary lives in `crates/emu198x-mcp-spectrum` as `SpectrumServer`. Future systems (C64, NES, Amiga) will each get their own `*-mcp-*` binary re-using the lib. Tool naming is bare (no `emu_` prefix) — MCP disambiguates via server name in the host config. Sixteen tools total, all covered by in-process duplex-transport integration tests.

**Why:** the roadmap lists MCP as an October must-have. MCP (Model Context Protocol) lets agents call emulator tools over stdio JSON-RPC — load a snapshot, step N frames, read memory at an address, inject a keypress, capture a screenshot.

**Scope:**
- New crate `emu198x-mcp` — separate sibling to `emu198x-shell`, not a module inside it, so agents can import the MCP surface without pulling in the capture pipeline DSP code
- Depends on `emu198x-shell` for the `System` trait and headless runner
- Implements the MCP stdio transport and tool protocol
- Exposes the `System` trait as MCP tools:
  - `load_snapshot(path: string)`
  - `run_frames(n: u32)`
  - `run_until(predicate: string)` — simple DSL for "PC == 0x8000" etc.
  - `read_memory(addr: u16, length: u16) -> bytes`
  - `write_memory(addr: u16, bytes: bytes)`
  - `get_register(name: string) -> u16`
  - `set_register(name: string, value: u16)`
  - `inject_input(event: JSON)`
  - `capture_screenshot(path: string)`
  - `save_state(path: string)`
  - `load_state(path: string)`
  - `state_hash() -> u64`
- Tool schemas generated from the `System` trait signatures (via a proc macro or hand-maintained JSON)
- Binary entry point: `emu198x-mcp --system sinclair-zx-spectrum --model 48k` starts an MCP server on stdio

**Important constraint on the trait:** because MCP will drive the emulator, every method on `System` must be deterministic, serializable in and out, and callable without a frame loop running. This is why Phase 0.3 is explicit about designing the trait against all four consumers at once.

**Why this is L, not M:** the MCP protocol itself is simple, but tool schema generation, deterministic behaviour, and agent-friendly error messages are all real work. Budget for one full week plus polish.

---

**Phase 0 deliverables:** zero new user-visible features, but: every subsequent phase becomes 2-3× cheaper, the October must-haves (headless, capture, input scripting, MCP) are in place, and the shell layer is ready for C64/NES/Amiga to plug into. This is the phase that trades short-term speed for long-term velocity, and it's the one that's most tempting to skip. Don't.

## Phase 1 — Persistence

**Goal:** save it, load it, write it back out. Everything that takes existing in-memory state and gets it onto disk.

Depends on: Phase 0 (especially 0.1, 0.3, 0.5) — all complete as of commit `97a1ba6`.

**Kickoff notes (for the session that starts Phase 1):**

1. **Brainstorm first — do not jump straight to code.** Sub-items 1.4, 1.7, and 1.9 still have "Open questions" blocks — run `/workflow:brainstorm` on each before writing code. (1.1 was brainstormed on 2026-04-09; its decisions are recorded below and in the decision doc.) The user's rule: *"Before implementation, ALWAYS brainstorm first. We burned an entire session retrofitting accuracy because we skipped planning."*
2. **Recommended ordering:** 1.1 → 1.6 → 1.7 → 1.9 → 1.4 → 1.5 → 1.8 → 1.2 → 1.3. Rationale: 1.1 is the foundation every other sub-item leans on (✅ shipped 2026-04-09 in commit `4d5d8b0`). 1.6 and 1.7 jump to second place because in-memory disk writes are *essential for CP/M and +3 BASIC SAVE* — without them, half the +3 software is broken even within a single session, regardless of any export story. 1.9 (settings) lands next so the launcher/hotkey-overrides surface exists. 1.4 (`.tap` exporter) is the hardest piece of authoring work and deserves the most runway. 1.5/1.8 are trivial follow-ups to 1.4/1.7. 1.2 and 1.3 (snapshot exporters) drop to last because the Phase 1.1 save state format already captures everything more completely than `.z80`/`.sna` ever could — the writers exist for *interop with other emulators and the WoS archive*, not for any user workflow.

**Hard rule for every sub-item below: loaded source files are immutable.** Sub-items 1.2–1.8 are *exporters*, not round-trip writers. The emulator never overwrites a file it loaded from. Modifications either survive in save states (1.1) or get exported to a new user-chosen path. See `RULES.md` items 22–24 for the full statement.
3. **Rewind is out of scope for Phase 1.** The [save-state-format decision](../../knowledge/decisions/save-state-format.md) discusses rewind as a consequence of the snapshot format, but the ring-buffer + replay-forward machinery lives in Phase 5.5 (`### 5.5 — Rewind buffer`). If rewind comes up in a 1.1 brainstorm, note it and defer.
4. **Every prerequisite is in place.** Z80 + all seven machine wrappers have serde derives (commit `5489bb4`). System trait and `HeadlessRunner` exist. MCP tools can exercise new save/load paths for integration tests without needing a UI.

### 1.1 — Save state format v1 + quick-save/load hotkeys (M)

postcard-encoded binary, header `{ magic: "EMU1", version: 1, model_id: String, timestamp: u64 }`, then a postcard payload of the machine. Saves live at `~/.emu198x/saves/<family>/<name>.state`. Phase 1.1 ships exactly one named slot: `quick.state`.

Reject loads where `header.model_id` doesn't equal `system.model_id()` exactly. No relaxation, ever — see the [save-state-format decision](../../knowledge/decisions/save-state-format.md#model-match-is-strict-and-permanent) for the rationale.

**Hotkeys (macOS, Linux, Windows — identical):**

| Hotkey | Action |
|--------|--------|
| `Alt+S` | Quick-save to `~/.emu198x/saves/spectrum/quick.state` |
| `Alt+L` | Quick-load from the same file |
| `F5` | Tape-play — unchanged |

Alt is the *only* safe modifier on this emulator because `Ctrl` is SYMBOL SHIFT and `Shift` is CAPS SHIFT (both load-bearing for BASIC keyword entry and 48K navigation). See the [hotkey modifier policy](../../knowledge/decisions/hotkey-modifier-policy.md) for the full rule.

**Prerequisite sub-task (1.1.0):** `update_keyboard` at `crates/emu-sinclair-zx-spectrum/src/main.rs:333` gains a one-line guard that returns early if either `LAlt` or `RAlt` is held. Without this, pressing `Alt+S` fires the hotkey *and* types `S` into BASIC in the same frame.

**Command-line flag:** `--load-state <path>` auto-loads a save file at startup. This is the thing that makes 1.1 genuinely useful before the launcher (Phase 4) exists — save at an interesting point, close the emulator, relaunch tomorrow with `--load-state path/to/file.state`.

**Multi-slot / named slots are deferred to Phase 4 (launcher).** Power users in Phase 1 manage extra slots by filesystem (`cp quick.state boss-fight.state`) or through the MCP `save_state` / `load_state` tools (which will grow a `path` parameter). A slot-picker UI inside the SDL binary is launcher-lite scope creep and doesn't belong here.

As of Phase 0.1 (commit `5489bb4`) the Z80 and all seven machine wrappers have serde derives — no prerequisite work remains on the serialisation side.

**Brainstorm resolutions (2026-04-09):**

1. ✅ **postcard** picked over bincode — smaller output, `no_std`-friendly, stable wire format. Recorded in the decision doc.
2. ✅ **`<family>` directory, `model_id` in header.** Lowercased `Family` enum variants as directory names; the full `model_id` string carries variant info and drives the rejection check.
3. ✅ **Model mismatch = refuse with typed error.** Strict equality, never auto-switch, never prompt, never relax. `LoadError::ModelMismatch { save_model, current_model }` surfaced identically to CLI/MCP/future launcher.
4. ✅ **Hotkeys on Alt+S / Alt+L**, F5 stays on tape-play, no F1–F9 cluster. Ctrl and Shift are permanently off-limits as hotkey modifiers (they're SYMBOL SHIFT / CAPS SHIFT). `update_keyboard` gains an Alt guard as sub-task 1.1.0.
5. ✅ **Rewind deferred to Phase 5.5.** Nothing in 1.1–1.9 touches rewind machinery. The format is *designed* to be fast enough for a ring buffer (that's one reason we chose postcard), but no ring-buffer code lands in Phase 1.

### 1.2 — `.z80` exporter for interop (S, deferred to end of Phase 1)

**User goal:** "I have a save state I want to share with someone running a different emulator." The Phase 1.1 save state format is strictly more capable than `.z80` (it captures µPD765A controller state, AY register history, ULA cycle position, etc.), so this exporter exists *only* for interop with the wider Spectrum ecosystem (Fuse, ZEsarUX, Spectaculator, World of Spectrum's preferred snapshot format).

Write v3 (the most complete `.z80` revision). The current readers in `crates/format-sinclair-zx-spectrum-z80` cover v1/v2/v3 — the writer mirrors the v3 read path.

**Immutability rule applies:** `export_z80(machine, &Path)` always writes to a new path. If the user happens to pick the same path they loaded from, the call still goes through — the rule is about default behaviour and code paths, not file-handle gymnastics. The user typed the path; that's their decision.

Round-trip test (load `.z80` → export `.z80` → reload, compare machine state) is a valid *test technique* but not a user-facing feature.

### 1.3 — `.sna` exporter for interop (S, deferred to end of Phase 1)

48K and 128K `.sna` variants. Same framing as 1.2 — interop-only, exporter to a user-chosen path.

`.sna` is more constrained than `.z80` (no v3-equivalent, no compressed pages, no AY snapshot block) so this exporter loses *more* state than the `.z80` exporter. Document the loss in the function's doc comment so callers know what they're discarding.

### 1.4 — `.tap` exporter for SAVE-routine output (M)

**User goal:** "I wrote a BASIC program in the emulator and ran SAVE — I want a `.tap` file I can share or load on real hardware." The exporter watches the running machine's MIC line, decodes the ROM SAVE pulse pattern back into bytes, and writes a fresh `.tap` file.

**Always writes to a new path.** This sub-item never modifies an existing tape — there is no "append to a loaded TAP" code path, period. The user's loaded source TAPs are read-only preservation artifacts.

**Design notes for a 1.4 brainstorm:** the Spectrum ROM SAVE routine lives at `$04C2`–`$053F` (SA-BYTES and its callers). It writes tape data as pilot tone → sync pulse → bytes, with a strict pulse-width contract:

- Pilot: 2168 T-states per half-pulse, 8063 pulses for header blocks, 3223 pulses for data blocks
- Sync: 667 + 735 T-states
- Zero bit: 855 + 855 T-states
- One bit: 1710 + 1710 T-states
- Byte order: most significant bit first, with a parity byte at the end

The writer is a tiny state machine that watches MIC line transitions from the output port `$FE` bit 3, measures pulse widths in T-states, and emits bytes when it sees a valid one/zero run. False positives (user typing during SAVE, AY audio bleed) are handled by requiring a full pilot-tone lead-in before capture starts. Reference: the existing TAP/TZX **reader** at `crates/format-sinclair-zx-spectrum-tap/src/lib.rs` uses the same pulse shapes in reverse.

Open question: do we capture from a **port write hook** (precise, but couples the tape writer to the ULA), or from the **beeper sample stream** (decoupled, but lossy near the edges)? Recommend port hook for accuracy, with a feature flag to disable if it regresses the audio path.

### 1.5 — `.tzx` exporter (S after 1.4)

Once `.tap` exporting works, `.tzx` is trivial — it's TAP plus block metadata. Same immutability rule: always writes to a new path, never modifies a loaded TZX.

### 1.6 — WD1793 in-memory write path + TRD exporter (M)

**Two separable concerns under one sub-item:**

1. **In-memory write path (essential).** Type II Write Sector and Type III Write Track in `crates/beta-disk-interface/src/lib.rs:271,286` are currently no-ops (the "Write Sector (not implemented — TRD is read-only for now)" and "Read Track / Write Track (not implemented)" branches). Make them mutate the in-RAM TR-DOS image so TR-DOS `SAVE` works during a session. TRD is a flat sector layout — straightforward.

   Modifications survive across emulator runs *via the Phase 1.1 save state format* — the inner `Pentagon128` / `ScorpionZS256` machines serialize their disk state automatically through serde. No file-on-disk write is needed for in-session correctness or for "save your CP/M work and come back tomorrow."

2. **TRD exporter (authoring).** A `Machine::export_trd(&Path)` method writes the current in-memory TRD image to a new user-chosen file. Always a new path, never the loaded source.

The write-protect flag (set via TR-DOS commands or via a future config toggle) blocks the in-memory mutation path, not the exporter — the user can always export the *unmodified* image.

### 1.7 — µPD765A in-memory write path + DSK exporter (M)

**Same two-concern shape as 1.6.**

1. **In-memory write path (essential).** Write Data command in `crates/nec-upd765a/src/lib.rs:249` — currently decoded in the command table but the `execute_command` match arm is not wired up. Trickier than WD1793 because the structured `DiskImage` needs an in-memory mutation path. Without this, +3 BASIC `SAVE *"a:..."` and CP/M file writes silently fail.

2. **DSK exporter (authoring).** Write the current in-memory DSK to a new user-chosen path. Standard DSK only at this layer; non-uniform-track (EDSK) export ships in 1.8.

Modifications survive across runs through Phase 1.1 save states. The DSK exporter is for sharing or migrating to other emulators.

**Open questions for a 1.7 brainstorm:**

1. **DiskImage mutation shape.** Current `DiskImage` is probably an immutable view over parsed DSK bytes. Options: (a) clone-on-write per sector, (b) hold the original bytes plus a sparse override map, (c) fully parse to an owned representation up front. Recommendation: (c) — the extra memory is negligible and the code is simpler than (b), and a fully-owned representation serializes cleanly through serde for save states.
2. **Non-uniform tracks.** Standard DSK assumes uniform sector layouts per track. EDSK handles non-uniform tracks (copy-protected disks). 1.8 covers EDSK export. Does 1.7 gate on EDSK, or does it ship as "standard DSK only" and return an error from the *exporter* (not the in-memory write path) for EDSK-shaped images? Recommendation: latter — the in-memory path always works; the exporter is permitted to refuse formats it can't represent.

### 1.8 — EDSK exporter (S, depends on 1.7)

`format-amstrad-dsk` gains `export_edsk(image: &DiskImage) -> Vec<u8>` for non-uniform-track images. Most software is plain DSK, so this is a follow-up rather than a blocker.

Same immutability rule. Same "always a new path" pattern.

### 1.9 — Settings persistence (S)

`~/.emu198x/config.toml`: hand-editable TOML containing the user's preferred default model, audio volume, display scale and filter, tape autoplay, and an optional ROM path override. Phase 1.9 *reads* the file at startup; *writing* it is deferred to Phase 4 (launcher) where a settings UI exists. Hand-editing is the Phase 1 authoring path — that's the whole reason we picked TOML over a binary format.

**Schema (post-brainstorm 2026-04-09):**

```toml
# ~/.emu198x/config.toml — emu198x user preferences
#
# Hand-editable. Missing fields fall back to compiled-in defaults.
# Malformed files log a warning and fall back to defaults wholesale —
# the file is never auto-rewritten over a parse error.

[general]
# Default model when no --model flag is passed. Read-only in
# Phase 1.9; Phase 4 launcher will write it.
default_model = "sinclair-zx-spectrum-128k"

[paths]
# Persistent override for ~/.emu198x/roms. The EMU198X_ROMS env
# variable still wins over this — env > config > home > workspace.
# Comment out or delete to use the home-dir default.
# roms = "/usr/local/share/spectrum-roms"

[audio]
# Output volume multiplier. 0.0 = silence, 1.0 = unmodified.
volume = 1.0

[display]
# Window size = framebuffer × scale. Integer multiplier; sharp
# filter requires integer scaling for crisp pixels (see the
# integer-scaling note in memory).
scale = 3
# "sharp"  = nearest-neighbour filter, integer scale
# "linear" = linear filter, integer scale (looks softer)
filter = "sharp"

[tape]
# When a tape is loaded via CLI, immediately start playing it
# instead of waiting for F5. Useful for headless workflows.
autoplay = false

# Hotkey overrides land with Phase 2 (joystick mappings,
# peripheral hotkeys, NMI button, etc. all want bindings).
# Phase 1's three hotkeys (Alt+S, Alt+L, F5) are hardcoded.
```

**Brainstorm resolutions (2026-04-09):**

1. ✅ **`default_model` semantics = preferred default, not last-run.** Read on startup if no `--model` flag is given. Never written by Phase 1.9 itself. `--model 48k` is a one-time override that doesn't touch the file. Phase 4 launcher gets explicit "set as default" UI.
2. ✅ **`paths.roms` lookup order: env > config > home > workspace.** `EMU198X_ROMS` still wins so ad-hoc shell sessions can override the persisted preference without commenting out a TOML line. This matches the universal Unix convention.
3. ✅ **Hotkey overrides deferred to Phase 2.** Three hotkeys (Alt+S, Alt+L, F5) is too small a surface to justify a parser + reverse serializer + event-loop plumbing — all of which gets re-touched in Phase 2 anyway when joystick mappings, peripheral hotkeys, and the NMI button add a dozen more bindings. `[hotkeys]` is a comment in the schema, not a section.
4. ✅ **Aspirational fields ship if cheap, drop if not.** `audio.volume` (~3 lines), `display.scale` (~5 lines), `display.filter` (~30 lines for the second sampler path), `tape.autoplay` (~3 lines) all ship. `tape.fast_load` is dropped because no fast-loader exists yet — adding the field would create an inert knob that confuses users. It comes back in Phase 2 or 3 when the ROM-trap loader lands.
5. ✅ **Crash safety via atomic write.** Tempfile + `rename()` in the same directory. `Missing file` → silent default. `Malformed file` → one-line stderr warning and use defaults. **Never auto-rewrite a malformed file** — that destroys whatever the user was hand-editing. Forwards compatibility via `#[serde(default)]` on every field.
6. ✅ **Single config file, not per-system.** A `[spectrum]` table can be added later if Phase 4+ needs it; splitting across `~/.emu198x/systems/<family>.toml` is a strictly later optimisation.

**Shopping list:**

- New module `emu198x-shell::config` with `Config` struct, `Default` impl, `load()`, `save()` (atomic), parse-warning helper.
- `Default for Config` defines every compiled-in default (matching the schema comments above).
- SDL bin loads config on startup, applies all five fields (`default_model`, `audio.volume`, `display.scale`, `display.filter`, `tape.autoplay`).
- `common-sinclair-zx-spectrum::roms::rom_base` grows config-aware lookup with the new env > config > home > workspace order.
- Tests: round-trip default, load missing → defaults, load malformed → warn + defaults, load partial → fill in defaults from `Default`, atomic write via tempfile + rename.

No new wiki decision record — the choices here are mechanical and the brainstorm resolutions live in the plan doc itself. If a later phase wants to revisit any of this (e.g. moving to per-system files, or adding a settings UI that writes back), that's the moment to start a `knowledge/decisions/config-file.md` entry.

---

**Phase 1 deliverables:** users can quick-save their game with `Alt+S` and resume it tomorrow; CP/M and +3 BASIC `SAVE *"a:..."` actually write data the running program can read back; users can author a fresh `.tap` from a BASIC SAVE session, export a fresh `.trd`/`.dsk` from an in-emulator-modified disk, and (for interop) export `.z80`/`.sna` snapshots that other emulators can load. Throughout: the canonical artifacts the user loaded from are never touched.

## Phase 2 — Peripherals that unlock content

**Goal:** the peripherals that let you load software you couldn't load before.

Depends on: Phase 0.7 (peripheral bus).

### 2.1 — DivMMC + HDF format (L)

**The big one.** Modern Spectrum scene uses esxDOS on DivMMC almost exclusively. Without this, the emulator is useful for retrocomputing nostalgia but not for current-day software.

- New crate `divmmc` implementing the DivMMC interface (`$E3` port for paging, automapping on M1 fetch)
- New crate `format-spectrum-hdf` for HDF disk image parsing
- esxDOS ROM loaded from `~/.emu198x/roms/divmmc/esxdos.rom`
- Wires into 48K, 128K, +2, +3 (anywhere with a free expansion edge)

Sub-tasks: paging logic, SD card emulation, FAT16/32 read paths, esxDOS hooks.

### 2.2 — Multiface 128 + .mfa (M)

The pause button that interrupts the running program and dumps state. Many tape-only games are *only* preserved as Multiface snapshots.

- New crate `multiface-128`: NMI button, RAM page-in at $0000, ROM at $0000 when active
- `.mfa` snapshot reader/writer
- Hotkey to trigger (MagicKey)

### 2.3 — Joysticks: Sinclair, Cursor, Fuller (S each)

Built on the Phase 0.7 peripheral bus. Each is a small `impl Peripheral`:

- **Sinclair**: Interface 2 maps to keys 1-5 (player 1) and 6-0 (player 2)
- **Cursor**: Protek/AGF, mapped to cursor keys
- **Fuller**: port `$7F`, classic stick

Frontend gains config: gamepad → which Spectrum joystick(s).

### 2.4 — Kempston Mouse (S)

Ports `$FADF` (buttons), `$FBDF` (X), `$FFDF` (Y). Pointer for Art Studio, OCP and a handful of games.

### 2.5 — ZX Printer (S)

**Steve called this out specifically and he's right — it's cool.** The original Sinclair thermal/spark printer hangs off port `$FB` and outputs 32 columns of dot-matrix at the wall. Implementation:

- `impl Peripheral` claiming `$FB`
- Each byte written = 8 vertical pixels of one column
- Simulated paper as a long PNG that grows downward
- Hotkey to "tear off" the paper (save current PNG, start a new one)
- Saved to `~/.emu198x/printer-output/<timestamp>.png`

This is a very small feature with disproportionate joy. It's also a clean demonstration of the peripheral bus working — if the ZX Printer is a 50-line crate, the peripheral bus design is right.

### 2.6 — POK file loader (S)

Cheats database format. Apply pokes via keyboard shortcut or a small UI. Per-game pokes loaded from `~/.emu198x/pokes/<game>.pok`.

### 2.7 — WAV/FLAC tape input (M)

Decode digitised tape recordings (real cassettes captured to audio) into pulse sequences. Edge-detection on the audio with a hysteresis threshold. Lets you load tapes that exist only as audio captures, not as `.tap` files.

---

**Phase 2 deliverables:** any Spectrum software that exists in any common format will load. Plus you can print things to a virtual fax-roll, which everyone needs.

## Phase 3 — Accuracy gaps

**Goal:** close the known accuracy holes that aren't covered by Tom Harte. Most are small.

These can be done in any order and can mostly happen in parallel with Phase 2.

### 3.1 — Issue 2 vs Issue 3 EAR feedback (S)

**Status:** substantially done via Phase 0.8 (commit `4fcafd8`). The FerrantiUla models both boards — Issue 2 drives bit 6 high on MIC-or-EAR, Issue 3 only on EAR. What remains for a full 3.1 close: a regression test that exercises a real tape loader known to care about the distinction (e.g. a title that polls bit 6 to detect the board), verifying it behaves differently on `BoardIssue::Issue2` vs `BoardIssue::Issue3`.

Already started by 0.8. Now actually model the difference: Issue 2 reads back the MIC bit (bit 3), Issue 3 reads back the inverted EAR bit (bit 4). This breaks tape loading on some titles when wrong.

### 3.2 — Snow effect (M)

When the I register points into screen RAM (`I & 0xC0 == 0x40` on 48K), the ULA and CPU fight for the bus during refresh. Real hardware shows visual snow. Used as a visual effect by some demos.

### 3.3 — +2A/+3 late port read timing (S)

`$FF` port reads on +2A/+3 have idiosyncratic timing not present on Sinclair models. Affects a small number of demos.

### 3.4 — 128K floating bus subtleties (S)

128K floating bus pattern is slightly different from 48K (timing offset within the line). Document and fix.

### 3.5 — Bus contention during interrupt acknowledge (S)

The IM 2 vector fetch involves a contended cycle. Verify our IM 2 handler matches real hardware timing.

### 3.6 — Beeper resistor banding on Issue 2 (S)

Issue 2 boards have 4 distinct beeper levels (not 2) due to the resistor network. Currently we model 2-level beeper. Issue 3 cleanup made this single-level which is *less* correct historically. Quantise the beeper level on Issue 2 specifically.

---

**Phase 3 deliverables:** accuracy parity with Fuse on the cases real users care about.

## Phase 4 — Sound and video extensions

**Goal:** the optional chips that distinguished particular Spectrums or their add-ons.

### 4.1 — Currah µSpeech (S)

Speech synthesizer on port `$37`. Translates allophone codes to PCM samples. Software is rare but characterful.

### 4.2 — Specdrum (S)

8-bit DAC on port `$DF` for sample playback. Cheetah's drum sampler hardware. Used by a handful of music programs.

### 4.3 — Fuller Audio Box (S)

**Important: this is NOT the same as the bug we just fixed.** Fuller Audio Box was a real third-party AY add-on for the *48K* Spectrum, accessed at non-standard ports `$3F`/`$5F`. It's an optional peripheral, not built-in hardware. Implement as `impl Peripheral` so users can opt in on a 48K machine.

### 4.4 — ULAplus 64-colour palette (M)

Modern extension allowing 64-colour palette per attribute cell. Common in current homebrew, used by Crusader, Castlevania Spectral Interlude, etc. Port `$BF3B` for register select, `$FF3B` for data.

### 4.5 — General Sound (L, defer)

Z80 sound coprocessor with its own ROM, RAM, and sample playback. Used by some demos. Large undertaking — second Z80, sample mixer, dedicated audio loop.

---

**Phase 4 deliverables:** the soundscapes that the Spectrum scene actually uses, including modern homebrew.

## Phase 5 — Runtime capture and control

**Goal:** the hotkey-driven runtime features that make the emulator a preservation and review tool — everything that can be a keypress, not a panel.

The debugger is explicitly **not** in this phase. It needs a real UI (disassembly view, register view, memory hex editor, breakpoint list) and the [native UI strategy](../../knowledge/decisions/native-ui-strategy.md) places that on the native-frontend track, not in SDL2. See "Deferred to native frontends" below.

### 5.1 — Pause / fast-forward / slow-motion with audio-preserving time-stretch (M)

Pause is a single boolean. The rest is speed control with **audio-preserving time-stretching as a day-one requirement**: pitch is preserved at all ratios, not just at 100%. This rules out the naive approach ("play N% of the audio samples") and requires real DSP — probably the `rubato` crate for high-quality pitch-preserving resampling, or an equivalent Rust time-stretcher.

**Preset ratios via hotkeys:** 25%, 50%, 100%, 200%, 400%, plus "unlocked" (run as fast as the host CPU allows — typical tape loading acceleration) and a hold-to-turbo hotkey for temporary unlock while pressed.

**Where the speed logic lives:** the SDL frontend's main loop (and, later, each native frontend's main loop). The `System` trait does not know about speed — it exposes `run_frame()` which always runs one frame of emulation at native speed. The frontend decides how often to call it and runs the resulting audio through the time-stretcher before queuing to the audio device.

**Unlocked mode silences audio.** Time-stretching is bounded; infinite speed is not. At unlocked speed, the frontend skips audio output entirely and just runs emulation as fast as possible. This is the convention across emulators and the sensible behaviour for turbo tape loading.

**Headless capture always runs at 100%** and never engages the time-stretcher — captures need to match real hardware exactly.

**MCP has no speed concept.** Agents call `run_frames(n)` and get N frames of emulation as fast as the CPU allows; no real-time pacing is involved.

### 5.2 — Screenshot capture (S)

PNG of the current framebuffer to `~/.emu198x/screenshots/<system>-<timestamp>.png`. Hotkey.

### 5.3 — RZX recording/playback (M)

Input recording format used by the speedrunning community and demo verification. Records exact input timing relative to frame counter. Replay-driven testing: load a snapshot, replay an RZX, verify the resulting state matches expected. Triggered by hotkey; target files via `rfd` dialog.

### 5.4 — Audio recording (S)

Capture `audio_buf` to a WAV file while recording is enabled. Hotkey to start/stop; target file via `rfd`.

### 5.5 — Rewind buffer (M, depends on 1.1)

Ring buffer of save states (every N frames, e.g. every second). Hotkey to rewind. The [save-state decision](../../knowledge/decisions/save-state-format.md) explicitly anticipates this.

### Deferred to native frontends (not in this plan)

These were originally in Phase 5 but require a real UI and belong on the native-frontend track:

- **Built-in debugger** — breakpoints, single-step, register view, disassembly view, memory hex editor, watchpoints. The signal-level Z80 makes this *easier* to implement than usual (you can break on bus signal patterns, not just opcodes) but the widgets are non-negotiable. First killer feature for the SwiftUI/GTK4/WinUI frontends.
- **Video recording (GIF/MP4 export)** — platform-native APIs do this better than anything we'd write ourselves.

---

**Phase 5 deliverables:** a tool for preservation and review that stays within the no-widgets discipline. Everything that needs a panel is deferred.

## Phase 6 — SDL frontend polish (within the no-widgets discipline)

**Goal:** the SDL frontend stops being barely-usable, while staying strictly inside the constraint that everything is a hotkey, a CLI flag, a TOML config entry, or a native `rfd` dialog.

### 6.1 — CRT shader toggle (S)

Already exists in `main.rs:263`. Add a hotkey to switch between CRT and integer-scaled output. Remember preference in `config.toml`.

### 6.2 — Configurable key mapping via config file (S)

Move the hardcoded host→Spectrum keyboard mapping in `update_keyboard` out to `~/.emu198x/config.toml`. Users edit the file by hand. No remap UI in SDL2 — that's the native-frontend track.

### 6.3 — Gamepad → joystick mapping via config file (S, depends on 2.3)

TOML entry picks which Spectrum joystick (Kempston / Sinclair / Cursor / Fuller) the connected gamepad feeds. One entry per physical gamepad slot. No GUI mapper — the native frontends handle that.

### 6.4 — Issue 2/3 selector via CLI + config (S, depends on 0.8 and 3.1)

`--board issue2|issue3` flag plus TOML default. Trivial once the underlying wiring is in place.

### 6.5 — ROM picker via `rfd` (S, depends on 0.10)

If the configured ROM isn't found, prompt with a native file picker via `rfd` rather than `std::process::exit(1)`. Remember the picked path in `config.toml`.

### 6.6 — File operations via `rfd` (S, depends on 0.10)

Hotkeys for "open ROM / tape / disk / snapshot" and "save state as / save screenshot as". Each pops a native file dialog. Covers the file-handling subset of what most users want.

### Deferred to native frontends (not in this plan)

These were originally in Phase 6 but require real UI widgets and belong on the native-frontend track:

- **Preferences panel** — GUI for settings users would otherwise edit in `config.toml`. Natural as a native Settings window.
- **Key remapping UI** — interactive host-keyboard remap. TOML editing covers the near-term need; a proper remap panel belongs in SwiftUI/GTK4/WinUI.
- **Gamepad remap UI** — same argument.
- **Disk swap / eject panel** — status line showing inserted disks with eject buttons. Belongs in the native preferences.
- **Tape browser / block control** — scrollable list of TZX blocks with "play from here" / "skip block". Needs a real list widget.
- **Game library / launcher** — screenshots, metadata, last-played. Deferred to native frontends, or to a separate meta-launcher.

---

**Phase 6 deliverables:** a frontend you'd happily hand to a friend who is *also* comfortable editing a TOML file. The graphical consumer-product polish comes from the native frontends.

## Phase 7 — Long-tail peripherals

**Goal:** completeness for collectors. Defer until everything above is done.

- **Interface 1 + Microdrive** — original Sinclair disk peripheral. Complex (hookcode interception of ROM calls, unique tape format).
- **Plus D / DISCiPLE** — alternative disk system. Different from Beta and µPD765A.
- **Opus Discovery** — yet another disk system.
- **ZX Interface 2 cartridges** — ROM cartridge slot. Small.
- **Lightpen, Cheetah Mark V** — minor.

These are all real peripherals with real software, but the user base for each is small in 2026 and they're all separate engineering projects in their own right. Worth doing eventually for completeness, not worth blocking the main plan.

---

## Cross-cutting dependencies

```
Phase −1 (safety net) ───→ Every subsequent phase

  −1.1 (CI) ───────────────→ Gate on all PRs after setup
  −1.4 (RZX parser) ───────→ Phase 0.3 completes the replay loop
                            → Phase 1.1 integrates replay in CI
  −1.5 (perf baseline) ────→ Phase 0.6, Phase 5 regress checks

Phase 0.1 (serde derives) ──┐
Phase 0.3 (System trait) ────┼─→ Phase 1 (persistence)
Phase 0.5 (model identity) ──┤   → Phase 0.12 (headless mode)
                              │   → Phase 0.15 (MCP server)
                              └─→ Phase 5.5 (rewind)

Phase 0.2 (shell + common) ──→ Phase 0.11 (SDL3 migration)
                              → Phase 0.12 (headless mode lives in shell)
                              → Phase 0.15 (MCP server lives in shell)

Phase 0.7 (peripheral bus) ───→ Phase 2 (peripherals)
                              └─→ Phases 4.1, 4.2, 4.3 (sound peripherals)

Phase 0.6 (shared driver) ────→ Easier accuracy fixes in Phase 3

Phase 0.10 (rfd) ────────────→ Phase 5.2 / 5.3 / 5.4 (file targets)
                              └─→ Phase 6.5 / 6.6 (file operations)

Phase 0.12 (headless) ────────→ Phase 0.13 (capture APIs)
                              └─→ Phase 0.14 (input scripting)
                              └─→ Phase 0.15 (MCP server)

Phase 1.7 (µPD765A in-RAM writes) → Phase 1.8 (EDSK exporter)
Phase 1.4 (.tap exporter) ────────→ Phase 1.5 (.tzx exporter)
Phase 1.1 (save state v1) ✅ ─────→ Phase 5.5 (rewind)
```

Phases 3, 4, 6, 7 have no hard dependencies on each other and can interleave.

Phase 0.11 (SDL3) is the only item in Phase 0 that is *not* on the critical path — if the `sdl3` Rust crate turns out to still be too rough, the migration aborts and everything else in Phase 0 still completes. But the dependency ordering (after 0.2) makes it cheap to attempt.

Phase 0.12 through 0.15 collectively deliver the October must-haves for the capture pipeline and MCP. Nothing in Phase 1 or later blocks on them, but skipping them means missing the product roadmap commitment.

## Starting sequence

The order below turns "a big refactor with many risks" into "mechanical changes with a safety net." Don't skip ahead.

**Session 1 — Minimum viable CI + Phase 0 trivia**
Set up `.github/workflows/ci.yml` with a single macOS job running `cargo build --workspace` and `cargo test --workspace --lib`. Enable branch protection on `main`. Then do the three trivial Phase 0 items: serde derives on `Z80` and the seven machine wrappers (0.1), carry `Model` on each machine (0.5), wire `BoardIssue` through to tape EAR feedback (0.8). First commit is the CI workflow; next three are the small code changes. Every change after this runs through CI automatically.

**Session 2 — CI matrix + test ROM strategy + perf baseline**
Expand CI to macOS / Ubuntu / Windows. Verify cross-platform builds (Phase −1.3). Pick the test ROM strategy (Phase −1.2) and land it. Add the `run_frame` criterion benchmarks (Phase −1.5) and commit baseline numbers. This fills out the rest of Phase −1 except for RZX.

**Session 3 — RZX parser + replay harness stub**
Build `format-sinclair-zx-spectrum-rzx` (or `format-rzx` if the format is cross-system — it is, but that's a naming call; Spectrum-family for now). Reader and writer. The replay harness that ties RZX to CI is a Phase 1-era item because it needs the System trait, so this session just ships the format crate and a placeholder test that parses a known RZX file.

**Session 4 — Phase 0.2 (shell extraction + common-sinclair-zx-spectrum grows)**
Create `emu198x-shell`. Move the `Machine` enum, snapshot loader, audio mixer, file router, and keyboard translation into `common-sinclair-zx-spectrum`. `emu-sinclair-zx-spectrum` becomes a thin SDL2 bin. One larger commit, all mechanical. CI catches anything that broke.

**Session 5 — Phase 0.3 + 0.4 (the trait)**
Define `System` in `emu198x-shell` and `SpectrumSystem` in `common-sinclair-zx-spectrum`. Implement on each machine. Push snapshot apply down into machines. `Machine` enum's match arms collapse to trait dispatch. Larger commit, still mechanical, CI-protected.

**Session 6 candidate — Phase 0.11 (SDL3 evaluation)**
Immediately after the shell extraction is the cheapest moment to attempt SDL3 migration — the frontend is a thin shell and swapping the windowing layer is contained. Start with evaluation and a spike before committing to the full migration. If the `sdl3` Rust crate isn't ready, abort and continue on SDL2.

**After session 6, Phase 0 is roughly half done** and the critical path items are all in place: CI protecting everything, the trait layer defined, the shell extracted, SDL3 evaluated. Phase 1 (persistence, starting with save states) is unblocked, and Phases 0.12-0.15 (headless, capture, scripting, MCP — the October must-haves) can start in parallel with Phase 1 work.

## What this plan is not

- **Not a roadmap for other systems.** This is Spectrum-specific. The cross-system roadmap lives in [product-roadmap](../../knowledge/decisions/product-roadmap.md).
- **Not a design doc.** Each phase item will need its own design when picked up. This is a sequence and a sizing estimate, not a spec.
- **Not exhaustive.** There are accuracy edge cases and obscure peripherals that aren't here. Add them as they come up.
- **Not committed delivery dates.** Sizing is S/M/L (one session / two-to-three / four-plus). Translate to dates as needed.
