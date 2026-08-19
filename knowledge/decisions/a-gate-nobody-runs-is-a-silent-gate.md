# Decision: a gate nobody runs is a silent gate

**Date:** 2026-08-10
**Status:** ACTIVE
**Applies to:** every system's catalogue and every fixture-gated accuracy test

## The question

Emu198x has strong verification gates. Two of them failed to protect anything
for months while working exactly as designed. What has to change?

## What happened

Restoring the Spectrum catalogue on 2026-08-09 turned up three independent
breakages, none of which was known beforehand:

| Breakage | Undetected since | Detection mechanism |
| --- | --- | --- |
| Media paths on the pre-reorganisation TOSEC layout | 2026-06-22 | none — nothing resolved |
| Routing versions two bumps stale | 2026-06-05 | Seam 4, worked correctly |
| `+3` restore wiping the FDC read cache | 2026-05-20 | catalogue snapshot check, worked correctly |

The middle row is the uncomfortable one. Seam 4's routing-version constants did
precisely their job: they refused to pass stale hashes, loudly, with an
actionable message, on the first entry. They did that for nine weeks and told
nobody, because running them meant someone choosing to start a half-hour job.

The third row compounds it. That bug was introduced on 2026-05-20 by the very
commit that made a mounted disk survive restore, six days after the last green
catalogue run. It then hid behind the other two breakages for three months. Its
detector also worked correctly and also went unread.

A related case in the same session: the Tom Harte Z80 gate resolved no corpus
and reported `test result: ok`. That one is a genuine defect in the gate
(fixed in `bd4e7887`) rather than an unrun gate, but the consequence is
identical — a green line that proved nothing.

## The decision

**A verification gate is only as good as the thing that makes it run.** Gate
quality — how loudly it fails, how precise its message — is worth nothing on
its own. When choosing between making a gate stricter and making it run
automatically, making it run wins.

Concretely:

1. **A gate whose only trigger is a human deciding to run it is not a gate.**
   Treat it as documentation of an intent to verify.
2. **Long runtime is not an excuse for no schedule.** The full Spectrum
   catalogue is ~90 minutes. That argues for a nightly or weekly trigger, not
   for no trigger.
3. **A gate that can skip must not report success.** A missing fixture, an
   unresolvable path or a stale oracle must fail, not return early. See
   `bd4e7887` for the shape.
4. **Scope the run so it can be scheduled.** `EMU198X_CATALOGUE_SYSTEMS`
   (`abd0360c`) exists because a four-system pass is hours and a one-system
   pass is not; per-system scheduling is what makes a schedule realistic.

## Current state of the sibling catalogues

Measured 2026-08-10 against the shared media root. Entry counts are
`[[entry]]` counts from the manifests.

| System | Entries | Media paths resolving | Routing versions declared |
| --- | ---: | --- | --- |
| Spectrum | 103 | 103 / 103 | audio 3, frame 4 — green at `ad686cb6` |
| C64 | 13 | 12 / 12 | audio 4, frame 6 |
| Amiga | 10 | **0 / 10** | none declared |
| NES | 5 | **0 / 5** | audio 1, frame 1 |

The Amiga and NES media paths do not resolve under the media root the harness
uses. Whether each expects a different root or is simply stale has not been
established here, and neither has been run to confirm — but on this evidence
neither catalogue can currently verify anything, and the Spectrum precedent is
that nobody would find out until someone tried.

**This is an observation, not an accusation of breakage.** The action it
implies is to run them and find out, before either the Amiga or NES campaign
claims a green foundation.

## A fourth instance: the BBC tape test, 2026-08-14

`machine-acorn-bbc-micro`'s `os_loads_a_real_tape` had never run. It resolved
`emulators/` two directories up instead of four, landing inside the emu198x
repo rather than at the umbrella level, so every invocation died on the UEF
before reaching the machine. It is `#[ignore]`d, so nothing invoked it
unbidden — rule 1 exactly.

The new wrinkle is the **shape of the failure**. This gate did not silently
report success; it failed loudly, as designed. But it failed with
`read UEF: NotFound`, which reads as a missing asset, and that is how it was
first reported — as something to go and fetch. `Welcome_B.uef` had shipped with
the vendored b-em throughout.

