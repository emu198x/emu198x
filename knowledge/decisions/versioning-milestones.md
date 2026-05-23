# Decision: Binary versions stay 0.x; library 1.0 is per-crate

**Date:** 2026-05-23
**Status:** Locked. Settles the 1.0 milestone question raised
during the post-October roadmap brainstorm. Sits alongside
[`versioning-strategy.md`](versioning-strategy.md) — that record
covers *how* version numbers are managed (lockstep default, carve
out per crate at publish); this one covers *when* 1.0 happens.

## What this is

The 1.0 milestone strategy. Per-system binary releases
(`emu198x-spectrum`, `emu198x-c64`, …) stay at 0.x indefinitely,
driven by the existing release-plz minor/patch cadence on the
workspace version. Library crates that carve out to publish on
crates.io hit their own 1.0 milestones independently, based on
public-API stability rather than calendar pressure.

There is no unified "Emu198x 1.0" event.

## The decisions

1. **`workspace.package.version` stays 0.x indefinitely.** No 0.99
   → 1.0 ceremony for the binary releases. The `v0.X.Y` cargo-dist
   tag pattern continues forever, or until a concrete reason to
   bump major emerges (in which case it's its own decision).
2. **Each carved-out library crate hits 1.0 on its own schedule**
   per [`versioning-strategy.md`](versioning-strategy.md). The bar:
   the public API has been used by at least 2 in-workspace consumer
   crates AND at least 1 external consumer (or 90 days post-publish
   with no breaking change need surfacing). Either condition
   triggers eligibility for 1.0; the call is made by the crate
   owner / maintainer at the time.
3. **"Production-ready" is signalled by the per-system catalogue
   passing**, not by the version number. The README compatibility
   matrix per system × media kind (a Phase A roadmap deliverable)
   is the production-ready signal users actually look at.
4. **No marketing milestones tied to a binary 1.0.** Crash! Live,
   Code198x Spectrum launch, "we shipped Atari 2600," "we hit four
   anchors at engineering bar" — these are the meaningful project
   events. They are decoupled from version arithmetic; none of
   them require a 0.x → 1.0 ceremony to be the moment they are.

## Why

### Binaries don't gain much from 1.0

The "is this production-ready?" question users actually care about
is "does my favourite Spectrum title load and play correctly?"
That's answered by the per-system catalogue passing — which is the
SOLID criterion shape for Spectrum and the equivalent for the
other anchors — not by a version label.

A 1.0 binary version that's actually buggy is worse than a 0.x
binary version that works well. The version number is a less
informative signal than the compatibility matrix.

### Libraries do gain from 1.0

A `mos-6502 = "1.0"` declaration on crates.io tells consumers
"this API is stable; you can depend on it without fear of
churn." That's a real commitment with downstream value to the wider
Rust emulator ecosystem. The chip crates (`mos-6502`, `zilog-z80`,
`motorola-68000`, `motorola-6809`) are the natural 1.0 candidates
because their public API surface is small, well-defined, and
already proven by use across multiple in-workspace consumers.

### The two cadences are different

`mos-6502` is plausibly API-stable in late 2026 — the trait
surface is already proven by NES + C64 use. A binary "all six
systems at SOLID-equivalent" milestone is 12+ months out per the
post-October roadmap. Forcing the library to wait for the binary
milestone delays the library commitment by a year for no benefit.

### Matches the versioning-strategy decision

[`versioning-strategy.md`](versioning-strategy.md) already
established that carved-out library crates version independently
from the workspace. Independent 1.0 milestones per published crate
is the natural extension of that strategy. The two records compose
cleanly.

### Steve's stated focus

The brainstorm that produced this decision included Steve's
explicit framing: "I'm focused on doing the work, not marketing
it." A unified 1.0 milestone is mostly a marketing artifact for
this kind of project. The decision aligns with the stated
priority: ship work, let version numbers describe what shipped.

## What we are NOT doing

- **Declaring a unified Emu198x 1.0** at any particular milestone
  (Spectrum SOLID, four anchors, six systems, first Wave 2, …).
  None of those events triggers a binary 1.0.
- **Holding library crates back from 1.0** until binaries reach
  some bar. `mos-6502` can hit 1.0 while `emu198x-spectrum` is
  at 0.3.4. The two are not coupled.
- **Versioning binaries by date** (MAME-style `2027.06`). The
  semver `0.X.Y` shape continues; only the major-bump-to-1.0
  event is rejected. release-plz keeps doing what it's doing.
- **Adding a "stability" marker** elsewhere (a STABILITY.md file,
  a banner on the README). The compatibility matrix is the
  stability signal.

## Alternatives considered

- **(a) Unified 1.0 at Spectrum SOLID.** Premature; declares
  stability over one system. Rejected.
- **(b) Unified 1.0 at four anchors at engineering bar.** More
  substantive but ignores Game Boy + Dragon, which are in the
  README's current focus list. Users would rightly say "you said
  this was 1.0 but Game Boy is half-done." Rejected.
- **(c) Unified 1.0 at six current README systems at
  SOLID-equivalent.** Honest about what's shipped. Real candidate;
  the fallback if option 6 (this decision) ever gets revisited.
  Currently rejected because the user prefers no unified
  milestone.
- **(d) Unified 1.0 at first Wave 2 system shipping.** Proves the
  extension path but doesn't add much production-readiness signal
  beyond what the catalogue passing already provides. Rejected.
- **(e) Never declare 1.0 at any level.** Includes libraries.
  Costs the real value libraries gain from a 1.0 commitment.
  Rejected.
- **(f) Decoupled: binaries stay 0.x; libraries hit 1.0 per crate.**
  **Chosen.** Matches the asymmetry between binary and library
  audiences. Matches the versioning-strategy decision. Aligns
  with Steve's "focused on work" priority.

## What changes downstream

1. **`workspace.package.version` continues to be bumped by
   release-plz on `feat` / `fix` commits as normal.** No special
   handling near 0.99. If we hit 0.99.x and the next bump would be
   1.0.0, release-plz follows conventional commits semantics —
   `feat:` at 0.99.0 bumps to 0.100.0 (not 1.0.0). semver pre-1.0
   stays semver pre-1.0.
2. **README gets an "About versioning" section** that explains the
   convention: binaries are 0.x by design; individual published
   library crates have their own version trajectories. Brief —
   one paragraph.
3. **The post-October roadmap's open question on 1.0 trigger
   criteria** is resolved by reference to this record.
4. **The first carved-out library crate's publish PR** does NOT
   need to decide library 1.0 timing — it can publish as 0.x
   like the rest. 1.0 for that crate is a later, separate
   decision when API stability is proven.
5. **No CHANGELOG.md ceremony at any milestone.** release-plz
   continues to write entries; users read them. No "1.0 release
   notes" event because there is no 1.0.

## Drift triggers

If I'm about to suggest any of these, stop and re-read this record.

- **"We should declare 1.0 for the marketing moment"** — Steve's
  current framing is "focused on the work, not marketing it."
  If/when marketing-driven 1.0 becomes a real need, this decision
  gets amended; until then, no.
- **"All binaries are at engineering bar now, time for 1.0"** —
  wrong reasoning; engineering bar IS the production signal.
  Adding a 1.0 label on top doesn't add information users care
  about.
- **"Consumers will expect a 1.0"** — for binary consumers (people
  running `emu198x-spectrum`) they don't; the version label is
  background noise. For library consumers (people depending on
  `mos-6502` from crates.io) they do; that's handled per-crate.
- **"Let's version binaries by date (2027.06) instead of semver"**
  — MAME does this and it works for MAME, but the project already
  uses semver successfully via release-plz, and the cargo-dist
  release pipeline is wired up for `v*` tags matching semver.
  Switching costs more than it gains. Rejected; not in scope.
- **"We need to coordinate the library 1.0 milestones so they
  ship together"** — wrong; library 1.0 is per-crate by API
  stability, not by coordination. Each crate's owner / maintainer
  makes the call at the time.
- **"We should set a target date for the first library 1.0"** —
  date-driven 1.0 contradicts the API-stability bar. If `mos-6502`
  hits 90 days post-publish stable, it can declare 1.0; if it
  doesn't, it stays 0.x until it does. No external deadline.

## Open question (deferred)

**What constitutes "external consumer" for a library crate's 1.0
eligibility?** Options: (a) any crates.io reverse dependency, (b)
a named consumer in a different project we know about, (c) the
crate owner's judgment that the API is being used "in production
somewhere." Not deciding now; the first library 1.0 makes this
concrete.

## Log

### 2026-05-23 — Decision locked

Brainstormed in-session with Steve as the answer to "what
constitutes 1.0?" The framing that landed on option 6 was the
asymmetry between binary and library audiences: binary users care
about the catalogue passing, not the version label; library users
care about API stability, which is per-crate.

Steve's framing: "I don't actually care about the unified
milestone (this may change, but right now I'm focused on doing the
work, not marketing it)." Decision aligned with that stated
priority — and explicitly reopenable if marketing pressure ever
makes the unified milestone valuable later.
