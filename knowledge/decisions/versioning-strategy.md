# Decision: Versioning strategy — lockstep by default, carve out at publish time

**Date:** 2026-05-23
**Status:** Locked. Governs Cargo.toml `version` declarations across
the workspace, release-plz behaviour, cargo-dist tag patterns, and
the path to crates.io publishing referenced by
[`../../../docs/plans/2026-05-23-post-october-roadmap.md`](../../../docs/plans/2026-05-23-post-october-roadmap.md)
§ Phase G.

## What this is

The workspace versioning shape. Every crate today inherits version
from `[workspace.package]` via `version.workspace = true` — lockstep.
The roadmap assumes ~6–10 chip and format crates eventually publish
to crates.io, and per-crate versioning is the natural shape for
published library crates. This decision records when and how the
switch happens.

## The decisions

1. **Lockstep is the default.** Every workspace crate inherits
   version from `[workspace.package]` via `version.workspace = true`
   unless and until that crate publishes to crates.io.
2. **Carve out per crate at publish time, not before.** The first
   time a chip or format crate publishes to crates.io, the carve-out
   happens in the same PR: remove `version.workspace = true`, add a
   concrete `version = "X.Y.Z"`, add a per-package release-plz
   override. Subsequent publishes follow the same pattern, one
   crate at a time.
3. **The six native shells stay lockstep with each other and with
   the workspace version, forever.** `emu198x-spectrum`,
   `emu198x-c64`, `emu198x-amiga`, `emu198x-dragon`,
   `emu198x-game-boy`, `emu198x-nes` are user-facing binaries that
   ship together as "Emu198x v0.3.0." They never carve out, even
   if some of them see more activity than others.
4. **No big-bang migration of the 113 crates.** A bulk switch to
   independent versioning is explicitly rejected. The cost is real
   (113 Cargo.toml edits, release-plz config rewrite, cargo-dist
   tag-pattern rewrite, ~113 CHANGELOG files in the tree, plus the
   ongoing cognitive load of 113 version numbers) and the benefit
   is zero for the ~100 crates that are internal implementation
   details with no external audience.
5. **cargo-dist's tag pattern stays `v*` matching the workspace
   version.** Per-crate tags from carved-out crates (e.g.,
   `mos-6502-v0.5.0`) coexist; cargo-dist's existing pattern
   ignores them.

## Why

### Most crates will never publish

Of the 113 workspace crates, perhaps 6–10 have an external audience:
the CPU cores (`mos-6502`, `zilog-z80`, `motorola-6809`,
`motorola-68000`), the larger format parsers (Spectrum TAP / TZX /
SNA / Z80, NES iNES, Amiga ADF), and a handful of chip
implementations that get reused (`gi-ay-3-8912`, `mos-sid-6581`,
`commodore-paula-8364`). The other ~100 crates are machine wirings,
common-* shared substrate, runtime glue, ULA/PIA/DMA implementations
specific to one system family — implementation details with no
audience outside this workspace.

Forcing those 100+ crates into per-crate versioning serves no one
and costs maintenance attention forever.

### The six native shells genuinely belong in lockstep

A user installing Emu198x has a single mental model: "I have
Emu198x version X." When `emu198x-spectrum` is at 0.3.4 and
`emu198x-c64` is at 0.1.2 because the Spectrum saw more commits,
that mental model breaks. "Which Emu198x do you have?" becomes "uh,
which binary?"

Independent versioning of the shells optimises for the wrong
audience — the shell-publish cadence is the right granularity for
some library crate consumers but is hostile to the human user who
just wants to know what version they're running.

The workspace.package.version IS the "Emu198x version." The native
shells track it. Carved-out chip crates track their own versions
because their audience is different.

### Lockstep cost is small until the moment of publish

Internal dependencies are all path-based (`{ path = "../foo" }`)
and ignore version constraints. The lockstep "everything bumps
together" pattern is invisible inside the workspace — the only
thing that bumps is the GitHub Release tag and the CHANGELOG entry.

The cost only materialises when a consumer of a *published* crate
pulls `mos-6502 = "0.5"` from crates.io and sees a needless 0.5.1
bump caused by an Amiga floppy fix. That's the moment per-crate
versioning starts paying back. Before that moment, lockstep costs
nothing visible.

### Iterative carve-out matches iterative publish cadence

The crates that eventually publish probably don't all ship on the
same day. `mos-6502` likely goes first (highest external value;
depends-by-everyone). `zilog-z80` and `motorola-68000` follow.
Format parsers later. Each publish is a discrete event that can
carry its own carve-out.

Big-bang migration assumes a coordinated "publish day" that
doesn't match how the project will actually ship. Iterative
carve-out matches reality: pay the cost per crate, when each crate
needs it.

### release-plz supports per-package overrides

The mechanism is in place. release-plz's `[[package]]` blocks in
`release-plz.toml` (or `[package.metadata.release-plz]` in each
crate) let individual crates opt into independent versioning,
independent CHANGELOG files, and independent publish actions while
the rest of the workspace stays lockstep. No tool change needed
when the first carve-out happens; just configuration.

## Alternatives considered

