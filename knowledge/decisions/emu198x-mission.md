# Decision: Emu198x mission — cycle-accurate, cross-platform, autonomous

**Date:** 2026-05-23
**Status:** Locked. Captures what's been implicit since the project's
inception. Companion to
[`../../../decisions/sibling-project-coordination.md`](../../../decisions/sibling-project-coordination.md)
(umbrella) which covers the Code198x ↔ Emu198x relationship; this
record covers Emu198x's own mission.

## What this is

The mission record. Names the three primary goals that shape every
significant Emu198x decision — including ones that have already
been made for other-stated reasons. Until this record landed,
those goals were inferable from the codebase and the product-
roadmap but not articulated as the *why*.

Captured because Steve's framing during the 2026-05-23
sibling-project brainstorm made the autonomous-mission angle
explicit: "Emu198x came out of Code198x, but can exist in its own
right" + "I've found the entire process of building emulators to
be exciting and technically challenging" + "one of the goals of
that project is to ensure that emulators are available for all
operating systems, instead of the current mishmash where some
systems have NO macOS or Linux emulator available."

## The mission, in three parts

### 1. Cycle-accurate emulation as a craft commitment

Every chip does what the silicon does. Every CPU is cycle-accurate
(per CPU — half-cycle for Z80 and 68000, cycle-level for 6502).
Every system's loop is master-clock-driven. Accuracy is a release
criterion, not a backlog item.

The bar isn't subordinate to any consumer's needs. The Tom Harte
100% pass on Z80 / 6502 / 68000 stands because cycle-accuracy is
the right thing to build, not because any curriculum unit needs
it. The seam-tightening architecture-review pattern (Spectrum,
C64, NES, Amiga) exists because closing the seams is good
engineering, not because the catalogue caught them all.

The personal-interest angle is honest: building emulators is
genuinely exciting and technically challenging work. That
motivation justifies accuracy-for-its-own-sake — pushing toward
silicon-truth past what any single consumer would demand.

Captured in: [`fresh-start-rationale.md`](fresh-start-rationale.md),
[`cpu-bus-interface.md`](cpu-bus-interface.md),
[`half-cycle-signals.md`](half-cycle-signals.md),
[`ula-drives-model.md`](ula-drives-model.md),
the four `*-architecture-review.md` records.

### 2. Cross-platform availability — fill the macOS / Linux gaps

A huge number of existing retro emulators are Windows-only. macOS
and Linux users either work without, run via Wine / emulation
layers, or use degraded alternatives. **Filling that gap is a
primary Emu198x mission.**

This isn't a hit list — it's opportunistic. When Emu198x adds a
system, the system's macOS / Linux availability improves by one,
and over time the gap shrinks for the systems that matter to the
project's audience.

**Clarified 2026-07-03 (near-term effort vs standing mission).** Gap-fill is a
*standing* mission goal, not the near-term *effort* priority. The best-in-class
programme ([`../../../decisions/emu198x-best-in-class.md`](../../../decisions/emu198x-best-in-class.md))
leads with reference-class depth on the four headliners — which is where the
living cross-platform incumbents already are, so it is not where the gap-fill
mission is best served. That is a deliberate choice: the campaigns lead because
of the Code198x launch anchor and prestige, and gap-fill is served *through* the
frontier lane (the 22 extended systems, ordered by CPU-family adjacency to the
active campaign) rather than by reprioritising ahead of the campaigns. So read
"primary mission" here as *enduring why*, not *this-quarter where the effort
goes*. The tension is named and resolved in the umbrella record's § "The mission
tension, named."

This goal has already shaped multiple decisions, even though it
hasn't been named in any of them:

- **Rust as the language** isn't just an accuracy choice. Rust's
  cross-compilation story is core to the cross-platform mission.
  C / C++ would be acceptable for accuracy; Rust is chosen
  because cross-compilation to three OSes from one source is the
  bar.
- **CI matrix covering macOS / Linux / Windows** isn't standard
  FOSS hygiene; it's mission-critical. A system that ships
  Linux-only because nobody tested macOS fails one of Emu198x's
  primary goals for that system.
- **Dependency selection** — `cpal` (cross-platform audio),
  `gilrs` (cross-platform input), `wgpu` (cross-platform GPU),
  `winit` (cross-platform windowing). Every host-layer crate is
  chosen to abstract platform-specific APIs. Mac-only paths
  (CoreAudio direct, Metal direct, GameController.framework
  direct) are deliberately not taken.
- **cargo-dist release artifacts** ship for `aarch64-apple-darwin`,
  `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`,
  `x86_64-pc-windows-msvc`. The four-target matrix is the
  mission expressed as release artifact.

### 3. System breadth that serves multiple consumers

The set of systems Emu198x emulates is shaped by:

- **Code198x Curriculum needs** — systems with current or
  near-future curriculum units. One input.
- **Pattern Library accuracy needs** — systems whose accurate
  emulation is required for the Pattern Library to demonstrate
  routines that actually work on real hardware. A stronger input
  than the curriculum because the Pattern Library's value
  proposition depends on it.
