# Decision: Crate Naming Convention

## Convention

| Category | Pattern | Example |
|----------|---------|---------|
| Chips | `{manufacturer}-{chipname}[-variant]` | `mos-6502`, `commodore-agnus-ocs` |
| Machine libraries | `machine-{manufacturer}-{system}` | `machine-commodore-amiga` |
| Runtime libraries | `runtime-{manufacturer}-{system}` | `runtime-sinclair-zx-spectrum` |
| Runner packages | `emu-{manufacturer}-{system}` | `emu-sinclair-zx-spectrum` |
| Formats | `format-{manufacturer}-{system}-{format}` | `format-sinclair-zx-spectrum-tap` |
| Peripherals | `peripheral-{manufacturer}-{system}-{peripheral}` | — |
| Multi-vendor standards | No manufacturer prefix | MSX crates |
| System-family common code | `{system}-common` | `common-sinclair-zx-spectrum` |
| Cross-project shell | `emu198x-{role}` | `emu198x-shell` |

## Format naming: always namespace by system

Every format crate includes the full system name, even when the format is currently unique to one system. Reasons:

- **TAP** exists for both Spectrum and C64 — completely different formats, same extension
- **DSK/EDSK** is shared between Spectrum +3 and Amstrad CPC — separate crates, each implements its system's quirks
- **TZX/CDT** are the same underlying format — but we still namespace (`format-sinclair-zx-spectrum-tzx`, `format-amstrad-cpc-cdt`) because each crate may need system-specific interpretation

The current `format-tap` and `format-tzx` need renaming to `format-sinclair-zx-spectrum-tap` and `format-sinclair-zx-spectrum-tzx` before Phase 2 (C64).

## Runtime libraries

Sit *above* the per-machine crates and *below* the runner bin. Hold the system-family `Machine` enum that wraps every variant, the snapshot loader, the audio mixer, the file router, and any other code that's shared across all variants of one system family but is not consumed by individual machine implementations.

Why this is its own category, separate from `{system}-common`: there's a layering constraint. `{system}-common` (e.g. `common-sinclair-zx-spectrum`) holds the chip-level primitives (`MemoryBus`, `Ula` trait, `BeeperAudio`, `FrameTiming`) and is *depended on* by every `machine-*` crate. A `Machine` enum that wraps every machine variant has to depend on every `machine-*` crate, which means it can't live in `common-sinclair-zx-spectrum` without creating a dependency cycle. The runtime layer is the natural home for this code.

Examples of what belongs here:

- The `Machine` enum and its `Model` enum (the user-facing variant identifier)
- Snapshot loaders (`.z80`, `.sna`) that route to the right machine variant
- File loaders (`.tap`, `.tzx`, `.trd`, `.dsk`, `.zip`) that route by extension
- Audio mixing math that combines beeper + AY into one stream (without any frontend audio output dependency)
- Anything that wraps multiple machine variants behind a uniform interface

Examples of what does *not* belong here:

- Anything SDL/GTK/SwiftUI specific — frontends are bins, not runtime libraries
- The `System` trait itself — that's cross-system (`emu198x-shell`)
- Per-machine details — those stay in their own `machine-*` crate

The runtime layer is per-system-family. Each family gets its own runtime crate: `runtime-sinclair-zx-spectrum`, `runtime-commodore-c64`, `runtime-nintendo-nes`, `runtime-commodore-amiga`. Each implements the cross-system `System` trait from `emu198x-shell`.

## Cross-project shell crates

The `emu198x-{role}` pattern is a new category for infrastructure that sits *above* any single system family. It exists because the [product roadmap](product-roadmap.md) commits to shared shell infrastructure (`System` trait, capture pipeline, MCP server, save state framework, launcher) that every system links against. That code isn't "common to Spectrum" — it's common to the whole project — so the `{system}-common` pattern doesn't fit.

Examples of what belongs here:

- `emu198x-shell` — the `System` trait, headless runner, capture pipeline, save state framework, speed control and audio time-stretching
- `emu198x-mcp` — MCP server exposing the `System` trait as agent-callable tools. Separate crate so agents can import the MCP surface without pulling in the capture-pipeline DSP dependencies.
- `emu198x-launcher` — unified system picker (future, for the post-October unified launcher)

Code that is specific to one system family still goes in `{system}-common` (e.g. `common-sinclair-zx-spectrum` for Spectrum-family Machine enum, snapshot loader, audio mixer).

## Why

Consistent naming makes the crate list self-documenting. The manufacturer prefix disambiguates chips with similar names (Motorola 6809 vs Hitachi 6309) and groups related crates in sorted listings.

## Drift triggers

Naming is where conventions decay first. If I'm about to propose or create any of these, stop and re-read the convention table above.

**Bad names to reject:**

- Short / unprefixed names: `emu-spectrum`, `chip-z80`, `chip-6502`, `core`, `utils`
- Un-namespaced formats: `format-tap`, `format-tzx`, `format-dsk`, `format-sna` — every format crate must include the full system name even if currently unique
- Ambiguous chip names missing the manufacturer prefix
- Generic dumping grounds: `shared`, `helpers`, `misc`, `util`
- `common` without a system prefix (should be `common-sinclair-zx-spectrum`, etc.)
- Cross-project infrastructure named `common-*` (should be `emu198x-*`)

**Phrases that signal drift:**

- "Let's just call it the short name, it's clearer"
- "We don't need the system prefix since it's only used by Spectrum right now"
- "`common` is fine, we'll rename it later when we add another system"
- "A `utils` crate would be convenient"
- "No manufacturer prefix, everyone knows what 6502 means"
- "Let me put this in `emu198x-shell` since it's kind of shared" (no — `emu198x-*` is for cross-project infrastructure, not Spectrum-specific code)

**Why this matters:** the existing `format-tap` and `format-tzx` are explicitly called out in this document as needing renaming to `format-sinclair-zx-spectrum-tap` / `format-sinclair-zx-spectrum-tzx` before Phase 2 (C64). TAP exists on both Spectrum and C64 as completely different formats. If I'm suggesting a short name "for convenience," I'm proposing to repeat a known mistake that already has a cleanup task on the roadmap.
