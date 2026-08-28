# Documentation splits on what the build reads

**Status: ACCEPTED (2026-08-28).**

## Decision

Documentation stays in `emu198x/emu198x` when something in the build reads it.
Everything else lives in the [`emu198x/docs`](https://github.com/emu198x/docs)
repository.

The test is mechanical: does a script, a CI job, or a test open this file?

**Stays here**

| Path | What reads it |
|---|---|
| `docs/status/` | `scripts/status/render_status.py --check` regenerates it and fails CI on drift; `check_registry.py` reads `docs/status/systems.toml` |
| `knowledge/decisions/` | `RULES.md` and doc comments cite them; `scripts/check-doc-links.py` resolves those citations |
| `test-data/accuracy-corpora.md` | `scripts/check-fixtures.sh` and the nightly workflow read its env-var contract |
| `RULES.md`, `README.md`, `CONTRIBUTING.md` | the repository's own entry points |

**Goes to `emu198x/docs`**

Testing policy, architecture notes, per-system status pages, plans,
brainstorms, handoffs, ideation, and the archive of superseded material.

## Why this line and not another

A file the build reads cannot rot quietly. `docs/status/` is regenerated and
diffed on every push, so a stale page fails a check rather than misleading a
reader six months later. That property is worth keeping the file next to the
code for, and it is the only property that is.

Prose documentation has no such gate, and gains nothing from proximity. What it
gains from a separate repository is that it can be revised without touching a
Rust workspace — no CI run, no release machinery, no version bump for a
paragraph.

## Linking across the split

Relative paths do not span repositories. A link from here to a page in
`emu198x/docs` is an absolute `https://github.com/emu198x/docs/...` URL, so it
resolves for a reader on GitHub and a reader with both checkouts alike.

`scripts/check-doc-links.py` enforces the rest: a repo-relative Markdown link
target that names nothing in this repository fails the `Test hygiene` job, and
the failure asks whether the file has moved to another repository. That check is
what keeps this decision true, rather than a rule nobody enforces.

## Consequences

- [`per-system-status-docs.md`](per-system-status-docs.md) specifies pages at
  `docs/systems/<manufacturer>/<system>.md`. Those pages are prose, and now
  live at `systems/<manufacturer>/<system>.md` in `emu198x/docs`. The rule it
  states is unchanged; only the repository is.
- A new document is placed by applying the test above, not by matching where
  similar documents happen to sit today.

## What this is for

The split happened without a written rule, and links were left pointing at the
old paths twice. `scripts/status/render_status.py` records the first: README
and RULES named the status pages as authoritative while they were dead links,
"long enough that nobody noticed" (#825). The second was #1259 — five links in
README and CONTRIBUTING, plus one in an issue template, aimed at documents that
had moved to `emu198x/docs`.

Both times the project's most-read files sent the reader to a 404. A rule
nobody wrote down cannot be followed, and a split with no rule invites the next
person to guess.
