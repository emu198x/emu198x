# Decision: Docs-site canon — promote a curated distillation subset

**Date:** 2026-07-03
**Status:** Active. Resolves the docs-site blocker from the 2026-07-03 audit:
the site's best "inner workings" material (`knowledge/chips/`,
`knowledge/systems/`) is deliberately gitignored, so no public site can be
built from it as-is. Companion to
[`per-system-status-docs.md`](per-system-status-docs.md) and the umbrella
[`emu198x-best-in-class.md`](../../../decisions/emu198x-best-in-class.md)
(the site hosts the W2 dashboards and W4 compat DB).

## The decision

The public docs site is built with **Astro** from three tracked bodies:

1. **`docs/systems/`** — the per-system status/inner-workings pages (already
   excellent and current; the site's spine).
2. **`knowledge/decisions/`** — already tracked and shipping.
3. **A promoted, curated subset of `knowledge/chips/` + `knowledge/systems/`**
   — each page passes a **provenance/originality review** before promotion:
   LLM-distilled content citing MAME/WinUAE/reSID/vAmiga must be verified as
   original prose citing the primary `reference/` library, not paraphrased
   third-party text. Cleared pages move into the tracked `docs/` tree
   (Vale-linted); the private `knowledge/` copy is then **deleted, not kept**
   — per SCHEMA's "correct not comprehensive", two copies means drift.

Promotion is **incremental, riding existing work**: a system's distillation
promotes as its campaign (Tier A) or write-up (Tier C) touches it —
prioritised by campaign order — not as a standalone big-bang migration.
`knowledge/` remains the private working layer for everything not yet
promoted; that layer's gitignore stance is unchanged.

Astro is the public product surface because Emu198x needs more than a generated
manual: campaign dashboards, system matrices, compatibility tables, and evidence
pages need first-class layouts while long-form docs still come from Markdown.
This also keeps the stack aligned with the Code Like It's 198x public site
(`https://code198x.com`) rather than creating a second web toolchain for the
family.

Supporting work (part of the site skeleton, from the audit):

- Rewrite the two stub on-ramp docs (`docs/architecture.md`,
  `docs/adding-a-system.md`) — both are synthesis jobs from existing
  material.
- **No shipped page links to unshippable paths**: gitignored `knowledge/`
  pages, sibling directories (`../reference/`, `../syntheses/`,
  `emulators/`), or memory files. Cite them as named sources ("VICE's
  `ciat.c`", "HRM p. 123"), not as links. Audit the four `docs/systems/`
  pages (and crate `//!` docs) that currently violate this.
- The site hosts the per-system accuracy dashboards (W2) and the compat
  database (W4), generated from nightly CI artifacts (static, no service).
  Generated pages land as static Astro data/pages, not a runtime service.

## What this is NOT

- **Not un-gitignoring `knowledge/` wholesale.** The working layer stays
  private; only reviewed pages graduate.
- **Not a documentation rewrite project.** Promotion rides campaign/write-up
  work; a system nobody is touching keeps its private notes.
- **Not a change to the reference canon.** The umbrella layered model stands;
  promoted pages cite `reference/` exactly as the private pages must.

## Drift triggers

- **`git add`-ing a `knowledge/chips|systems` page without the provenance
  review** — the review is the whole point of "curated".
- **A shipped doc linking to a gitignored or sibling path** — broken on the
  public site by construction.
- **Private and promoted copies of the same page coexisting** — delete the
  private copy at promotion.
- **Standalone "migrate the knowledge base" work** appearing on a plan —
  promotion rides campaigns; a bulk migration re-creates the provenance
  problem at scale.

## Log

| Date | Event |
|------|-------|
| 2026-07-07 | Revised implementation choice from mdBook to Astro. Reason: the public site must carry campaign dashboards and product-facing system matrices, and Code Like It's 198x (`code198x.com`) already standardises the family on Astro for public web work. Source bodies, provenance review, promotion discipline, and private-knowledge boundary unchanged. |
| 2026-07-03 | Captured. Initial strategy-session call: mdBook; site body = docs/systems + decisions + promoted curated distillation subset; provenance review mandatory; promotion rides campaign order; private copy deleted at promotion. |
