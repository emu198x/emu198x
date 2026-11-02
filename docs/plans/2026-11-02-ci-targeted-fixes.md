# CI: targeted fixes

Agreed with Steve on 2 November 2026: fix the specific CI failures first, and
leave a family-wide CI review for later.

## What went wrong on 2 November

Shipping two small fixes (#2234, #2235) and their crate releases took most of
a day, mostly waiting on CI.

1. **Apple Silicon runners stop releases.** The v0.46.2 tag run built every
   package. Then `Verify source lock (aarch64-apple-darwin)` waited 15 minutes
   for a `macos-15` runner, got none, and was cancelled. It ran no steps and has
   no log. The release failed, and nothing was hosted. GitHub was posting
   "Due to capacity constraints, jobs targeting macOS arm64 runners may
   experience longer queue times". On the full retry (attempt 3),
   `build-local-artifacts (aarch64-apple-darwin)` queued too, while the other
   four builds started within seconds. v0.46.0 also failed on Apple Silicon
   runners (the `macos-14` brownout, fixed in #2227). That makes two of the
   last three releases.
   - A retry of only the failed job is rejected, by design
     (`knowledge/decisions/producer-source-locks.md`, "Release retries"), so
     every retry rebuilds all five platforms.
   - Unknown: what cancels a job that has queued for 15 minutes. Nothing in
     our workflows does: the job's `timeout-minutes: 10` counts running time,
     and tag runs aren't cancelled by the concurrency group.
2. **Release-plz can't prepare releases in 33 of the 42 extracted crate
   repos.** Each fails with "failed to find a workspace root". Release-plz
   compares against the commit the crate was last published from, and for
   these crates that commit predates the repo's own workspace. The fix is
   one release published from the repo itself. That fixed `pentagon-ula`
   (0.2.1), whose Release-plz now reports "already up to date", and
   `bus-sinclair-zx-spectrum` (0.1.1). Until then, each repo needs a
   hand-made release PR.
   - The hand-made PRs have their own trap: the shared release check wants
     `## [x.y.z]`, but the `pentagon-ula` 0.2.0 precedent used `## 0.2.0`.
   - Release-plz has no setting for this. `git_only` reads versions from tags
     but can't be combined with `publish`.
3. **The release PR goes stale when main moves.** With several sessions
   pushing, #2232 conflicted after its checks had passed. Maintain release
   rebuilt it on the next push, but the merge waited another CI run. This
   mostly comes from pushing often; see "Not in scope".

## Tasks

### 1. Find out what cancels a queued Apple Silicon job

- Look in the run's check-suite and timeline data, and in GitHub's documented
  queue limits, for what cancelled job 125367344614 in attempt 1 of run 41467795429.
- **Verified by:** a cited cause, or a written "not determinable" with what
  was checked.
- **Done (2 November 2026).** The job's check run carries a failure annotation
  from GitHub: "The job was not acquired by Runner of type hosted even after
  multiple attempts". GitHub's own runner assignment gave up. No limit of ours
  was involved, and there's none to raise, so option B below doesn't apply.

### 2. Let releases survive a missing Apple Silicon runner

Steve chose the simplest, fastest option (2 November 2026): drop the separate
`Verify source lock` jobs for every platform. Each producer already verifies
its own archives natively before upload, so a release needs one runner per
platform, not two, and `gh run rerun --failed` works because producer and
verifier share an attempt. Build failures can still happen; a partial retry
now recovers from them without rebuilding every platform.

- `.github/workflows/qualify-packages.yml`: remove the `source-lock` job;
  keep `linux-install`. `release.yml` is unchanged.
- `scripts/test_source_lock.py`: fail if a verifier returns to the qualify
  workflow.
- `knowledge/decisions/producer-source-locks.md`: "Producers verify
  themselves".
- **Verified by:** `python scripts/test_source_lock.py` passes, and fails
  against the old workflow; `actionlint` is clean; the PR's own release
  packaging run passes; the next tag run hosts with no qualify source-lock
  jobs.

### 3. Give every extracted crate repo a working Release-plz

- Write the hand release as a runbook in the shared workflows repo: version
  bump, `## [x.y.z] - date` changelog heading, and a PR titled `chore:
  release <crate> x.y.z`. Release-plz then publishes and tags it.
- Change the shared release check's message to name this runbook when
  Release-plz's "failed to find a workspace root" is the cause.
- Do **not** publish 33 empty releases. Each repo is fixed by its next real
  release.
- **Verified by:** the next extracted-crate release follows the runbook with
  no failed checks, and Release-plz in that repo then says "already up to
  date".

## Not in scope

- The release PR going stale (problem 3). It recovers on its own, and fixing
  it means changing how releases are cut
  (`knowledge/decisions/releases-are-cut-from-commits-not-packages.md`).
  Revisit in the family-wide review.
- A family-wide CI review, which comes after these fixes.
