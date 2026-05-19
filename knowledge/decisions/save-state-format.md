# Decision: Save State Format

**Date:** April 2026

## The decision

serde with **postcard** as the wire format. Derive `Serialize`/`Deserialize` on every chip struct and machine struct from day one.

### Why postcard over bincode

Decided 2026-04-09 during Phase 1.1 brainstorm. Three reasons:

1. **Size matters more than speed.** A 128K snapshot is ~160KB of RAM plus CPU/ULA state. Postcard's varint encoding shaves bytes, which matters for F1–F9 quick-save responsiveness and for the future rewind ring buffer.
2. **`no_std`-friendly.** Keeps the door open for embedding the core in wasm or a handheld port. Postcard supports this today; bincode 2.x is still catching up.
3. **Stable wire format.** Postcard promises wire-format stability. Bincode 1→2 is mid-migration with incompatible wire formats — exactly the churn we don't want under a file format we've promised to keep loading forever.

The header stays `{ magic: "EMU1", version: 1, model, timestamp }`. The payload is versioned separately, so if postcard ever bites us we bump `version: 2` and keep reading v1.

## Why serde

- **Automatic schema evolution** — adding a field with `#[serde(default)]` doesn't break old saves
- **Compact binary** — postcard produces small blobs, comparable to raw memcpy
- **35+ systems** — deriving Serialize is dramatically less work than writing custom binary serialisation for each system
- **Fast enough for rewind** — the ring buffer (save every N frames) needs fast serialisation. postcard serialises a Spectrum snapshot (~50KB of state) in microseconds.

## Why not raw memcpy

Faster, but fragile. Any struct layout change breaks all existing saves. With 35+ systems and ongoing development, that's unsustainable. The serde overhead is negligible compared to the maintenance cost of manual binary formats.

## On-disk layout

Saves live at `~/.emu198x/saves/<family>/<name>.state`, where `<family>` is the lowercased `Family` enum discriminant (`spectrum`, `c64`, `nes`, `amiga`). The directory is *family-scoped*, not model-scoped — a 48K save and a +2A save share `saves/spectrum/`.

A small helper on `Family` (e.g. `fn dir_name(self) -> &'static str`) is the single authoritative place that names directories, so aliases or renames never leak across the codebase.

### Why family, not model

1. **Matches how users think about saves.** A user saved a game "on the Spectrum," not "on the specific ROM revision that happened to be loaded." Pushing a quick-save key should find *your* save, regardless of which model the emulator is currently running.
2. **Cross-family collisions don't exist.** A C64 save and a Spectrum save have no reason to share a namespace; per-family directories are naturally distinct.
3. **The rejection check runs anyway.** We always compare the header's `model_id` against `system.model_id()` on load, regardless of directory layout, so per-model directories would buy nothing beyond awkward navigation.

### Edge case: Timex variants

TC2048 and TS2068 both live in `Family::Spectrum` but are genuinely different machines (TS2068 is NTSC, different ROM, Sound Chip, extended video modes). A save made on TC2048 gets rejected on TS2068 by the `model_id` check — they share `saves/spectrum/` but can never clash in practice. This is deliberate.

## Model match is strict and permanent

Decided 2026-04-09 during Phase 1.1 brainstorm.

**The rule:** `header.model_id` must equal `system.model_id()` exactly. No relaxation, now or ever. This applies across *all* apparent near-matches — 48K↔128K, 128K↔+2, +2A↔+3 — every pair is a hard reject.

**Why permanent, not "v1 strict, maybe relax later":**

- Any relaxation requires a compatibility matrix that grows combinatorially with each new model.
- Once a pair is declared compatible, tightening it later *does* break users — the relaxation is effectively load-bearing from day one.
- The user-visible cost of strictness is tiny (press a different slot, or relaunch with `--model X`). The cost of silent corruption from a mismatched load is enormous.
- "Use the launcher to pick the right model" is the right long-term answer; the launcher (Phase 4) will know which saves match which models and surface them accordingly.

