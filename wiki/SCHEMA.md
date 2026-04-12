# Wiki Schema

This wiki is an LLM-curated knowledge base for the Emu198x project. It follows the LLM Wiki pattern: raw sources (code, datasheets, reference emulators) are immutable; the wiki is a persistent, compounding layer of extracted knowledge maintained by the LLM.

## Structure

```
wiki/
  index.md          # page catalogue — one line per page, grouped by section
  log.md            # append-only record of ingests, queries, and lint passes
  SCHEMA.md         # this file

  chips/            # one page per IC, shared across all systems
  systems/<name>/   # per-system knowledge (one directory per computer/console)
  concepts/         # cross-system technical knowledge
  decisions/        # architectural choices with rationale
  tests/            # test suite status and methodology per system
  references/       # pointers to external sources and tools
```

## Conventions

- **Markdown only.** No frontmatter, no metadata — just content.
- **Cross-reference freely.** Use relative links: `[Z80](../chips/zilog-z80.md)`, `[contention](../systems/spectrum/contention.md)`.
- **Tables for data, prose for rationale.** Constants, timing values, and register maps go in tables. Design decisions get prose explaining the why.
- **One topic per page.** If a page covers two unrelated things, split it.
- **Correct, not comprehensive.** Every claim must reflect the current codebase. Remove outdated information rather than marking it stale.

## Page naming

- Chips: `{manufacturer}-{chipname}.md` matching crate names (e.g. `zilog-z80.md`, `ferranti-6c001e.md`)
- Systems: directory per system, pages within (e.g. `systems/spectrum/contention.md`)
- No spaces in filenames. Use hyphens.

## Operations

### Ingest
When a session produces new knowledge (brainstorm findings, bug investigation results, new hardware details, test suite progress), update the relevant wiki pages and append an entry to `log.md`.

### Query
At the start of a work session, read `index.md` to find relevant pages. Read those pages before starting work. This replaces re-deriving knowledge from code.

### Lint
When asked (or when something feels inconsistent), check for:
- Claims that contradict the current code
- Pages that reference removed or renamed crates/files
- Missing cross-references between related pages
- Gaps where knowledge exists in code but not in the wiki
