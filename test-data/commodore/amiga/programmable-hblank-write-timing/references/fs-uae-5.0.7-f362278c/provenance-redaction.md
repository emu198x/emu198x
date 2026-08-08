# Can the registered FS-UAE v1 package be redacted in place?

No. The retained package contains capture-host details, and those details are
part of its registered byte identity.

## Recorded host details

All ten configurations contain absolute firmware, input-disk, and save-state
paths. All ten producer logs contain absolute build, data, user-ROM,
capture-input, and capture-output paths, as well as host process details. All
ten run manifests contain absolute producer, tool, configuration, input, and
capture paths together with the operator and host description. All ten
semantic records repeat the operator and host description.

These values are provenance from the original run. They are not required to
interpret the retained pixels, but they are not currently represented as
portable logical identifiers.

## Why v1 must remain unchanged

`package-v1.json` binds every configuration, log, manifest, record, and APNG
by SHA-256. The records also bind the related configuration, log, manifest,
and capture identities. Replacing a path, operator, or host string changes
those hashes and therefore changes the registered package.

The tracked repository does not contain the original raw BGRA capture tree,
the isolated input tree, or the exact producer binary used by `package.py`.
The packager validates those capture-time files and copies the configuration,
log, and run manifest verbatim. A mechanically edited tree could be made
self-consistent by rewriting hashes, but it would not be a reproduction of
the registered capture procedure.

A redacted edition therefore needs a new package registration identity and a
specified canonical redaction step, or a new capture whose adapter emits
logical path and operator identifiers at source. The semantic suite may stay
at version 1.0.0 if its stimuli and observations do not change. The current
format has no separate redacted-package revision field, so substituting a
redacted tree within this format requires a corpus-package version bump. It
must not overwrite the registered v1 evidence.

## Current verification boundary

[`verify_fs_uae_package.py`](../../tools/verify_fs_uae_package.py) checks
the retained APNGs and semantic records without emitting any capture-host
paths. It establishes that all thirty decoded frames and all ten re-derived
semantic observations match the registered hashes. It does not alter or
anonymise the underlying provenance files.

## Related Documents

- [Registered package](README.md)
- [Run manifests](manifests/README.md)
- [Producer logs](logs/README.md)
- [Captured configurations](configs/README.md)
- [Capture records](records/README.md)
