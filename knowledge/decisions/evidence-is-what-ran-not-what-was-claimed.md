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

## Amendment 2026-08-18 — the published canon is built from CI, and gains by it

Most of this project's verification depends on ROMs, corpora and disk images
it has no right to distribute. That rules out publishing a fixture-complete
run, and the first reading of that was a loss: the public page would
understate every machine whose real evidence needs a ROM.

It is the opposite, though by less than first argued. **A fixture-gated test
only announces itself where the fixture is absent.** On a machine with the
ROMs staged those tests pass and disappear into the totals; in CI they skip
with a reason naming what they need.

Both environments were then measured on the same commit:

| | CI | Development machine |
| --- | --- | --- |
| Passed | 6,169 | 6,193 |
| Ignored | 558 | 558 |
| Skipped for a missing fixture | **26** | **2** |
| Ignored with no stated reason | 96 | 96 |

The mechanism is real and the books balance — 24 extra skips, 24 fewer
passes. But it is 24 tests, not a transformation. The dominant absence
signal is the 558 ignored, and `#[ignore]` does not care where it runs, so
that figure is identical either way.

CI is therefore the right source for reasons that need no overstating: it is
reproducible by anyone, it is marginally more diagnostic on fixtures, and it
requires no copyrighted material. Not because a development machine is
blind to the gaps.

So the published ledger has three kinds of entry, none of which needs a
byte of copyrighted material:

| Bucket | Meaning |
| --- | --- |
| Passed | Verified, reproducibly, by anyone |
| Explained absence | Names the fixture, corpus or variable it needs |
| Unexplained absence | Does not run and does not say why |

417 of the workspace's 591 `#[ignore]` attributes already carry a reason,
and the reasons name the fixture. **96 ignored tests state no reason at
all** — that residue is the finding, and it is the same in both
environments. (The source grep counts 174 bare attributes; the collector
counts 96 tests actually reporting as ignored without a reason. The
measurement is the number to quote.)

Reasons are recorded verbatim and grouped by exact string. Sorting them into
a "fixture" category by keyword would be guessing at prose — the same class
of move that produced three wrong answers while building the registry.

There is a second reason not to publish a local run: it would commit a
standing attestation of which commercial ROMs sit on one person's disk.
Nothing infringing, and no reason to put it in a public repository.

## Amendment 2026-08-19 — `support_tier` is retired

The entry above named `support_tier` as one of the claims a status page
should show beside its evidence. That was the wrong destination for it, and
measuring the field is what settled the matter.

- **It was never promoted.** Most `profiles.rs` files have exactly one
  commit touching `support_tier`: the one that created them. The handful
  with two are creation plus a structural change — adding variants,
  moving files. Not one commit in this repository's history raised a
  machine's tier because the machine got better.
- **Two of five rungs were ever used.** 28 `Boots`, 12 `Research`.
  `Usable`, `Teaching` and `Reference` were never assigned to anything.
- **Nothing consumed it** but a query path that echoed it back.
- **The Spectrum declared `Research`** with 705 own passing tests, a
  closed contention campaign and a curriculum being built on it.

Publishing that on the page built to end status drift would have put
unmaintained claims at the centre of the fix. So the field is gone: the
struct member, the `SupportTier` enum, and the
`session.profile.support_tier` query path.

**What replaces it is evidence, not a better claim.** "Does it boot" is
now checked on every push for the eight machines where that is honestly
possible — the seven whose profiles declare no firmware, proven by
synthetic cartridges, and the Spectrum, whose ROM Amstrad permits to be
distributed. For the other twenty-two the answer is "not verifiable in
public infrastructure, because the firmware cannot be", which is a fact
about the licence rather than a guess about the machine.

A tier could earn its way back. It would need a stated meaning per rung, a
mechanism that moves it, and something that consumes it. Absent all three
it was decoration that looked like data.

## Amendment 2026-08-19 (later) — fourteen of thirty, and the unit that count is in

The entry above says boot is checked "for the eight machines where that is
honestly possible" and that the other twenty-two are "not verifiable in
public infrastructure, because the firmware cannot be". Both figures are
superseded, and the second was too pessimistic about its own reasoning.

**Fourteen of the thirty registered systems now have boot evidence that runs
on every push**, in three strengths that must keep being worded differently:

