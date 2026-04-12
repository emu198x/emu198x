# Code Like It's 198x Historical Exemplar Registry

**Status:** Draft
**Date:** 2026-04-12

## Purpose

Define the research registry for historical exemplar claims attached to the pattern library.

This registry exists to keep statements like "first seen", "early commercial use", and "canonical exemplar" evidence-driven.

It is not the public-facing teaching copy. It is the planning and research artifact behind that copy.

The machine-readable registry is in [code-like-198x-historical-exemplar-registry.csv](./code-like-198x-historical-exemplar-registry.csv).

Related planning files:

- [Code Like It's 198x commercial technique taxonomy](./2026-04-12-code-like-198x-commercial-technique-taxonomy.md)
- [Code Like It's 198x pattern library structure](./2026-04-12-code-like-198x-pattern-library-structure.md)
- [Code Like It's 198x pattern backlog](./2026-04-12-code-like-198x-pattern-backlog.md)

## Why A Separate Registry Exists

- Historical claims age badly when they live only in prose.
- Different claim types need different evidence thresholds.
- The same title may matter for several patterns, but in different roles.
- Low-confidence candidates should remain visible without being presented as settled fact.

## Claim Types

Each candidate claim should be tagged as one of:

- `earliest_known_use`
- `earliest_known_commercial_use`
- `breakthrough_popularization`
- `canonical_teaching_exemplar`
- `follow_up_deepening_exemplar`

This keeps "earliest", "important", and "good for teaching" from collapsing into one vague label.

## Confidence Levels

- `hypothesis`
  - plausible lead, not yet well verified
- `likely`
  - strong candidate, but still needs better sourcing or cross-checking
- `well_supported`
  - evidence base is good enough to cite publicly with normal caution

## Evidence Sources

Each row should eventually point toward one or more evidence classes:

- contemporary release documentation
- contemporary reviews or coverage
- manual or packaging material
- source code or binary analysis
- emulator or hardware test evidence
- secondary historical research
- internal notes awaiting verification

## Public-Use Rule

No row should be turned into confident public copy unless:

- the claim type is clear
- the confidence is at least `likely`
- the evidence notes say what still remains uncertain

`canonical_teaching_exemplar` can be published earlier than `earliest_known_use`, because it is a pedagogical judgment rather than a fragile historical first.

## Initial Registry Scope

The first registry should seed claims for:

- `Spectrum`
- `NES`
- `C64`
- `Amiga`
- deepening systems that sharpen important patterns
  - `Atari 2600`
  - `Apple II`
  - `Game Boy`
  - `SNES`
  - `Spectrum` clones and `Spectrum Next`

## Registry Fields

Each row in the CSV records:

- `registry_id`
- `technique_area`
- `pattern_ids`
- `claim_type`
- `title`
- `family`
- `platform_or_profile`
- `release_year`
- `region`
- `confidence`
- `evidence_status`
- `why_it_matters`
- `notes`

## Research Workflow

1. Seed candidate titles from memory, prior reading, and obvious commercial examples.
2. Mark them `hypothesis` until at least one concrete evidence lead exists.
3. Upgrade to `likely` once the claim survives basic date and platform cross-checking.
4. Upgrade to `well_supported` only when the evidence notes are clear enough to support public-facing wording.
5. Keep public pattern pages conservative even when the internal registry is still growing.

## Immediate Use

This registry should support three near-term tasks:

- choosing sensible teaching exemplars for the first patterns
- identifying where the history is thin and needs real research
- preventing overconfident "first seen" claims from leaking into lesson or pattern copy

## Notes

- A title can appear several times if it supports several technique areas or claim types.
- Regional release order matters and should not be hand-waved away.
- Clones and enhanced-family systems should be tracked explicitly when they change the historical story rather than merely repeating it.

## Next Planning Step

Expand the registry with:

- explicit sources per row
- a lightweight verification checklist
- reviewer fields for when historical claims are ready for public-facing copy