**An unrun gate's first failure is evidence about the gate, not about the
environment.** A test that has never passed has never had its fixtures, paths
or assumptions checked by anything. Read its first failure as "this test is
wrong" until proven otherwise; the environmental reading is the comfortable one
and it costs nothing to rule out first.

Fixing it gave the BBC end-to-end tape coverage it had never had, and gave the
`motorola-6845` VSync correction landed the same day an integration test it
would otherwise have lacked.

## Non-goals

This does not argue for running every gate on every commit. The corpora are
large, some need firmware that cannot be committed, and several take hours.
Scheduled and scoped is the target, not universal.

It also does not argue that Seam 4 was wrong. Seam 4 is a good mechanism and
should stay. The claim is narrower: a good mechanism plus no trigger equals no
protection.

## Amendment 2026-08-19 — the guard shape is now checked, and the checker checks itself

The decision above said a gate is only as good as the thing that makes it
run. There was a second half nobody had written down: a guard that reports
a missing fixture and *returns* makes libtest print `ok`, so the gate does
not merely go unrun — it reports success.

That shape was not rare. #1011 found **54 of them across 39 files**, most
in the Spectrum family, where the correct idiom often sat two lines above
the broken one in the same function. Two were not even `#[ignore]`d:
`emu198x-spectrum`'s MCP and script-runner tests ran on every push and
exercised nothing, because CI has no 48K ROM.

`scripts/check-fixture-guards.py` now fails the build on the shape, in a
buildless CI job.

**The checker carries its own self-test**, and that is the part worth
keeping. This repository runs no CI job over the scripts' own tests, so a
detector that had stopped detecting would report a clean tree and be
believed — exactly the failure it exists to prevent, one level up. Before
scanning, it proves it still flags a known-bad sample and still ignores an
ordinary diagnostic `eprintln!`.

A checker that cannot demonstrate it still detects is a silent gate wearing
a checker's clothes.

## Amendment 2026-08-19 — the self-test proves detection, not coverage

The section above argues a checker must demonstrate it still detects, and
this one does: known-bad sample in, hit out, before every scan. It passed
that self-test for its whole life while missing 86 guards — more than the 54
the sweep it protects had found.

Detection and coverage are different properties. The sample it proved itself
against was the shape already swept, `eprintln!` then a bare `return;`, so
the self-test kept confirming it could still see the one form nothing was
writing any more. Every guard that survived bails with `return None` or
`return Ok(())`, mostly from inside a helper whose caller goes quiet with
`let Some(x) = helper() else { return }`. The checker matched the return
keyword narrowly and never saw them.

**A self-test built only from the cases a sweep already fixed proves the
checker can find what is no longer there.** The samples have to come from
the forms *not* yet cleaned up, which means writing them from what the
codebase actually contains rather than from the example in the docstring.

What made this visible was not the gate. It was reading an unrelated test's
guard while scoping other work, recognising the shape, and checking whether
the checker matched it. The gate stayed green throughout.

## Drift triggers

Stop and re-read this decision if you find yourself:

- Recording a gate as "passing" from a status document rather than a run.
- Adding a gate whose only invocation is a command in a README.
- Writing `eprintln!("skipping: …"); return;` in a gate that a summary line
  will report as `ok` — or any other bail: `return None` from a helper and
  `return Ok(())` from a `Result` test are the same failure, and are what
  the first version of the checker missed.
- Trusting a checker's self-test as evidence of coverage. It proves the
  detector still detects the samples it carries; it says nothing about the
  forms nobody thought to write a sample for.
- Deferring a re-capture with a note in a commit message. `85f3abbc` did
  exactly that, correctly and in detail, and the note went unread for nine
  weeks.
- Reasoning about catalogue health from entry counts in a manifest instead of
  a result from the harness.
- Concluding that an asset, ROM or fixture is missing on the word of a
  never-passing test, without checking that the path it built is the path you
  meant.

## Related Documents

- [Spectrum accuracy closure campaign](spectrum-accuracy-closure-campaign.md)
- [Spectrum architecture review](spectrum-architecture-review.md) — Seam 4
- [Routing versions do not cover CPU timing](routing-versions-do-not-cover-cpu-timing.md)
- [October catalogue](october-catalogue.md)
