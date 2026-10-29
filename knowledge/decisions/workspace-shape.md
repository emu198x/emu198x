# Decision: Workspace shape — stay unified until concrete pain forces a split

**Date:** 2026-05-23
**Status:** Locked, **amended 2026-10-22**: the workspace splits by layer,
then by system family; **amended again 2026-10-25**: every CPU and chip gets a
repository of its own, however many families use it, and later that day so
does every bus, board protocol and peripheral. Read
[§ Amendment 2026-10-25 (later)](#amendment-2026-10-25-later--buses-board-protocols-and-peripherals-too),
[§ Amendment 2026-10-25](#amendment-2026-10-25--every-cpu-and-chip-gets-its-own-repository)
and then
[§ Amendment 2026-10-22](#amendment-2026-10-22--extract-by-layer-then-by-system-family)
first. The body below is history for everything the amendment moves, and still
governs what stays until it moves. Governs whether and when the 113-crate workspace
splits into multiple workspaces or repos as the project grows
through Waves 2 / 3 (per the
[post-October roadmap](../../../docs/plans/2026-05-23-post-october-roadmap.md)).
Companion to [`versioning-strategy.md`](versioning-strategy.md) —
that record covers crate version management; this one covers
workspace organisation.

## Amendment 2026-10-25 (later) — buses, board protocols and peripherals too

**Layer 2 covers buses, board protocols and peripherals as well as CPUs and
chips.** Each gets a repository of its own on the same terms: the `emu198x-`
prefix, its own version, and an entry in `scripts/extract-repos.toml`. The
Atari SIO bus, the Amiga's Zorro II autoconfig protocol, the Amiga floppy
drive and keyboard, and the Ultimate Command Interface's network target are
the first of them.

Steve's reasoning, in his words: "So that anyone could assemble these pieces
into a proper emulator themselves, not just following my path." A machine is
more than its chips. Someone building an emulator of their own needs the bus
the drives hang off and the drives themselves as much as the video chip, and
if those stay inside a family repository, the only way to get them is to
take the family too.

The exceptions in § Amendment 2026-10-25 still hold, unchanged:

- **A crate that depends on a family's glue (a `common-<family>` crate) or on
  a format layout must be untangled first, or it moves with its family.** The
  Spectrum peripherals, which implement `common-sinclair-zx-spectrum`'s
  traits, are the plain case for peripherals, as the Spectrum ULAs are for
  chips.
- **The 68k family still moves last**, and an expansion board built on it,
  such as the GVP A530, moves with it.
- **Families still wait for the shell gate.**

## Amendment 2026-10-25 — every CPU and chip gets its own repository

**Every CPU and every chip gets a repository of its own, whether one family
uses it or several.** Layer 2 below no longer asks how many families use a
chip. The test of "two or more families or another sibling" is dropped, and
so is the sentence "A chip only one family uses moves with that family".

Steve's reasoning: a chip or CPU with one consumer today may simply be one
whose other uses nobody has thought of yet. So extraction no longer waits for
a second consumer to appear. Being extracted and published is what makes
those other uses possible.

What does not change:

- **Families still wait for the shell gate.** Layer 4 is as written below:
  four weeks in a row with no breaking change to `emu198x-shell`'s public
  interface, then one pilot family, then the rest. A family repository holds
  its machines, runtimes, binaries and family glue (`common-<family>`), but
  no longer its chips.

  *Measured since 2026-10-27.* The shell carries its own version rather than
  the workspace's, and CI's `Shell semver` job runs `cargo semver-checks`
  against each pull request's base. A breaking change fails it unless the
  same PR raises the shell's minor version. The four weeks therefore run
  from the last commit that raised that minor version, and start no earlier
  than 2026-10-27, since nothing recorded breaks before then.
- **The 68k family still moves last**, in one repository.
- **A crate moves as a chip only when its public interface stands on its
  own.** A chip crate that depends on a family's glue (a `common-<family>`
  crate) or on a format layout must be untangled first, or it moves with the
  family. The Spectrum ULAs, which implement `common-sinclair-zx-spectrum`'s
  `Ula` trait, are the plain case.
- **Every move still publishes**, takes the `emu198x-` prefix and its own
  version, and leaves no machine's test or corpus result changed.

`scripts/extract-repos.toml` lists every repository this amendment makes
movable; a chip that is not yet listed is either one of the exceptions above
or a rename-and-publish change still to make.

## Amendment 2026-10-22 — extract by layer, then by system family

Trigger 3 has fired (Play198x consumes cores and chips,
[#1214](https://github.com/emu198x/emu198x/issues/1214)), and the family went
multi-repo on 2026-10-21. The goal is now **one repository per system
family**, split line (b), after the layers every family shares are extracted
below them. Every extracted repository publishes, and dependents use
published versions, never `git` + `rev` pins.

1. **One repository per CPU:** 6502, Z80, 6809, LR35902, and the 68k family
   in one, because its models build on each other.
2. **One repository per shared chip**, meaning a chip two or more families or
   another sibling use: AY-3-8910 (with its 8912 facade), TMS9918, SN76489,
   8255, 6522, 6845, 6520, 6847, uPD765 (with the DSK adapter), WD1770,
   Z80 CTC, SID, Paula and the 2A03 APU. `common-z80-machine` and
   `common-tape` (with the TZX adapter) get one each too. ~~A chip only one
   family uses moves with that family.~~ *(Superseded 2026-10-25: every
   chip gets its own repository; see § Amendment 2026-10-25.)*
3. **Format layouts go to Format198x**, under the umbrella
   [`emu198x-format-layouts-move-to-format198x.md`](../../../../decisions/emu198x-format-layouts-move-to-format198x.md).
4. **One repository per system family**, single-machine families included,
   holding its machines, runtimes, ~~family-only chips~~ and binaries.
   *(Chips leave too since 2026-10-25.)*
5. **This repository keeps the platform and the hosts:** `emu198x-shell`,
   `emu198x-ui`, `emu198x-native-video` and `emu198x-web`, published for the
   families, and the cross-family hosts (`emu198x-apple`,
   `emu198x-fleet-web`, `emu198x-catalogue`, `emu198x-commodore-web`).

**Gates.**
- **CPUs and chips move now**, least churn first and the 68000 last. In the
  30 days to 2026-10-22, the 68000 had 28 commits.
- **Families wait for the shell:** four weeks in a row with no breaking change
  to `emu198x-shell`'s public interface. It had 27 commits in the same 30 days.
  Then one pilot family moves, then the rest.
- **The Spectrum family waits for the DSK adapter** to move in with the
  uPD765. That adapter is the one crate the CPC and the +3 share; nothing else
  crosses between those families.
- **Every move:** no machine's test suite or accuracy corpus result changes.

Extracted cores must preserve the hardware signals and timing required by
[RULES.md](../../RULES.md). The former interface-shape ban was retired on
2026-10-28: traits and callbacks are allowed when they preserve that behaviour.
Generated repository guidance links the [current bus-interface decision](cpu-bus-interface.md);
existing repositories adopt it when next touched, without a sweep.

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
  *(Superseded 2026-10-22 by § Amendment.)*

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

### 2026-10-22 — Amended: extract by layer, then by system family

Trigger 3 fired with Play198x, and the family went multi-repo. Steve chose
system-family repositories as the goal, with one repository per CPU and per
shared chip below them ("the amount of setup work is less important"),
families waiting until the shell settles, and a repository for each
single-machine family. See § Amendment.

### 2026-10-25 — Amended: every CPU and chip gets its own repository

Steve dropped the shared-chip test from layer 2. A chip with one consumer
today may be one whose other uses nobody has thought of yet, and extracting
and publishing it is what makes those uses possible. Families still wait for
the shell; the 68k still moves last. See § Amendment 2026-10-25.

### 2026-10-25 (later) — Amended: buses, board protocols and peripherals too

Steve extended layer 2 past chips: "So that anyone could assemble these
pieces into a proper emulator themselves, not just following my path."
Buses, board protocols and peripherals get repositories of their own, on the
same terms as chips. The family-glue and format-layout exceptions, the shell
gate and the 68k's last place are unchanged. See § Amendment 2026-10-25
(later).