| Evidence | Systems |
| --- | --- |
| Executes from cartridge and renders | 7 — 2600, 7800, Game Boy, NES, Master System, Game Gear, SG-1000 |
| Executes from ROM socket and renders | 6 — ColecoVision, MTX, MSX, M5, SVI-328, Einstein |
| Real firmware cold-starts | 1 — the Spectrum line, and the MSX via C-BIOS |

The licence framing was the thing that was wrong. "The firmware cannot be
distributed" is true and was treated as the end of the argument, when for
six machines the answer was to stop needing the firmware: a synthetic image
in the ROM socket proves the machine fetches, executes and renders without
anyone's ROM. For the MSX, C-BIOS then went further — a real BIOS nobody
needs permission for. A licence blocks *one* route to evidence, not
evidence.

**The count is in registry systems, and nothing else.** `systems.toml` has
thirty entries; `sinclair-zx-spectrum` is one of them, covering 48K, 128K,
+2, +2A, +2B and +3 inside a single shipping crate. Counting those variants
individually against the thirty produced "19 of 30" and then "20 of 30" in
the session that wrote this amendment — both inflated, the second also
counting the MSX twice because C-BIOS *raised* its evidence instead of
adding a machine. The units invite mixing precisely because the variants
have their own crates and their own boot tests.

A stronger claim about a machine already counted does not move the count. It
changes which row of the table that machine sits in, which is the more
useful thing to report anyway.

## Amendment 2026-08-19 (third) — sixteen, and free reimplementations arrive

Open ROMs and AltirraOS take the count to **16 of 30**. The C64 had no boot evidence at
all before it: Commodore's BASIC, KERNAL and character ROMs cannot be
distributed, so every C64 waypoint here ran only on a machine that already
had them, and public infrastructure knew nothing about whether the machine
started.

The 800XL follows on AltirraOS, and carries a lesson of its own: **a
project's stated licence need not govern every artefact it produces.**
Altirra the emulator is GPLv2, and the project page, the repository root
and the kernel directory all say so and nothing else — enough to conclude,
wrongly, that the ROM was GPL and needed source shipped beside it. The
kernel source *file headers* carry an all-permissive notice instead. Read
the artefact's own licence, not the project's.

It arrives in the strongest row rather than the weakest, which is worth
naming. The route that unblocked the TMS9918 six was to stop needing
firmware; the route here is a *different* firmware — clean-room, GPL, and
written precisely so emulators need nobody's permission. Two distinct
answers to one licence problem, and which one applies is a fact about what
somebody else has already built, not about the machine.

| Evidence | Systems |
| --- | --- |
| Executes from cartridge and renders | 7 |
| Executes from ROM socket and renders | 6 |
| Real firmware cold-starts | 3 — the Spectrum line, the C64 via Open ROMs, and the 800XL via AltirraOS |

7 + 6 + 3 = 16. The MSX cold-starts C-BIOS too, but it is counted in the
middle row and must not be added twice: a machine sits in one row, and
gaining stronger evidence moves it rather than duplicating it. Both its
tests stay and both keep running — the synthetic one needs no fixture and
runs on every push, the C-BIOS one needs the store.

## Amendment 2026-08-19 (fourth) — nineteen, and the route that needs nobody

Synthetic firmware takes the count to **19 of 30**: the VIC-20, the Atari
5200 and the Amiga now execute from their ROM sockets and render.

