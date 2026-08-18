# Decision: evidence is what ran, not what was claimed

**Date:** 2026-08-18
**Status:** ACTIVE
**Applies to:** the system status canon (`docs/status/`) and anything that
reports per-machine maturity

## The question

Every statement this repository makes about how far a machine has got is a
claim somebody typed: a `support_tier` in a profile, a milestone somebody
closed, a row in a README table. None of them is checked against the machine.
What should a status page be built out of instead?

## What happened

#825 was filed because `docs/status/current-system-usability.md` and
`docs/status/outstanding-work.md` were named as authoritative by README, by
RULES, and by the acceptance text of many open issues — and did not exist on
`main`. The links had been dead long enough that nobody noticed.

That is the visible half. The invisible half is why the pages went stale
before they went missing: keeping them true meant reconciling four
vocabularies by hand. The same machine is a crate (`emu198x-c64`), a
`machine_id` (`commodore-c64`), a label (`system:c64`) and a milestone
(`C64 100%`), and they agree only by convention. Nothing could be
cross-referenced mechanically, so reconciliation was manual, so it stopped.

Restoring the pages by hand would restore exactly the thing that decayed.

## The decision

**A status page states claims and evidence side by side, and never lets a
claim stand in for evidence.**

Three things follow.

**1. The registry states the joins.** `docs/status/systems.toml` is the one
place the four vocabularies meet, and every join is written down rather than
inferred. Three attempts to infer them by pattern during this work produced
wrong answers — false gaps for the Atom, the Dragon and the Oric, a silently
missing Game Gear, and profile ids captured as machines. A guess that is
usually right is exactly what the registry replaces. `check_registry.py`
holds it to the workspace and the tracker on every push.

**2. Evidence is a test that ran and passed. Nothing else.** In particular a
skipped test is not evidence, an `#[ignore]`d test is not evidence, and a
green CI run over a suite where neither ran proves nothing about the machine.
This follows [a gate nobody runs is a silent gate](a-gate-nobody-runs-is-a-silent-gate.md):
the Dragon golden-frame test reported `ok` in CI for nearly three months
while comparing nothing, because CI has no Dragon ROM and the test returned
early. Counting those returns as passes would encode that failure in the
status page permanently.

**3. A test is attributed to a machine through the dependency graph, not
through its name.** A machine's shipping crate transitively depends on a set
of workspace crates. A crate in exactly one shipping closure is that
machine's own; a crate in several is shared and distinguishes nothing.
`cpu-z80` passing tells you nothing about whether the Einstein boots, and a
status page built on name-matching would not know the difference.

## What this deliberately does not do

**It does not derive a support tier from counts.** A hundred passing unit
tests on a memory map do not establish that a machine boots; one golden-frame
test may establish more than all of them. Deriving a tier needs tests to
declare what they verify, and they do not yet. Until they do, the ledger is
reported and the declared tier is reported next to it — the gap between them
is the useful output, and inventing a formula to close it would put the
canon back to being a claim.

**It does not treat a closed milestone as verification.** Milestones stay on
the page, in the claims column, because they record intent and sequencing.
"Acorn Atom 100%" is closed with eleven issues done; that is a statement
about the issues, not about the Atom.

## Consequences

- Evidence differs by environment, and that is correct, not a defect. A
  machine with ROMs staged produces more evidence than CI, which has none.
  The committed canon is built from the reproducible environment, and the
  richer local run is a development instrument.
- A machine with no exclusive crates cannot have its own evidence separated
  from its neighbour's. Two machines currently ship from one crate
  (Master System and Game Gear), which is what #998 asks to split. Until it
  is split, the page must say so rather than attribute the crate's tests to
  both.
- Doctests are outside the ledger: rustdoc runs them, not a test binary, so
  the executable-level attribution used here cannot see them.

## Drift triggers

Re-read this entry when you catch any of these:

- "the milestone is closed, so the machine is done"
- "CI is green, so the machine works"
- writing a status table by hand, or editing a generated one in place
- adding a machine, crate, label or milestone without touching the registry
- proposing a formula that turns test counts into a support tier
- treating a `skip!` or an `#[ignore]` as anything but a gap