- **Chip-reuse leverage** — systems that become cheap to add once
  shared chips exist (TMS9918 + SN76489 unlock MSX / ColecoVision /
  SG-1000 / Memotech MTX / Aquarius / SVI-328 / Sord M5 as cheap
  follow-ons after the chips land). Per the
  [chip reuse map in product-roadmap.md](product-roadmap.md).
- **Historical / cultural significance** — systems that matter
  beyond what any single Code198x leg needs to cover. The
  five-axis scope (period, cultural-geographic, tier, form factor,
  distribution context) is the framework. Some of these systems
  will never have Code198x curriculum or Pattern Library content
  — that doesn't disqualify them.
- **Cross-platform-fill opportunities** — systems where the
  existing emulator landscape is Windows-only; the macOS / Linux
  gap is the wedge.
- **Wider Rust emulator ecosystem value** — the chip and format
  crates have external audience beyond Emu198x; per
  [`versioning-strategy.md`](versioning-strategy.md), some of
  these get published to crates.io.

No single input dominates. The chip-reuse map structures the
sequencing; the Code198x consumers (Curriculum + Pattern Library)
shape near-term priority within that structure;
cross-platform-fill is a swing input when an opportunity surfaces.

## Independence from Code198x

Per [`../../../decisions/sibling-project-coordination.md`](../../../decisions/sibling-project-coordination.md):

- Code198x and Emu198x are sibling projects with independent
  missions.
- Code198x is a primary near-term consumer of Emu198x but **not
  its master**.
- Emu198x has autonomous engineering logic that can override
  Code198x preferences when the two conflict.
- The Vault does not influence Emu198x system selection at all.
- Emu198x releases at engineering cadence, not curriculum
  cadence. See
  [`versioning-milestones.md`](versioning-milestones.md).

## What this changes today

**Nothing.** All three mission elements are already shaping
decisions — Rust choice, CI matrix, accuracy bar, dependency
selection, system roadmap. This record captures the *why* so
future decisions can cite it rather than re-derive it.

**Two follow-on README touches** worth doing when convenient:

1. The README's opening sentence describes the project as "a
   fresh Rust workspace for building cycle-accurate vintage
   computer and console emulators." True but flat. Adding a
   sentence about cross-platform availability and the
   accuracy-as-craft motivation would make the project's actual
   character visible. Small edit; do it when the README next
   gets touched.
2. The README's "What This Project Is Trying To Do" section
   could expand from "model the real machines directly" (the
   how) to also include the cross-platform fill mission (the
   why). One paragraph addition.

Neither is urgent.

## Drift triggers

If I'm about to suggest any of these, stop and re-read this record.

- **"Emu198x should drop accuracy work that doesn't serve a
  Code198x consumer"** — accuracy is a craft commitment, not a
  consumer-needs-driven choice. Float48K, blargg PPU, Tom Harte,
  the seam reviews — these exist because they're the right thing
  to build.
- **"Why are we testing on Windows? We're a Mac shop"** —
  cross-platform availability is a primary mission. Dropping
  Windows from CI would be a mission failure for the systems
  that are Windows-only-elsewhere-and-we-fix-that.
- **"Let's use Metal directly for better Mac performance"** —
  cross-platform abstraction (wgpu) is intentional. Mac-specific
  paths fail the mission. The performance gap would need to be
  catastrophic and unfixable in wgpu before considering it.
- **"Code198x doesn't want system X, don't add it"** — Code198x
  consumer priority is one input, not the only one. If chip-reuse
  or cross-platform-fill argues for X, X can land regardless of
  Code198x interest.
- **"The Vault has an entry for system Y, let's emulate it"** —
  the Vault is curatorial and does not influence emulator
  selection. Vault entries describe systems; some of those systems
  will never be emulated and that's by design.
- **"Why pick boring Rust when we could use a cool new language"**
  — Rust isn't only chosen for accuracy or speed; it's chosen
  because cross-compilation to three OSes from one source is core
  to the mission. Languages without that property fail the
  mission.

## Log

### 2026-05-23 — Decision locked

Three mission elements that were implicit got made explicit
during Steve's brainstorm clarification:

> "Emu198x came out of Code198x, but can exist in its own right;
> whilst development of Emu198x might be partially driven by the
> needs of the Code198x curriculum and website, I've found the
> entire process of building emulators to be exciting and
> technically challenging.
>
> Emu198x is 100% going to support systems that might never come
> to Code198x; one of the goals of that project is to ensure
> that emulators are available for all operating systems, instead
> of the current mishmash where some systems have NO macOS or
> Linux emulator available. That's one of the reasons I selected
> Rust in the first place."

Captured: (1) accuracy-as-craft, (2) cross-platform availability,
(3) system breadth that serves multiple consumers including ones
beyond Code198x. None of these are new behaviour; the record
makes them legible.

Companion record at
[`../../../decisions/sibling-project-coordination.md`](../../../decisions/sibling-project-coordination.md)
covers the Code198x ↔ Emu198x relationship from the umbrella
side.