None of the three had any boot evidence. The VIC-20 has no free
reimplementation at all — Open ROMs is C64/C65 only. The Amiga has one, and
cannot use it: AROS m68k spans two ROM windows and this emulator maps one
(#1022). Waiting on either would have left both at zero indefinitely.

**The weakest route is the only one that always works.** A synthetic image
needs no licence, no upstream project, and nobody's permission — the ROM
socket takes bytes, and this project can write bytes. It answers less than a
real firmware cold start, and it answers it for every machine.

Each image ships with a **control**: the same socket, spinning, touching no
hardware. Every control renders black, and each test asserts the control
shows none of the expected colour. That converts "the framebuffer is red"
into "the framebuffer is red because our code ran" — and it is how the
expected colours were chosen rather than guessed. A boot test without a
never-ran comparison is asserting a constant.

| Evidence | Systems |
| --- | --- |
| Executes from cartridge and renders | 7 |
| Executes from ROM socket and renders | 16 |
| Real firmware cold-starts | 3 |

7 + 16 + 3 = 26, the Dragon 32 joining the CPC, BBC Micro, Aquarius, Jupiter
Ace, Atom and Oric.

**The Dragon is worth a note on method.** Three attempts failed by reasoning
from the framebuffer: too few pixels changed, so the renderer looked broken.
The fourth read memory back instead and found all 6144 bytes present — which
moved the question from "is the renderer working" to "what is the VDG
fetching", and that question had an answer. Its display base is `$0000` at
reset in a graphics mode wanting 6144 bytes, and the fill loop outruns ten
frames.

Reading back the state a test depends on, before doubting the thing under
test, would have saved three attempts here and one on the 800XL.

**Those last two are the strongest synthetic proofs, and they show the
weakness of the rest.** Every other machine here has something on screen at
power-on, so one register write suffices. The CPC and BBC have a 6845 that
is entirely unprogrammed at reset: no raster exists until the firmware
builds one. Their programs run to ~170 bytes against a dozen elsewhere, and
a pass means dozens of instructions executed in order and two chips took
their programming.

So "executes from ROM socket and renders" covers a real range, from writing
one port to constructing a display from nothing. The row does not
distinguish them, and it would be honest to say so wherever it is
published.

Neither has a background register to write. The Oric keeps colour in the
text stream as attribute bytes; the Atom's 6847 takes its mode from pins.
Both are flooded by filling video RAM instead, which is a different proof of
the same thing — and a reminder that "write the background register" is a
habit of the machines that have one, not a method.

**The Atom broke the control's assumption, and the Aquarius confirmed the
break.** Every control until then rendered black, and the wording had
started to lean on that. A 6847 with no programming still paints its
alphanumeric screen, so the Atom's "never ran" is green on dark green; the
Aquarius powers on light blue. The assertion that survives every case is *a
colour appears that the control does not contain* — never *the frame stopped
being black*. Where the power-on frame is a known constant the control test
now pins that too, so drifting off it fails as well.

The Jupiter Ace is the cheapest of the lot and for a reason worth keeping:
its character set is RAM. Redefining glyph 0 floods the screen in eight
stores, because power-on video RAM already holds glyph 0 everywhere. Knowing
where a machine keeps its character generator changes the cost of proving it
runs.

## Consequences

- Evidence differs by environment, and that is correct, not a defect. The
  committed canon is built from CI; the richer local run is a development
  instrument, and `EMU198X_STRICT_FIXTURES` is how it fails loudly where
  the fixtures are supposed to be present.
- A machine with no exclusive crates cannot have its own evidence separated
  from its neighbour's. The Master System and the Game Gear shipped from one
  crate and reported an identical 43 own passes because nothing could tell
  their tests apart; #998 split them, and they now report 16 and 13 with the
  shared runtime's tests in the shared column for both. Where a crate is
  ever shared again, the page must say so instead of attributing its tests
  twice.
- Doctests are outside the ledger: rustdoc runs them, not a test binary, so
  the executable-level attribution used here cannot see them. They are still
  *run* — a separate CI step, because collecting evidence replaced
  `cargo test --workspace` and would otherwise have stopped checking them
  silently.

## Drift triggers

Re-read this entry when you catch any of these:

- "the milestone is closed, so the machine is done"
- "CI is green, so the machine works"
- writing a status table by hand, or editing a generated one in place
- adding a machine, crate, label or milestone without touching the registry
- proposing a formula that turns test counts into a support tier
- reintroducing a declared tier without a stated meaning per rung, a
  mechanism that changes it, and a consumer that reads it
- treating a `skip!` or an `#[ignore]` as anything but a gap
- counting machine variants or crates against the thirty-system denominator
  — the unit is the registry system, and the Spectrum is one of them
- counting a machine again because its evidence got stronger
- "the firmware cannot be distributed, so this machine cannot be verified" —
  that blocks one route, and the synthetic-firmware six are the counterexample
- "the local run has better numbers, publish that one" — it has fewer
  visible gaps, which is not the same thing
- sorting ignore reasons into buckets by keyword rather than grouping them
  by their exact text
