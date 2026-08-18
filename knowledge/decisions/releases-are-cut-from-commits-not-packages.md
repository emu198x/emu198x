# Decision: releases are cut from commits, not from packages

**Date:** 2026-08-18
**Status:** ACTIVE
**Applies to:** how a version is decided and a release PR is opened
**Supersedes:** the release-plz configuration removed in this change

## The decision

The next version is computed from the conventional commits since the last tag,
by git-cliff, and applied by `scripts/prepare-release.py`. There is no
package-oriented release manager in this repo.

release-plz is removed: `release-plz.toml`, its 29 `[[package]]` entries, and
the pinned fork it was installed from.

## Why

This repo is one suite, one version, one changelog. All 215 crates carry
`version.workspace = true`, so there is exactly one version number and no
per-package versions to compute. release-plz reasons in packages, which is a
dimension that does not exist here.

Almost all of the tool had already been switched off to fit. Publishing,
tagging, GitHub releases and the changelog were disabled in `release-plz.toml`;
the tag was hand-rolled shell in the workflow; the changelog was git-cliff.
What remained was one job — decide the next version — supported by 29 hand-kept
package entries, a 52-line idempotency script, and a build of
`structured-world/release-plz` pinned to a commit, for a fix whose upstream PR
(release-plz#2789) is still open.

That one remaining job is what failed. `git_tag_name = "v{{ version }}"` is a
single suite tag shared by every package, so release-plz resolved "latest
release of `emu198x-amstrad-cpc`" to `v0.2.3` — a tag cut an hour before that
crate was written — checked out a worktree at it, and errored because the
package was not there.

Every run failed from 2026-08-15 to 2026-08-18. Because the changelog step then
sat *after* it in the same job, the release PR silently froze two days behind
with 5 entries against a range of 21. One failure, two casualties, one of them
unreported.

## Why not patch it

Cutting v0.3.0 unblocked it — the new tag contains the crate — but only that
instance. **Any crate added after a tag reproduces it exactly**, because the
shared tag makes release-plz believe every package existed at the last release.
This repo adds machine crates routinely.

The three problems were not independent:

- a fork of an unmerged upstream PR,
- a hand-maintained list that must be updated with every new machine,
- a tag model that mis-resolves any crate newer than the last release.

Each exists to make a package-oriented tool serve a repo with one version.
Fixing them individually means maintaining all three indefinitely.

## What replaced it

- `scripts/prepare-release.py` — computes the next version via
  `git-cliff --bumped-version`, sets the workspace version, and rewrites the
  intra-workspace dependency requirements when the caret range moves.
- The `Maintain release` workflow — tags the suite version, then prepares the
  next release PR.

The requirement rewrite is the non-obvious part, and it is real work release-plz
was doing. All 634 intra-workspace path dependencies request the current version
as a caret range, which admits a patch bump but not a minor one:

```
error: failed to select a version for the requirement `commodore-agnus-ocs = "^0.2.0"`
candidate versions found which didn't match: 0.3.0
```

It fires only when the compatible base moves, so it is a no-op for a patch
release. v0.3.0 was cut by hand because this script was not yet merged, which is
how the requirement was discovered rather than predicted.

## Decisions inside the replacement

**Bump policy is config, not code.** `cliff.toml`'s `[bump]` sets
`breaking_always_bump_major = false`, because git-cliff's default turns the
first `fix!:` after 0.2.3 into **1.0.0**. Measured, not assumed. A 1.0 is a
statement about the project, not a fact about one commit.

**A fixed release branch.** `release/next`, not `release/v{version}`. Naming
the branch after the version means a later `feat!:` computes a different
version, opens a second PR, and orphans the first — which still carries a bump
for a release that will never happen. One branch, retitled when the computed
version moves.

**On push to main, not on dispatch.** A manual dispatch cannot rot unnoticed,
which argued for it; but a release that depends on remembering to ask for one
does not happen. The always-current PR is the reminder, and the failure it had
before is now visible rather than silent.

**Tagging still gates the version computation.** `release-pr` genuinely
`needs: [tag]`, with no `if: always()`: the next version is computed from
commits *since the last tag*, so tagging must have happened or it measures from
the wrong point. That is the opposite of the changelog job it replaces, whose
dependency on release-plz only looked real — see
[`a-gate-nobody-runs-is-a-silent-gate.md`](a-gate-nobody-runs-is-a-silent-gate.md).

## Drift triggers

- "Just add the new crate to `release-plz.toml`" — there is no such file, and
  the hand-maintained list is one of the reasons why.
- "Bring back release-plz for per-package versions" — there are none; every
  crate is `version.workspace = true`. If that ever stops being true, this
  decision needs revisiting rather than working around.
- "Name the release branch after the version" — see *a fixed release branch*.
- "Let git-cliff pick the bump with its defaults" — that declares 1.0 on the
  next `fix!:`.
- Adding `if: always()` to `release-pr` — it would compute the version from the
  wrong tag.

## What this does not solve

`RELEASE_PLZ_TOKEN` keeps its name: it is a repository secret, and renaming it
means creating a new one. The name is now a historical artefact.

Whether the tag fires cargo-dist still depends on that token being a PAT, since
tag pushes made with `GITHUB_TOKEN` do not trigger other workflows. That was
true before and is unchanged.
