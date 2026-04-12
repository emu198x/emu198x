# Handoff: Phase 0.3 — System trait in `emu198x-shell`

Written 2026-04-08 at the close of Phase 0.11. Phase 0.11 banked the
SDL3 + SDL_GPU + naga migration in three commits (`2cfee9a`, `6cfd084`,
`c19ab97`). Workspace is green, no in-flight work.

## Where to start

1. Read `docs/plans/2026-04-07-feat-spectrum-completeness-plan.md` for
   the full sequenced rollout. Phase 0.3 lives in there.
2. Read `docs/brainstorms/2026-04-07-cross-system-shell-requirements-brainstorm.md`
   for the per-system analysis (C64, NES, Amiga) that drove the design
   constraints. The trait must accommodate all four families, not just
   the Spectrum.
3. Read `crates/emu198x-shell/src/lib.rs` (~25 lines) — the empty
   scaffolding waiting for `System`.
4. Read `crates/runtime-sinclair-zx-spectrum/src/lib.rs` to understand
   the `Machine` enum's current shape — it's the first concrete System
   implementation.

## Goal of Phase 0.3

Define the `System` trait in `emu198x-shell` plus a `SpectrumSystem`
extension trait, then make `runtime-sinclair-zx-spectrum::Machine`
implement both. No frontend or capture work yet — just the trait shape
and the Spectrum binding so we know it fits one real system before we
generalise it for C64/NES/Amiga later.

## Decisions already made (in the brainstorm — do not relitigate)

- **Address space**: `u64` everywhere. Wide enough for Amiga's 24-bit
  bus and any future 32-bit system; cheap enough for 6502 systems.
- **Register access**: string-keyed (`fn read_register(&self, name: &str)
  -> Option<u64>`). Avoids per-system enum proliferation; the cost of
  string lookup is negligible compared to anything an introspector
  would do with the value.
- **Media kinds**: `Tape`, `Disk`, `Cartridge`, `Optical`, `Snapshot`.
  (`Optical` not `CdRom` — symmetric with the others, covers
  CD/DVD/whatever else.)
- **Speed control**: audio-preserving slow-motion. The trait needs a
  speed-multiplier hook, and the runtime is responsible for resampling
  audio so 0.5x sounds half-speed at the same pitch (or the same pitch
  if the user prefers).
- **Cycle accuracy**: full, foundational. Not a "fast mode". Every
  System impl is cycle-perfect.
- **Full peripheral fidelity**: full 1541 for C64, full mappers for
  NES, full OCS+ECS+AGA for Amiga. No "simple mode" escape hatches.
- **ROM provenance**: user-supplied; the shell doesn't ship ROMs.
- **Headless runner / capture / scripting / MCP**: deferred to Phases
  0.12 through 0.15. Phase 0.3 only needs to leave the trait shape
  *capable* of supporting them, not implement them.

## What Phase 0.3 must produce

Approximately:

- `emu198x_shell::system::System` trait with at least:
  - identity (`fn model_id(&self) -> &str`, `fn family(&self) -> Family`)
  - lifecycle (`fn reset(&mut self)`, `fn run_frame(&mut self)`)
  - timing (`fn frame_cycles(&self) -> u64`,
    `fn cycles_elapsed(&self) -> u64`)
  - memory (`fn read_byte(&self, addr: u64) -> u8`,
    `fn write_byte(&mut self, addr: u64, val: u8)`)
  - registers (string-keyed read/write)
  - framebuffer (`fn framebuffer(&self) -> &[u8]`,
    `fn framebuffer_size(&self) -> (u32, u32)`,
    `fn framebuffer_format(&self) -> FramebufferFormat`)
  - audio (`fn audio_samples_per_frame(&self) -> usize`,
    `fn end_audio_frame(&mut self, out: &mut [f32])`)
  - input (an opaque per-system handle? or a typed enum? — see open
    questions below)
  - media loading (`fn load_media(&mut self, kind: MediaKind, bytes:
    &[u8]) -> Result<String, String>`)
  - speed (`fn set_speed(&mut self, multiplier: f32)`)
- `Family` enum: `Spectrum`, `C64`, `Nes`, `Amiga` (extensible).
- `MediaKind` enum (already decided).
- `FramebufferFormat` enum: at least `Indexed8WithPalette`,
  `Rgba8`, room to grow.
