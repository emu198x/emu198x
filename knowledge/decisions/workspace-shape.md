# Decision: Workspace shape — stay unified until concrete pain forces a split

**Date:** 2026-05-23
**Status:** Locked. Governs whether and when the 113-crate workspace
splits into multiple workspaces or repos as the project grows
through Waves 2 / 3 (per the
[post-October roadmap](../../docs/plans/2026-05-23-post-october-roadmap.md)).
Companion to [`versioning-strategy.md`](versioning-strategy.md) —
that record covers crate version management; this one covers
workspace organisation.

## What this is

The workspace organisation decision. Today's 113-crate workspace
keeps everything in one tree: one `Cargo.toml` `[workspace]` block,
one `target/`, one `Cargo.lock`, one CI pipeline, one
cargo-dist release pipeline, one release-plz config. The
post-October roadmap adds ~30 crates per wave (Wave 2: Atari 2600 /
BBC / MSX / SMS; Wave 3: another ~30+; by 2028 plausibly 200+).
This decision establishes when, if ever, that growth triggers a
split.

## The decision

**Stay unified. Do not pre-split. Re-evaluate only when concrete
pain materialises against the documented triggers below.**

The 113-crate workspace is large but not unreasonable
(rust-analyzer runs ~150+ crates; servo had thousands at peak;
tokio is ~30). Splitting now to anticipate growth is premature
optimisation that pays real costs (cross-crate refactoring becomes
multi-repo, shared dep pinning fragments, contributor onboarding
gets worse) for theoretical benefits.

## Triggers that would force a re-evaluation

If any of these become true, this decision gets re-opened:

1. **Full `cargo build --workspace` exceeds 10 minutes on a
   reference dev machine.** Today's compile time is the baseline
   the project's contributors have accepted; if it doubles or
   triples, the cost of unification has crossed the cost of
   splitting.
2. **`cargo metadata` or rust-analyzer chokes the IDE.** Concrete
   signal: hover info takes seconds, autocomplete lags, RA's
   memory footprint becomes uncomfortable. Re-evaluate at the
   point this is repeatedly noticeable.
3. **A library consumer surfaces.** Someone wants to depend on a
   specific crate (e.g., `emu198x-spectrum` as a library, or one
   of the system runtimes) without pulling in 100+ unrelated
   crates. If this happens, splitting may be the right response;
   but more likely the per-crate publish pattern from
   [`versioning-strategy.md`](versioning-strategy.md) is the
   correct answer.
4. **A maintainer carve-out becomes valuable.** Someone (or some
   group) wants to take over a system family (the Spectrum line,
   the Amiga family) without taking ownership of the whole
   workspace. Splitting per system family becomes the natural
   answer.
5. **A licensing change forces separation.** Hypothetical: if a
   specific crate needed to relicense for downstream
   compatibility, a split might be the cleanest path. Not
   imminent.
6. **CI compute cost crosses a budget.** GitHub Actions free-tier
   exhaustion or paid-tier cost outpacing project value would push
   toward splitting (independent CI per split keeps per-PR cost
   bounded).

None of these are true today. Until one is, stay unified.

## Why not pre-split now

### Comparable workspaces are larger and still work fine

- **rust-analyzer**: ~150+ crates in a single workspace
- **servo** (at peak): thousands of crates in one workspace
- **tokio**: ~30 crates
- **axum**: ~10 crates
- **bevy**: ~70 crates
- **nushell**: ~50 crates

The Rust ecosystem has converged on workspaces as the right shape
for related crates. 113 is on the larger end of normal but not
exceptional. 200 (post Wave 3) is still well within precedent.

### The real costs of splitting are high

A split workspace pays:

- **Multi-repo cross-crate refactoring.** Today, renaming a trait
  in `emu198x-shell` is one PR touching every consumer; split,
  it's N PRs across N repos coordinated to land in the right
  order. Architectural evolution becomes painful.
- **Fragmented dep pinning.** One `Cargo.lock` today guarantees
  every crate sees the same `wgpu`, `winit`, `serde`. Split,
  each workspace has its own lockfile and version skew becomes
  possible (and likely).
- **Fragmented CI.** N pipelines instead of one. Each needs its
  own caching, its own coverage gate, its own release config.
- **Worse contributor onboarding.** "Clone the project" becomes
  "clone these N projects." First-impression cost.
- **Loss of `cargo build --workspace` as a single command.**
  Replacing that ergonomic with multi-repo orchestration is
  almost always worse.

### The supposed benefits of splitting often don't materialise

- "Faster compile" — true on the changed sub-workspace; but most
  development touches the shared crates, so the per-change
  compile cost is similar either way.
- "Easier ownership" — only matters if there are multiple
  maintainers wanting carve-outs. Currently: one.
- "Independent versioning" — already solved at the crate level by
  [`versioning-strategy.md`](versioning-strategy.md) (carve-out
  per crate at publish time). The workspace shape doesn't gate
  this.
- "Cleaner public API" — the crate-level public API is what
  consumers see; the workspace shape is internal.

## Possible split lines (if it ever comes to that)

When/if a split becomes necessary, the candidate seams are:

