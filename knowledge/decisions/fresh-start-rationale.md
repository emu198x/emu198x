# Decision: Full Ground-Up Rewrite

**Date**: April 2026

## The decision

New repo. Old repo stays as reference. No code carried forward — only knowledge, test data files, and ROMs.

## Why

The existing codebase was built "fast first, accurate later." Every accuracy improvement was a risky retrofit:

- The run loop ticked at CPU frequency instead of master oscillator
- Contention was bolted on after the fact
- The tape ran from a separate accumulator that drifted
- [Signal Part 3](../systems/spectrum/signal-part-3.md) exposed all of this — its interrupt handler is graphics data that only works when contention is cycle-perfect

The fundamental architecture was wrong. Fixing it in place would mean rewriting every system's driver loop, every test, and every timing assumption — effectively a rewrite but with the constraint of not breaking the old code along the way.

## What carried forward

- **Knowledge**: everything in this wiki
- **Test data**: Tom Harte JSON files, FUSE test suite, ZEXDOC/ZEXALL binaries
- **ROMs**: unchanged
- **Design patterns**: the [Z80 tick walker](../chips/zilog-z80.md) design was proven correct and ported forward (the design, not the code)

## Hard rule

Accuracy is foundational, not retrofitted. If it's wrong, fix it now. Don't plan to "add accuracy later" — that's how we got here.

## Drift triggers

This entry is history, not a rule — but history has drift modes too. If I'm about to suggest any of these, stop and re-read the "Hard rule" section above.

**Phrases that signal drift:**

- "We can port this from the old codebase"
- "The old repo had a neat pattern for [X], let me copy it"
- "Let's grab the [module] from the archive, it was working"
- "Ship first, fix accuracy in a later pass"
- "Add accuracy later" in any framing
- "We can retrofit [X] once the basics work"
- "The old code handled this, let's just use it"
- "For speed, let me reuse the old [implementation]"
- "Fast first, accurate later" — this is literally the framing that started the burn

**What to do when triggered:** the only things carried forward from the old repo are knowledge (this wiki), test data (Tom Harte JSON, FUSE, ZEXDOC/ZEXALL), and ROMs. Code is *not* carried forward. If I'm proposing to port something, I'm proposing to re-introduce the architectural mistakes that the rewrite exists to escape. The correct path: extract the *design* into a wiki page, implement it fresh against the new architecture, cite the old repo as reference only.

**The hard rule is absolute:** accuracy is foundational, not retrofitted. If I hear myself saying "we can fix the timing later" — even in a different form — I am wrong.

## Related

- [ULA-drives model](ula-drives-model.md) — the architecture that replaced the old one
- [Signal Part 3](../systems/spectrum/signal-part-3.md) — the demo that proved the old architecture wrong
