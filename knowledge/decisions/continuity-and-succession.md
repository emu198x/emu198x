# Decision: Continuity and succession — the bus-factor-1 mitigation

**Date:** 2026-07-03
**Status:** Active. Companion to the best-in-class programme
([`../../../../decisions/emu198x-best-in-class.md`](../../../../decisions/emu198x-best-in-class.md),
[`../../docs/plans/2026-07-03-best-in-class-programme.md`](../../docs/plans/2026-07-03-best-in-class-programme.md)).
Written because a `ce-ideate` stress-test named single-author fragility as the
programme's largest uninsured risk, and the programme *adds* solo-operator load
(hardware rigs, four campaigns, a publishing obligation) without touching it.

## The risk, stated plainly

Emu198x is a solo project (Steve + LLM agents), evenings-and-weekends pace, no
funding surface, single point of contact for everything (releases, security,
Code of Conduct, publishing credentials, org ownership). Comparable
accuracy-focused emulators show the failure mode: higan nearly died at founder
burnout and survived only because it was fork-friendly enough to continue as
ares; mGBA is famously bus-factor-1; ~73% of OSS maintainers report burnout.
Every accuracy investment in the programme is contingent on one person's
sustained multi-year output — and none of it survives that person stopping
unless the project is *resumable by someone else*.

## The decision

**Make the project resumable, and treat resumability as a real property — not a
someday-nicety.** Funding is deliberately deferred (see below); the cheap,
high-leverage move is continuity, and it is taken now while there is nothing to
lose and the context is fresh.

Resumability rests on assets the project already has, made explicit:

- **The decision-record corpus** (`knowledge/decisions/`, the umbrella
  `decisions/`) is the *why* a successor needs — every load-bearing choice with
  its rationale and drift triggers. It is already the succession mechanism; this
  record names it as one.
- **The agent-native surface** (MCP, deterministic headless machines, the
  schema-bound knowledge base) means a competent stranger *plus* an LLM can
  orient and resume far faster than on a typical solo codebase. That is a
  genuine, unusual succession asset — but only if it is packaged as one, not
  left implicit.

### What "resumable" concretely requires (the standing checklist)

1. **A load-bearing-decisions map** — which records are foundational (the
   RULES.md constraints, `cpu-bus-interface.md`, `fresh-start-rationale.md`, the
   mission and best-in-class records) versus which are incidental/historical, so
   a successor knows what must not be casually undone.
2. **Credential and ownership continuity** — where the GitHub org, crates.io
   publish rights, release-signing, and domain/hosting live, and how they would
   transfer. (Kept out of the public repo; the *pointer* to where it is
   documented lives here.)
3. **A security-contact continuity plan** — `SECURITY.md` names one person; a
   disclosure must not silently go unanswered if that person is unavailable.
4. **Fork-friendliness as posture** — the licence (GPL-2.0-or-later) and the
   published-crate story already make the project continuable by a fork; keep
   it that way (the higan→ares lesson: decide this *before* it is needed, not
   after).

### Contributor on-ramp — two lanes, not one wall

The 44-decision-record onboarding is correct for *architecture* contributions
and should stay. But it repels the two contribution classes that scale and that
the programme actually needs — and those get a **low-friction lane** that does
not require the full onboarding:

- **Catalogue-manifest authors** (adding tested titles to the compat catalogue).
- **Hardware-capture submitters** (flux dumps / logic traces + provenance
  metadata against a published spec — see W3). This is the W3
  crowdsource-before-buying-rigs path; it is also succession-seeding, because it
  builds a second set of hands that know the project.

## What is deferred (deliberately)

- **Funding surface (FUNDING.yml / sponsorship).** Not now. The tradeoffs
  (funder-capture concerns, the overhead of a funding relationship, and the fact
  that Patreon rarely replaces income for emulator devs) outweigh the benefit at
  this stage. Revisit if the hardware-truth pipeline's capital cost becomes the
  binding constraint (the `ce-ideate` "10x budget buys legality + a second
  maintainer, not more rigs" observation is the trigger to re-open it).
- **A second maintainer.** No contributor exists today; the low-friction lanes
  above are how one would first appear. Don't manufacture governance ahead of
  people.

## Drift triggers

- **Adding programme load (a new campaign, the hardware pipeline, a publishing
  obligation) without asking "does this increase solo-operator load with no
  mitigating resumability step?"** — that question is now part of taking on
  scope, not an afterthought.
- **Letting the security contact or publish credentials become a single silent
  point of failure** — if `SECURITY.md` or crates.io ownership can't be
  answered/transferred in the author's absence, the continuity property has
  quietly lapsed.
- **Raising the contributor wall for catalogue/capture work** to match the
  architecture-contribution bar — that recloses the low-friction lane and
  removes the only near-term succession-seeding path.

## Log

| Date | Event |
|------|-------|
| 2026-07-03 | Captured. `ce-ideate` stress-test flagged bus-factor-1 as the programme's largest uninsured risk; Steve chose "write the continuity note now, defer funding." Records resumability as a real property, names the low-friction contributor lanes (catalogue + hardware capture) as the succession-seeding path, and defers the funding surface with an explicit re-open trigger. |
