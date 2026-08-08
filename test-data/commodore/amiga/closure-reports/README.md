# Amiga closure reports

This directory retains machine-readable evidence from complete Amiga
accuracy-closure runs.

Each child directory is named by the full Git revision exercised by
`scripts/verify-amiga-closure.py`. It contains the immutable `report.json`
produced at that revision and the redacted lane logs referenced by the report.
Only an overall passing run from a clean worktree may be archived here.

The reports join the focused Amiga corpora and regression fixtures in the
neighbouring directories. They record whether the declared closure lanes
passed together; they do not replace the source evidence, conformance-process
documents, or individual test assertions.

Expected contents are one directory per retained revision. A report must not
be edited or replaced after publication. Corrections require a new committed
revision and a new closure run.

## Related Documents

- [Amiga accuracy-closure verification](../../../../knowledge/processes/amiga-accuracy-closure-verification.md)
- [Amiga accuracy closure campaign](../../../../knowledge/decisions/amiga-accuracy-closure-campaign.md)
- [Accuracy corpora](../../../accuracy-corpora.md)
