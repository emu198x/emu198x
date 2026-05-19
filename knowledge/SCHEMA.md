# Wiki Schema

This wiki is an LLM-curated knowledge base for the Emu198x project. It follows the LLM Wiki pattern: raw sources (code, datasheets, reference emulators) are immutable; the wiki is a persistent, compounding layer of extracted knowledge maintained by the LLM.

## Structure

```
knowledge/
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

## Freshness and staleness

When sources disagree, this is the precedence:

1. **RULES.md** at the project root is the highest authority. Hard constraints.
2. **Decision docs** in `decisions/` are binding within their topic.
3. **System / chip / concept pages** describe the current code. Per "Correct, not comprehensive": outdated content is removed rather than marked stale.
4. **`log.md`** is append-only history. Past events, not current truth.
5. **Memory** (LLM-managed at `~/.claude/projects/-Users-stevehill-Projects/memory/`) is point-in-time and decays. Defer to wiki and RULES.md when they disagree.

A decision that supersedes an older one adds `**Status: SUPERSEDED by [link]**` to the top of the older doc. Check that line before quoting a decision.

The three sibling archives (`~/Projects/Emu198x-archive`, `~/Projects/Emu198x-archive-april2026`, `~/Projects/Emu198x-backup`) are dead reference material per RULES.md §25-29. Do not surface them in normal grep or search unless a port wave is explicitly underway. Cycle-accuracy code in archives is suspect (the April 2026 fresh start changed the model); format crates and tools are usually portable.

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