- A `SpectrumSystem` extension trait in
  `runtime-sinclair-zx-spectrum` (not in shell) that adds Spectrum-only
  hooks: keyboard matrix access, Kempston access, tape control. The
  shell trait stays family-agnostic.
- `impl System for Machine` and `impl SpectrumSystem for Machine` in
  `runtime-sinclair-zx-spectrum`. This is mostly delegation to existing
  Machine methods.
- The emu binary keeps using `Machine` directly (no shell-level
  refactor in this phase). The point is to *prove the trait fits* the
  Spectrum, not to migrate consumers yet.

## Open questions to brainstorm before coding

1. **Input model**: typed enum (`enum InputState { Spectrum {
   keyboard: [u8; 8], kempston: u8 }, C64 { ... }, ... }`) or opaque
   per-system handles? The brainstorm leaned toward family-specific
   extension traits, but the headless runner in 0.12 will need to
   apply scripted input from outside the family-specific code. Worth
   resolving the layering before writing the trait.
2. **Memory access granularity**: byte-only or also word/dword? Z80
   and 6502 work in bytes, 68000 in words/longs. A byte-only API
   forces the 68000 frontend to assemble multi-byte reads externally
   from address bus events; a word-aware API muddies the trait for
   8-bit systems. Brainstorm worth having.
3. **Register access naming**: case-sensitive or normalised? "PC" vs
   "pc" vs "Pc" — pick one and document. Probably lowercase.
4. **Framebuffer ownership**: borrowed slice (`&[u8]`) or callback
   (`fn write_framebuffer(&self, out: &mut [u8])`)? Borrowed is
   simpler, but commits the System to keeping the framebuffer alive
   between frames. The Spectrum already does this. Cheap default.
5. **Timing**: should `frame_cycles` and `cycles_elapsed` even live
   on the trait, or are they capture/MCP-only? They're useful for
   capture pipelines and time-travel debugging. Probably keep them.
6. **Family enum or family marker trait**? If the shell needs to do
   `match family { Spectrum => ..., C64 => ... }` then enum. If only
   extension traits ever differ, marker trait. The headless runner
   probably needs to do the former. Enum.

These six are the right starting brainstorm. Use `/workflow:brainstorm`
or `AskUserQuestion` *before* writing trait code — the project rule is
"don't jump to code, especially on architectural work".

## What Phase 0.3 should NOT touch

- The `emu-sinclair-zx-spectrum` bin. It keeps using `Machine`
  directly. Migrating it is a separate phase if we ever need to.
- Headless runner, capture, scripting, MCP — all Phases 0.12-0.15.
- Any C64/NES/Amiga code. Phase 0.3 only validates the trait against
  the Spectrum.
- The wiki. Once Phase 0.3 lands, the System trait warrants a wiki
  page in `wiki/concepts/` — but write the trait first.

## Definition of done

- `emu198x-shell` exports `System`, `Family`, `MediaKind`,
  `FramebufferFormat`, and any supporting types.
- `runtime-sinclair-zx-spectrum::Machine` implements `System` and
  `SpectrumSystem`.
- A small unit test in `runtime-sinclair-zx-spectrum` that boots a
  Spectrum 48K through the `System` trait (not `Machine` directly)
  and ticks one frame, asserting framebuffer size and audio sample
  count.
- `cargo test --workspace --exclude emu-sinclair-zx-spectrum --lib
  --tests` is green.
- The bin is unchanged and still runs.

## Suggested commit boundaries

1. Trait definitions + supporting types in `emu198x-shell`. No impls.
2. `impl System for Machine` + `impl SpectrumSystem for Machine`
   + the unit test.

Two commits. Both small. Phase 0.3 should be a one-session phase if
the brainstorm is done first.

## Memory references

- `project_sdl3_gpu_pipeline.md` — irrelevant to 0.3 directly but
  the model for "save the gotchas of an architectural pass".
- `wiki/decisions/system-specific-run-loops.md` — already decided
  there's no universal tick pattern. The trait's `run_frame` is just a
  frame-boundary marker; what each system does inside is its business.
- `wiki/decisions/save-state-format.md` — serde derives are already on
  every chip and machine struct. The System trait does NOT need its
  own save-state mechanism; serialization is per-system via serde.
