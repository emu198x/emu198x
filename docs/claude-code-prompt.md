# Emu198x — Claude Code Project Instructions

## What this project is

Emu198x is a cycle-accurate multi-system emulator targeting every 8-bit and 16-bit platform. It is written in Rust. The architecture is documented comprehensively in `docs/architecture.md` (the authoritative reference — read it before any implementation work). Supporting documents are `docs/launch.md` (productisation strategy) and `docs/references.md` (reference material catalogue).

This is a **clean-room implementation**. There is a prior version of emu198x on disk elsewhere. Do not reference, read, copy from, or be influenced by any existing emu198x code. Every implementation decision should derive from the architecture doc, the reference material in `refs/`, and first-principles engineering. If you encounter files from a prior implementation, ignore them entirely.

## Who you're working with

Steve is a senior engineer with ~20 years of professional experience, primarily in Ruby/Rails but increasingly in Rust. He has deep domain knowledge of the target hardware — he grew up with these machines, owns several of them, and has been building emulators for over a year. He treats you as a senior engineering peer and expects honest pushback when he's wrong.

Be direct. Match his technical depth. Skip preamble. Challenge premises during design; deliver pragmatically during execution.

## Project structure

Monorepo workspace. The architecture doc §34 defines the crate structure in detail. The workspace root is:

```
emu198x/
├── Cargo.toml                  # workspace root
├── docs/
│   ├── architecture.md         # THE authoritative technical reference
│   ├── launch.md               # productisation strategy
│   └── references.md           # reference material catalogue
├── refs/
│   ├── manifest.toml           # reference catalogue metadata
│   ├── cpu/                    # CPU datasheets and manuals
│   ├── systems/                # per-system reference material
│   ├── chips/                  # chip datasheets
│   ├── formats/                # file format specifications
│   ├── community/              # wiki snapshots, test suites
│   └── analysis/               # die photography, reverse engineering
├── crates/
│   ├── emu-machine/
│   ├── emu-observe/
│   ├── emu-debug/
│   ├── emu-audio/
│   ├── emu-input/
│   ├── emu-display/
│   ├── emu-capture/
│   ├── emu-export/
│   ├── emu-ide/
│   ├── emu-rewind/
│   ├── emu-config/
│   ├── emu-peripheral/
│   ├── emu-network/
│   ├── emu-mcp/
│   ├── emu-regression/
│   ├── emu-debug-views/
│   ├── cpu-z80/
│   ├── cpu-6502/
│   ├── cpu-m68k/
│   ├── machine-sinclair-spectrum/
│   ├── machine-sinclair-spectrum-views/
│   ├── machine-nintendo-nes/
│   ├── machine-nintendo-nes-views/
│   └── ...
├── shells/
│   ├── shell-native/
│   └── shell-wasm/
├── bins/
│   ├── emu198x-sinclair-spectrum/
│   ├── emu198x-mcp/
│   ├── emu198x-regression/
│   └── emu198x-tools/
└── tests/
    ├── cpu-validation/         # FUSE, Blargg, Tom Harte test harnesses
    ├── format-corpus/          # known-good and known-bad format test files
    └── integration/            # multi-crate integration tests
```

Do not create all crates upfront. Create them as needed, following the phased implementation sequence in the architecture doc §35. Start with Phase 1.

## Crate naming conventions

- `emu-*` — library crates (runtime infrastructure, never produce binaries)
- `emu198x-*` — binary crates (system binaries, MCP server, tools)
- `machine-{manufacturer}-{system}` — system-specific library crates
- `machine-{manufacturer}-{system}-views` — system-specific debug views
- `cpu-*` — CPU core library crates
- `format-*` / `parser-*` — media format library crates
- `shell-*` — platform-specific UI library crates
- `{manufacturer}-{chip}` — hardware-identity library crates

See architecture doc §34 for the full naming table with manufacturer examples.

## Coding standards

### Rust conventions