### Error surface

Phase 1.1 defines a typed `LoadError`:

```rust
pub enum LoadError {
    Io(std::io::Error),
    BadMagic,
    UnsupportedVersion(u32),
    ModelMismatch {
        save_model: String,     // header.model_id
        current_model: String,  // system.model_id()
    },
    Decode(postcard::Error),
}
```

The `ModelMismatch` variant carries both IDs so:
- The CLI shell can print a specific message (`"Slot saved on sinclair-zx-spectrum-48k. Running sinclair-zx-spectrum-128k. Load cancelled."`) to stderr.
- The MCP server can return the enum fields directly — agents branch on `save_model` to decide what to do.
- The future launcher can filter the save list by currently-running model without re-parsing headers.

Never auto-switch models and never prompt. Refuse, explain, keep running. The running machine is more valuable than any save-load convenience.

## Current status

| Component | Serde derives | Notes |
|-----------|--------------|-------|
| ULA implementations | Yes | Ferranti, Sinclair, Amstrad, Pentagon, Scorpion, Timex SCLD |
| AY-3-8912 | Yes | |
| NEC µPD765A | Yes | |
| Common structs | Yes | Registers, BeeperAudio, TapePlayer |
| Z80 CPU | Yes | commit `5489bb4` — Phase 0.1. Walker's `sequence: &'static [MStep]` is `#[serde(skip)]` defaulting to `SEQ_NOP`; save states must be taken at instruction boundaries (`walker.instruction_complete == true`) to round-trip cleanly. |
| Machine wrappers | Yes | commit `5489bb4` — all seven: Spectrum48K, Spectrum128K, SpectrumPlus, Pentagon128, ScorpionZS256, TimexTC2048, TimexTS2068 |

Every prerequisite is now in place — the save state format work in Phase 1.1 can start without any more derive-plumbing work.

## Rewind implications

Rewind = ring buffer of serialised snapshots every N frames + replay-forward to target frame. Snapshot size varies: ~50KB (Spectrum) to several MB (Amiga with chip+fast RAM). bincode's speed makes this practical.

## Drift triggers

Save state format drift usually comes dressed as a performance optimization. If I'm about to propose any of these, stop and re-read the "Why not raw memcpy" section.

**Code patterns to reject:**

- `unsafe { std::mem::transmute(self) }` on a chip struct — raw memcpy is explicitly ruled out
- Hand-rolled `fn to_bytes(&self) -> Vec<u8>` / `from_bytes` on chip structs
- `#[repr(C)]` followed by byte-level snapshotting
- A new chip struct without `#[derive(Serialize, Deserialize)]`
- `use serde_json` for save states (too slow, too large)
- Skipping serde derives on "perf-sensitive" or "internal" structs

**Phrases that signal drift:**

- "Raw memcpy would be faster for the rewind ring buffer"
- "Let's write a custom binary format for this one"
- "JSON for debuggability"
- "We don't need serde on this struct, it's internal"
- "serde overhead is too high for per-frame snapshots"
- "I'll add serde derives later once the struct stabilizes"

**What to do when triggered:** bincode/postcard serialises a Spectrum snapshot (~50KB) in microseconds — that's already fast enough for rewind. The serde overhead is negligible compared to the maintenance cost of manual binary formats across 35+ systems. Any new chip struct must get `#[derive(Serialize, Deserialize)]` from day one. If I'm proposing to skip it, I'm proposing to pay the maintenance cost 35 times.

**Known work still to do:** the Z80 CPU and machine wrappers (Spectrum48K, Spectrum128K, etc.) still need serde derives added — see the Current status table above. If I'm editing Z80 or machine struct code and drafting new fields, I should check whether the derives are in place and flag it if not.

## Related

- [Product roadmap](product-roadmap.md) — serialisation as Phase 1 must-have
