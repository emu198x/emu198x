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

## Non-goals

This does not argue for running every gate on every commit. The corpora are
large, some need firmware that cannot be committed, and several take hours.
Scheduled and scoped is the target, not universal.

It also does not argue that Seam 4 was wrong. Seam 4 is a good mechanism and
should stay. The claim is narrower: a good mechanism plus no trigger equals no
protection.

## Drift triggers

Stop and re-read this decision if you find yourself:

- Recording a gate as "passing" from a status document rather than a run.
- Adding a gate whose only invocation is a command in a README.
- Writing `eprintln!("skipping: …"); return;` in a gate that a summary line
  will report as `ok`.
- Deferring a re-capture with a note in a commit message. `85f3abbc` did
  exactly that, correctly and in detail, and the note went unread for nine
  weeks.
- Reasoning about catalogue health from entry counts in a manifest instead of
  a result from the harness.

## Related Documents

- [Spectrum accuracy closure campaign](spectrum-accuracy-closure-campaign.md)
- [Spectrum architecture review](spectrum-architecture-review.md) — Seam 4
- [Routing versions do not cover CPU timing](routing-versions-do-not-cover-cpu-timing.md)
- [October catalogue](october-catalogue.md)