- **Edition:** 2021 (or latest stable at time of creation)
- **Formatting:** `rustfmt` with default settings. Run `cargo fmt` before every commit.
- **Linting:** `cargo clippy` with warnings as errors. Fix all warnings, don't suppress them unless there's a documented reason.
- **Error handling:** `Result<T, E>` everywhere. No `unwrap()` or `expect()` outside tests and provably-infallible cases. See architecture doc §29 for the error handling policy.
- **Panics:** Never panic in library crates. If something cannot fail, prove it with types, not with `unwrap()`.
- **Unsafe:** Avoid. If unavoidable (e.g., memory-mapped page tables for performance), isolate it, document why it's sound, and wrap it in a safe API.
- **Dependencies:** Minimise external dependencies. Prefer the Rust standard library. When a dependency is needed, prefer well-maintained, widely-used crates. Document why each dependency is needed.
- **Documentation:** Public APIs get doc comments. Internal functions get comments when the "why" isn't obvious. Hardware behaviour always gets `Ref:` citations (see below).

### Reference citations in code

Every implementation of hardware behaviour must cite its source. Use this format:

```rust
// The ULA contends memory access during active display with a
// pattern of 6,5,4,3,2,1,0,0 T-state delays repeating every 8 T-states.
//
// Ref: spectrum-ula-book, Chapter 7 "Memory Contention", pp. 147-162
// Ref: spectrum-contention (Ramsoft technical note)
fn apply_contention(&self, t_state: u32) -> u32 {
    // ...
}
```

The `Ref:` IDs correspond to entries in `refs/manifest.toml`. If a reference is needed but not cached, note it:

```rust
// TODO(ref): Need amiga-hw-ref for Paula audio DMA timing.
// Current implementation based on cross-referencing community documentation.
```

"Because another emulator does it this way" is never a valid citation. If the only source is another emulator, say so explicitly and mark it as needing verification against hardware documentation.

### Testing

- **Unit tests** in the same file (`#[cfg(test)] mod tests`).
- **Integration tests** in `tests/`.
- **CPU validation** uses external test suites (FUSE, Blargg, Tom Harte). These run as integration tests.
- **Format parsers** need: round-trip tests, known-good corpus, known-bad corpus (no panics), fuzz targets.
- See architecture doc §30 for the full testing strategy.

### Commit discipline

- Each commit should compile and pass tests.
- Commit messages: imperative mood, concise summary line, body if needed. E.g., "Implement Z80 LD group instructions with FUSE test validation".
- Don't commit dead code, commented-out code, or TODO placeholders without a tracking issue reference.

## How to read the architecture doc

The architecture doc (`docs/architecture.md`) is ~4700 lines across 37 sections. You don't need to read it all at once. Use it as a reference:

- **§1** — core ambition and architectural principles (read this first, always)
- **§2-3** — clock tree and scheduling (read before implementing any machine timing)
- **§4** — observation/debugger layer (read before implementing bus or memory access)
- **§5** — debug views (read before implementing any views crate)
- **§6** — display pipeline, CRT, speed control (read before implementing video output)
- **§7-8** — asset export and capture (read before implementing any export)
- **§9** — UI and window management (read before implementing shell)
- **§10** — audio pipeline and mixer (read before implementing any audio)
- **§11** — input system (read before implementing keyboard/joystick)
- **§12-13** — peripherals and networking (read when those phases arrive)
- **§14** — IDE, assembler, BASIC (read when that phase arrives)
- **§15-24** — media subsystems (read the relevant section when implementing that media type)
- **§25-26** — save state interaction, write-back
- **§27** — rewind/time travel
- **§28** — configuration and settings
- **§29** — error handling (read early, apply everywhere)
- **§30** — testing strategy (read early, apply everywhere)
- **§31** — reference management (read before implementing any hardware behaviour)
- **§32** — system variants, extensions, modern recreations (read before implementing any machine)
- **§33** — generalisation rules
- **§34** — crate strategy and naming (read before creating any crate)
- **§35** — implementation sequence (the roadmap — know where you are)
- **§36** — design rules (the principles — internalise these)
- **§37** — concise takeaway

## How to approach a new implementation task

1. **Read the relevant architecture section** before writing any code. The architecture doc contains trait definitions, struct layouts, and design rationale that should guide the implementation.