- **(a) Stay lockstep forever, never publish to crates.io.** Lowest
  cost path. Rejected because Phase G of the post-October roadmap
  assumes publishing (the chip libraries have real external value;
  not publishing them is leaving cross-ecosystem leverage on the
  table).
- **(b) Switch everything to per-crate now.** 113 Cargo.toml edits,
  release-plz config rewrite, cargo-dist tag pattern rewrite,
  ~113 CHANGELOG files. Costs months of attention for zero benefit
  to the ~100 unpublished crates. Rejected.
- **(c) Switch the six native shells to independent versioning.**
  Breaks the "Emu198x v0.3.0" user mental model. Rejected.
- **(d) Hybrid from the start: shells lockstep, chips independent.**
  The correct *shape* but commits the carve-out work for all chip
  crates up front, including the ones that will never publish.
  Rejected in favour of (e).
- **(e) Iterative carve-out at publish time.** **Chosen.** Matches
  the actual cost shape: pay per crate, when each crate publishes.

## What we are NOT doing

- **Carving out crates that don't publish.** A crate stays
  `version.workspace = true` unless and until it ships to
  crates.io. "Consistency" is not a reason to carve out.
- **Versioning the six native shells independently of each other.**
  They are the user-facing product; they ship together; their
  version IS the Emu198x version.
- **Forcing internal dependency versions** (`mos-6502 = "0.4"` in
  Cargo.toml dep blocks). Internal deps stay path-based until and
  unless a real cross-version compatibility concern emerges.
- **Migrating existing v0.x.x tags.** Today's tags are valid; new
  per-crate tags (e.g., `mos-6502-v0.5.0`) coexist with them.
- **Splitting CHANGELOG.md per crate** until that crate carves out.
  Lockstep crates share the workspace CHANGELOG; carved-out crates
  get their own under `crates/<name>/CHANGELOG.md`.

## What changes downstream

**Nothing today.** The workspace stays as it is. The decision shapes
future PRs, not current state.

When the first chip crate is ready to publish (probably `mos-6502`),
the carve-out PR does five things:

1. `crates/mos-6502/Cargo.toml`: remove `version.workspace = true`,
   add `version = "0.1.0"` (or whatever first-published version is
   appropriate)
2. `release-plz.toml`: add a `[[package]]` block for `mos-6502`
   with `publish = true` and any per-package overrides
3. `crates/mos-6502/README.md` and `crates/mos-6502/CHANGELOG.md`:
   create per-crate documentation appropriate for crates.io
4. `Cargo.toml` per-package metadata: ensure `description`,
   `keywords`, `categories` are set (these can stay workspace-wide
   if they apply, or per-crate if more specific)
5. `cargo publish -p mos-6502` (or `release-plz publish` once the
   workspace is wired up for it)

The carve-out becomes the template. Subsequent chip crates that
publish follow the same five-step pattern.

## Open questions deferred

- **First crate to publish.** Probably `mos-6502` (highest external
  audience; depended on by every 6502-family system in the wider
  Rust emulator ecosystem). Not deciding now; the choice happens
  when the first carve-out PR is opened.
- **Meta-crate strategy.** Whether to ship an `emu198x-chips`
  meta-crate that re-exports the published chip crates as a
  convenience for consumers, or expect consumers to pull crates
  individually. Defer; depends on adoption patterns we don't have
  yet.
- **Versioning of `emu198x-shell` and `emu198x-native-video`.**
  These are workspace-internal infrastructure that the six native
  shells depend on. They probably stay lockstep with the workspace
  forever (they're not external-audience crates), but if a third
  party wanted to build an Emu198x-style emulator on top, the
  shell crate becomes external-audience. Cross this bridge when /
  if it arrives.

## Drift triggers

If I'm about to suggest any of these, stop and re-read this record.

- **"Let's switch everything to independent versions"** — premature
  by design; do per-crate at publish time. Re-read § Alternatives
  considered (b) and (d).
- **"This internal crate needs its own version for consistency"** —
  consistency is not a reason. Only crates that ship to crates.io
  carve out. Internal crates stay lockstep.
- **"The native shells should diverge so the busiest one isn't
  held back by the quietest one"** — wrong; the unified
  "Emu198x v0.3.0" version is a real user concept. Holding the
  busy shell back is the *correct* behaviour; carving the busy
  shell out breaks the product mental model.
- **"We need to migrate the existing v0.x.x tags before
  publishing"** — wrong; current tags stay valid. New per-crate
  tags coexist.
- **"We should bulk-add per-crate versions to all chip crates so
  they're ready to publish"** — bulk-readying is the same cost as
  bulk-migration. Carve out per crate, at the moment of publish.
- **"Internal deps should use real version constraints
  (`mos-6502 = "0.4"`) instead of `path = "../"`"** — adds
  cascade-bump pain across the workspace for no real benefit while
  everything ships together. Path deps stay default.

## Log

### 2026-05-23 — Decision locked

Brainstormed in-session with Steve as the answer to "when do we
switch from lockstep to per-crate versioning?" The answer landed
on **never, for most crates** — carve out per crate at the moment
each one publishes to crates.io, and never for the six native
shells. Steve confirmed the recommendation and asked for it
written down.

No code or config changes today; this record is the binding shape
that future carve-out PRs cite.
