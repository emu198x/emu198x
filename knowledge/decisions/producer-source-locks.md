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