2. **Read the relevant reference material** from `refs/`. If the reference isn't cached, check `refs/manifest.toml` for the source URL and acquire it (download if freely available, note it as needed if not).

3. **Implement with citations.** Every hardware behaviour gets a `Ref:` comment pointing to the source document.

4. **Write tests alongside the implementation.** CPU instructions get test cases from validation suites. Format parsers get corpus tests. Timing gets mathematical validation.

5. **Check the design rules** in §36 before making architectural decisions. The rules exist because we've thought through the consequences.

## Key architectural decisions to internalise

These come up constantly. Know them by heart:

- **Master oscillator, not CPU clock.** Tick at the true crystal frequency. Component interleaving emerges from integer clock division. (§2)
- **Trait, not enum.** Canonical interfaces (TapeTimeline, DiskImage, OpticalDisc, AudioSource) are uniform traits, not discriminated unions. (§15, §16, §17, §10)
- **Flatten at import.** All source-format control flow (TZX loops/jumps) is resolved into seekable canonical form by importers. The runtime never interprets format-specific flow control. (§15)
- **One time unit per domain.** Master cycles inside machines. Nanoseconds at the media/audio/UI boundary. Never mix them. (§2)
- **Four media pathways.** Transport (tape/disk/optical), state artifact (snapshot), ROM (cartridge), persistent storage (memory card/SRAM). They're different pipelines. (§15-24)
- **Observation by default, cost by choice.** BusObserver hooks are always present. The cost of observation is zero when no observer is attached (branch-predicted not-taken check). (§4)
- **Views interpret, shells render.** System-specific debug view models produce renderer-agnostic DebugViewOutput. Shells never interpret hardware state. (§5)
- **Variants are configurations, not codebases.** A Spectrum 128K is a 48K with extensions pre-attached. The machine builds itself from a MachineConfig. (§32)
- **Every implementation cites its source.** No "because the other emulator does it." (§31)
- **Errors are values, not panics.** (§29)
- **Each system is independently releasable.** (§35)

## What not to do

- **Do not read or reference any existing emu198x code.** This is a clean-room implementation.
- **Do not create all crates upfront.** Follow the phased sequence. Create crates when the phase calls for them.
- **Do not define types for domains you haven't built yet.** DiskImage, SnapshotImage, and MicrodriveImage should not exist until their phases arrive. (§36: "Defer until exercised")
- **Do not optimise prematurely.** Get it correct first. The next-event scheduler and page-table memory dispatch are the performance work that matters — not micro-optimisations. (§3)
- **Do not add game-specific hacks.** Tolerant parsing, timing tolerance in importers, known-issues catalogue. Never special-case a game in the runtime. (§36)
- **Do not hardcode system-specific behaviour in shared crates.** If it's specific to the Spectrum, it belongs in `machine-sinclair-spectrum`. If it's shared infrastructure, it goes in `emu-*`. (§34)
- **Do not fight the architecture doc.** If you think the architecture is wrong, raise it as a design discussion. Don't silently diverge.

## Getting started

Phase 1 from the architecture doc:

1. Set up the workspace with `Cargo.toml` at the root.
2. Create `emu-machine` crate — `ClockFrequency`, `ClockDivisor`, `ClockTree`, `MachineConfig` skeleton.
3. Create `emu-observe` crate — `BusObserver` trait with no-op defaults, `ObservationFlags`.
4. Set up the `refs/` directory structure and `manifest.toml` with the initial Spectrum reference set (see `docs/references.md` "Priority for initial Spectrum bring-up").
5. Set up error handling types in a shared `emu-core` or within `emu-machine`.
6. Set up `tracing` integration for logging.
7. Write clock tree tests that validate Spectrum 48K timing: 14MHz master, ULA at ÷2 (7MHz), CPU at ÷4 (3.5MHz). Verify that the integer clock division produces exact ratios with no drift.

Then proceed to Phase 2 (observation foundation) and Phase 3 (audio pipeline), building the infrastructure that every system will use before starting on any specific machine.

The first machine is the ZX Spectrum. Its reference material is the priority acquisition target. Read the ULA book and the Z80 manual before writing the first line of Spectrum-specific code.
