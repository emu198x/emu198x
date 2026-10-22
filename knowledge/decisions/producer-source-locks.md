# Producer source-lock receipts

Approved 2026-10-16; umbrella issue #137.

Every native producer hashes the committed `Cargo.lock` before and after the
actual cargo-dist packaging invocation. Missing, empty or changed bytes stop
packaging. Each archive receives an unsigned JSON receipt binding the source
commit, lock hash, target, producer job/run/attempt and archive/executable hashes.
The source manifests define the complete suite; an omitted package fails.

Qualification reads the actual archives independently on all five native hosts
before GitHub hosting. The producer also rechecks bytes after notarization,
which submits copies and does not modify the archives. Linux retains generated
Homebrew checksum rejection, installation and emulator smoke checks.

PRs use the real packaging jobs, without publication. The macOS signing setup
step runs only for publishing runs, so PRs receive no signing/notary secrets.
Release signing, partial-secret rejection, hardened runtime checks and
notarization remain in the existing steps. Receipt `signed: false` describes the
JSON record, independently of an executable's code signature.

The existing `allow-dirty = ["ci"]` convention remains: release.yml contains
maintained action pins, signing/notarization and Homebrew fixes. Critical
producer, secret and pre-host gates are exercised by
`python scripts/test_source_lock.py`; do not overwrite the workflow with an
unreviewed `dist generate`. No new generation mechanism is introduced.

The manual historical release-run check remains artifact-only. It cannot
manufacture producer receipts for past releases. Neither these receipts nor
their checks establish the compiled per-executable dependency/feature graph;
that remains unknown under umbrella #72.


## Release retries

Producer receipts and their verifiers must belong to the same workflow run and
attempt. A retry of only failed/selected jobs can retain successful archives from
an earlier attempt while rerunning their verifiers under a newer attempt. Those
receipts remain valid evidence of the earlier producer; the strict current-attempt
publication gate rejects them. Do not edit receipts, spoof `GITHUB_RUN_ATTEMPT`,
or relax the comparison to make a partial retry green.

For a transient failure **before hosting/publication**, inspect the run first,
then rerun the complete workflow:

```sh
gh run view RUN_ID --repo emu198x/emu198x
gh run rerun RUN_ID --repo emu198x/emu198x
```

Use all jobs, without `--failed` or `--job`. This rebuilds all native producer
packages and their downstream qualification in one new attempt. Keep the same
source/tag; a retry is not a reason to move or recreate a release tag. If hosting
already published assets, inspect that publication and its receipts separately
before choosing recovery; this pre-publication procedure does not authorize
deleting/replacing published assets.

The v0.44.1 run 39749624981 demonstrates this boundary: four retained attempt 1
producer receipts correctly failed attempt 2 verification after an ARM macOS
upload retry. The jobs API's attempt 2 view gives retained jobs new wrapper IDs
and `run_attempt=2` despite original attempt 1 timestamps. That field alone does
not prove a new producer ran. Mixed-attempt acceptance requires separately
reviewed proof of the actual successful producer attempt; it is not implemented.

The all-jobs attempt 3 plan uploaded its stable artifact names successfully with
`overwrite: false`. No artifact-name conflict was demonstrated, so no overwrite
workaround is added. Diagnose an actual 409 conflict before changing artifact
replacement policy.
