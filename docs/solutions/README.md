# Solutions Notes

`docs/solutions/` is curated engineering memory for emulator development. It is
not a raw work log.

Use these notes to preserve patterns, test setups, and failure modes that will
help future implementation work. If a note no longer changes how the project is
built, tested, or debugged, it should not stay in the main `solutions/` tree.

## What Belongs Here

### `implementation/`

Keep durable implementation patterns, design notes, and hardware-specific
approaches that are likely to be reused.

Examples:

- cycle-accurate micro-op patterns
- addressing-mode handling patterns
- reusable chip or bus integration approaches

### `testing/`

Keep repeatable test setup notes, harness behavior, fixture preparation, and
verification workflows.

Examples:

- test ROM setup
- harness behavior and trap detection
- fixture conversion or assembly steps

### `logic-errors/`

Keep bug notes only when they capture a recurring failure mode, a subtle
hardware behavior, or a mistake pattern that is likely to happen again.

Examples:

- decode-mask mistakes that can recur across instruction families
- state-corruption bugs caused by reused internal fields
- undocumented hardware behavior that is easy to model incorrectly

## What Should Move To Archive

Move a note out of the active `solutions/` tree when all of the following are
true:

- the bug is fixed
- regression coverage exists
- the write-up is mostly historical
- the lesson has been absorbed into code, tests, or a broader durable note

Archive destination: [archive/README.md](archive/README.md)

## Authoring Rules

- Use one file per idea.
- Keep the visible `H1` as the document title.
- Do not duplicate the title in front matter for new notes.
- Keep front matter minimal and filterable: `category`, `module`, `tags`,
  `status`, and `resolved`.
- Prefer short sections with concrete outcomes over long debugging diaries.
- End with `Regression coverage` and `Related` so the note points back to code
  or tests.

## Status Values

- `durable`: keep in the active `solutions/` tree
- `archive-candidate`: useful now, but likely to move once coverage and related
  docs are in place

## Templates

- [templates/implementation-template.md](templates/implementation-template.md)
- [templates/testing-template.md](templates/testing-template.md)
- [templates/logic-error-template.md](templates/logic-error-template.md)

## Review Rule

When touching an existing note, ask one question before expanding it: "Will
this help someone solve the same class of problem again?" If the answer is no,
shorten it or move it toward archive.
