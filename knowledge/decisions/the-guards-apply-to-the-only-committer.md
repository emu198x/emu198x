# Decision: the guards apply to the only committer

**Date:** 2026-08-11
**Status:** BINDING
**Applies to:** every change to `main` in `emu198x/emu198x`

## The question

Four separate quality gates were configured, correct, and working. None of
them stopped a single thing.

- `scripts/coverage.sh` has run `--no-fail-fast` since it was written, and
  says so at line 43. Local `cargo test --workspace` does not, so a Dragon
  failure hid behind an Amiga one for two days and was reported as "the
  count of red systems is one".
- CI on `main` was red on every run from 2026-08-09 onward. Nobody read it.
- Branch protection required five status checks. Every push bypassed them
  via admin rights, printing `Bypassed rule violations` each time.
- The Spectrum coverage gate had not executed since 2026-07-10. It sits
  after the test step with no `if: always()`, so a failing test skips it —
  and before that, nothing was pushed for a month, so it never ran at all.

None of these is a missing tool. Each is a working signal that reached
nobody. The question is what actually changes that.

## What was rejected

**Splitting the monorepo.** Raised because the failures felt like they came
from everything being in one place. They did not. Every guard above would
exist per-repo, go red per-repo, and be unread per-repo — six dashboards
instead of one, plus the loss of atomic changes across `zilog-z80`,
`common-sinclair-zx-spectrum` and every machine that depends on them. This
is the shape the global rules warn about: structural complexity proposed
against a communication problem.

**More notification.** A red-build alert is worth having, but it is the same
class of instrument that already failed four times. Adding a fifth signal
to be ignored is not a fix.

## The decision

`enforce_admins` is **on** for `main`. The five required checks — Format,
Clippy, Build (macos-latest), Build (windows-latest), Coverage — apply to
the repository owner exactly as they apply to anyone else.

Being the only committer is the argument *for* this, not against it. A solo
project has no second reader; the checks are the only reader there is.
Bypassing them means nothing reads the change at all.

The merge flow is the one already documented on 2026-06-22 in
`docs/plans/ui-harness-migration-resume.md`: branch, push, PR, then

```
gh pr merge <branch> --auto --merge --delete-branch
```

`--auto` queues the merge and lands it when the checks go green, so the
wait costs attention rather than time.

Release automation is unaffected. `release-plz` pushes **tags** and opens a
**pull request**; it never pushes commits to `main`, and tag pushes are not
subject to branch protection. This was verified before enforcement was
turned on.

## What this costs

Direct pushes to `main` now fail. That is the point, and it is not free: a
one-line documentation fix takes a branch, a PR, and a CI round trip. The
trade accepted here is that a slower path which is always read beats an
instant path which is never read.

Never make a path-skipping check required — it would deadlock every merge.
That warning predates this entry and still holds.

## Drift triggers

Re-read this entry if you find yourself:

- pushing to `main` directly "just this once" because the change is small,
  or documentation, or urgent;
- turning `enforce_admins` off to unblock something;
- treating a green CI badge as evidence the suite ran — see
  [`a-gate-nobody-runs-is-a-silent-gate.md`](a-gate-nobody-runs-is-a-silent-gate.md),
  and note that 217 tests in this workspace return early when a fixture is
  missing and report `ok`;
- proposing a repository split, a new notifier, or a new dashboard in
  response to a missed failure. Ask first which existing guard already
  caught it and why nobody read the answer.
