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

## Amendment 2026-08-26: published crates carry the `emu198x-` prefix

**The table above governs crates that stay in this workspace. A crate that is
published takes `emu198x-` in front of whatever that table gives it.**

`mos-sid-6581` on disk becomes `emu198x-mos-sid-6581` on crates.io. The prefix
is added and nothing is dropped, so the category word survives:
`format-commodore-amiga-adf` would publish as
`emu198x-format-commodore-amiga-adf`, not `emu198x-commodore-amiga-adf` —
this repo sorts crates by category, and dropping the word would collide a
format crate with a machine crate for the same system.

This implements [`198x/decisions/crate-naming.md`](../../../../decisions/crate-naming.md)
at the moment it starts to bind. A registry entry has no folder to sit in, so
the name is the only place provenance can live; on disk the path already says
it. That is why the rule applies at publication and not before, and why the
other ~214 crates here keep their unprefixed names.

### What is published, and why only these

Six leaf crates, renamed and published 2026-08-26:

| Crate | Why |
|---|---|
| `emu198x-mos-6502` | `.sid` playback needs a 6502 — the tune *is* a program |
| `emu198x-mos-sid-6581` | the chip that program drives |
| `emu198x-zilog-z80` | the same shape for `.ay` |
| `emu198x-gi-ay-3-8910` | the chip that program drives |
| `emu198x-ricoh-apu-2a03` | NSF, the same shape again |
| `emu198x-commodore-paula-8364` | Amiga audio |

All six are true leaves — one dependency each, `serde`, and none of it
internal — so publishing them drags nothing else onto the registry. That was
the selection criterion, not importance:
[emu198x#1214](https://github.com/emu198x/emu198x/issues/1214) needs Play198x
to consume a chip and a CPU, and these are the pairs that reach it without
committing a permanent name for any `common-`, `format-` or `peripheral-`
crate.

Publishing the remaining 56 chip crates would drag in 9 of those, one of
which is `format-commodore-amiga-adf` — and the family already publishes
`format198x-commodore-amiga-adf`. Two ADF crates from one family, on a
registry that never releases a name, is a decision that has not been taken.
It is the gate on widening this scope.

### Independently versioned

These six drop `version.workspace = true` and carry their own versions,
starting at the `0.6.0` they already had. A published crate's version tells
consumers when *it* changed; tying it to the suite would bump all six on
every Emu198x release whether or not a line moved. This follows
`format198x-commodore-amiga-adf`, whose manifest makes the same argument.

It also means the package-release machinery removed from
`maintain-release.yml` — release-plz, which reasons in packages against this
repo's one-suite-one-version tag — is not reintroduced by this change.
Publishing these six is a `cargo publish` per crate, not a release model.

### Drift triggers

- **"Rename the rest to match"** — no. The prefix binds at publication.
  Renaming an unpublished crate buys nothing and breaks in-repo paths.
- **"Add a `[lib] name` alias so `use mos_6502::` keeps working"** — no.
  Format198x renamed all the way through and its consumers import
  `format198x_commodore_amiga_adf`; an alias would hide the real name from
  exactly the reader the rename is for.
- **"Publish a chip crate that has internal dependencies"** — not without
  deciding what happens to those dependencies' names first, and not while the
  ADF duplication above is open.

## Format naming: always namespace by system

Every format crate includes the full system name, even when the format is currently unique to one system. Reasons:

- **TAP** exists for both Spectrum and C64 — completely different formats, same extension
- **DSK/EDSK** is shared between Spectrum +3 and Amstrad CPC — separate crates, each implements its system's quirks
- **TZX/CDT** are the same underlying format — but we still namespace (`format-sinclair-zx-spectrum-tzx`, `format-amstrad-cpc-cdt`) because each crate may need system-specific interpretation

The current `format-tap` and `format-tzx` need renaming to `format-sinclair-zx-spectrum-tap` and `format-sinclair-zx-spectrum-tzx` before Phase 2 (C64).

### Amendment 2026-08-14: parsing may be shared beneath the namespaced crates

**The namespaced crates stay. What moves is what sits under them.**

When a format genuinely is one format across systems, the *byte-level block
parsing* may live in an unnamespaced crate that the per-system crates depend
on. The per-system crate remains the public entry point and owns the
interpretation.

This came up building the Amstrad CPC's tape support. CDT is TZX — not a
lookalike with the same extension, the same format — and the Spectrum's
parser is 987 lines of block decoding with nothing Spectrum-specific in it.
Duplicating that into `format-amstrad-cpc-cdt` would put two copies of the
same block table in the tree and let them drift, which is what
[RULES.md](../../RULES.md) rule 30 exists to prevent.

The original reasoning — "each crate may need system-specific interpretation"
— was right, and the CPC proves it rather than contradicting it. CDT pulse
lengths are expressed in the Spectrum's 3.5 MHz T-states, so the CPC scales
them by 40/35 to its own 4 MHz clock (Caprice32's `CYCLE_SCALE`). That scale
is exactly the "system-specific interpretation" the decision anticipated. It
is also about ten lines, sitting on top of a parser that is identical for both.

So the split is:

| Layer | Crate | Holds |
|---|---|---|
| Block parsing | `format-tzx` | The format's block table. No system knowledge. |
| Interpretation | `format-sinclair-zx-spectrum-tzx`, `format-amstrad-cpc-cdt` | Clock scaling, system quirks, the public API each machine uses. |

**This does not reinstate the pre-Phase-2 naming.** `format-tap` and
`format-tzx` were renamed because *TAP* means two unrelated formats on
Spectrum and C64, and an unnamespaced `format-tap` was a genuine ambiguity.
A shared crate is admissible only where the format really is one format; the
test is whether a single parser can serve both systems without conditionals
on which system is asking. TAP fails that test and must stay two crates. TZX
passes it.

**Drift trigger.** If a shared parsing crate starts growing
`if system == …` branches, the sharing was wrong. Split it and take the
duplication.

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

- Short / unprefixed names: `emu-spectrum`, `emu-c64`, `chip-z80`, `chip-6502`, `core`, `utils`
- Un-namespaced formats as a *system's entry point*: `format-tap`, `format-dsk`,
  `format-sna` — the crate a machine depends on must include the full system
  name even if the format is currently unique to one system. The 2026-08-14
  amendment permits an unnamespaced crate **beneath** those, holding block
  parsing only, and only where one parser serves every system without
  branching on which is asking. `format-tzx` is such a crate. A machine
  reaching past its own format crate to depend on the shared one directly is
  the drift to catch.
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

**Why this matters:** `format-tap` and `format-tzx` were both once unnamespaced and were renamed before Phase 2 (C64). TAP exists on both Spectrum and C64 as completely different formats, so that rename was correcting a real ambiguity and still stands. If I'm suggesting a short name "for convenience," I'm proposing to repeat a known mistake that already has a cleanup task on the roadmap.

The 2026-08-14 amendment is not a licence to undo it. It permits exactly one
thing: a shared *parsing* crate under the namespaced ones, where a single
parser serves every system without conditionals. "For convenience" is not
that test, and TAP still fails it.