- **(a) Per CPU family** — `emu198x-6502-ecosystem` repo (C64, NES,
  Atari 8-bit, BBC, Apple II), `emu198x-z80-ecosystem` (Spectrum,
  MSX, CPC, SMS, ColecoVision), `emu198x-68k-ecosystem` (Amiga,
  Atari ST, Mega Drive), `emu198x-6809-ecosystem` (Dragon, CoCo).
  Maps to the chip-reuse structure but mid-cuts most systems
  (C64 needs Z80 for the 1541 — wait, the 1541 is 6502; but the
  Amiga has Z80 audio cards, the Mega Drive has Z80 + 68k…). Real
  cross-family dependencies make this less clean than it sounds.
- **(b) Per system family** — `emu198x-spectrum-family`,
  `emu198x-amiga-family`, etc. Cleaner per-system but the shared
  `emu198x-shell` and chip crates need to live somewhere and
  every system-family repo depends on them.
- **(c) Per layer** — `emu198x-chips`, `emu198x-formats`,
  `emu198x-machines`, `emu198x-runtimes`, `emu198x-binaries`. Each
  layer depends only on layers below. Architecturally clean but
  cross-cutting work (adding a system means touching every layer)
  spans every repo.
- **(d) Per release tier** — `emu198x-published` (the stable
  external-quality library crates) + `emu198x` (everything else).
  Maps to the publish/no-publish split. Smaller surface to keep
  stable.
- **(e) Per accuracy tier** — `emu198x-anchors` (Spectrum / C64 /
  NES / Amiga / Game Boy / Dragon) + `emu198x-wave2` + `emu198x-
  longtail`. Releases at different cadences.

**My current lean if forced:** option (d) — published-vs-not.
Smallest churn on the existing structure; aligns with the
versioning-strategy carve-out pattern; lets the published library
crates have a polished public face without dragging the whole
workspace through every release cycle.

But this is speculation until a trigger fires. The right split
line will likely be obvious in hindsight once the actual pressure
materialises.

## Workarounds short of splitting

Compile-time pressure can be addressed without splitting:

- **`cargo build -p <crate>`** for per-crate incremental work
- **Workspace `default-members`** to limit what `cargo build`
  without `-p` builds by default (we already use this implicitly
  via the workspace member list)
- **Faster linker:** `mold` on Linux, `lld` on macOS / Windows. A
  significant win for the link stage of large workspaces.
- **`cargo-nextest`** for faster test runs
- **`incremental = true`** in the dev profile (Cargo default)
- **`sccache`** for cross-checkout caching
- **`[lib] doctest = false`** per crate where doctest builds add
  noise without value

These can extend the unified-workspace runway substantially before
any split becomes necessary.

## What we are NOT doing

- **Pre-splitting in anticipation of Wave 2/3 growth.** The growth
  is real but the pain isn't (yet). Splitting now pays cost for
  imaginary benefit.
- **Splitting per CPU family** as a default architectural shape.
  Cross-family deps (audio Z80 + main 68k in Amiga, etc.) make
  this less clean than it sounds.
- **Splitting "for cleanliness."** The workspace shape isn't the
  public API; the per-crate API is. Splitting for aesthetic reasons
  costs real engineering time.

## What changes downstream

**Nothing today.** The workspace stays at 113 crates and grows
linearly with each new system. Phase E and Phase F of the
post-October roadmap add ~30+ crates each; the workspace
absorbs them.

When (if) a trigger fires:

1. Document the specific trigger in this record's Log section
2. Brainstorm the split line that matches the trigger (probably
   option (d) per the current lean, but evaluate against the
   actual pain)
3. Plan the migration as a dated plan in `docs/plans/`
4. Execute as a series of PRs, not a big-bang split

## Drift triggers

If I'm about to suggest any of these, stop and re-read this record.

- **"The workspace is getting too big; we should split"** — by
  what concrete measure? Re-read § Triggers. Without a specific
  trigger having fired, the suggestion is premature.
- **"Let's pre-split the Wave 2 systems into their own workspace
  to keep this one clean"** — re-read § Why not pre-split. Wave
  2 systems benefit from sharing the workspace's chips, formats,
  and shell infrastructure.
- **"We should split per CPU family so it matches the chip reuse
  map"** — re-read § Possible split lines (a). The cross-family
  deps make this less clean than it sounds; (d) is currently the
  better candidate if forced.
- **"Compile time is getting noticeable"** — try the
  workarounds in § Workarounds short of splitting first.
  Splitting is the nuclear option; workarounds buy real runway.
- **"Splitting will help us keep the published crates stable"** —
  per-crate versioning + carve-out per publish already solves
  this (see [`versioning-strategy.md`](versioning-strategy.md)).
  Workspace shape isn't the right lever.

## Log

### 2026-05-23 — Decision locked

Brainstormed in-session as the workspace-growth-strategy question.
113 crates today is large but not unreasonable
(rust-analyzer ~150, servo had thousands). Wave 2 adds ~30; Wave 3
another ~30+. By 2028 plausibly 200+ crates — still within
ecosystem precedent.

Decision: stay unified until a concrete trigger fires. Document
the six triggers (compile time > 10min, IDE choke, library
consumer surfaces, maintainer carve-out needed, licensing-forced
separation, CI cost overrun). Document the four workarounds short
of splitting (per-crate builds, faster linker, nextest, sccache).
Document the candidate split lines if forced ((a) per CPU, (b)
per system family, (c) per layer, (d) per release tier, (e) per
accuracy tier) — (d) is the current lean.

No code changes today. The record exists so future "let's split"
suggestions get checked against the triggers rather than acted on
prematurely.
