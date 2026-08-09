# VIC-II VICE breadth-survey assets

This directory identifies the external inputs consumed by the PAL 6569
VIC-II breadth survey.

The scope is 17 selected test programs across 13 categories, their 17
reference PNGs and the three C64 ROMs used to boot each program. The
colour-fetch-bug category contributes all five of its programs; every other
category contributes one representative. The assets themselves remain in the
operator's external testbench and firmware holdings. Their byte counts and
SHA-256 identities are pinned by [`assets-v1.json`](assets-v1.json).

The directory supplies fixture identity to the C64 verification process.
Implementation details remain in the C64 runtime tests, while interpretation
and evidence limits belong in the corresponding `knowledge/processes/`
document. Other C64 test-data directories cover independent fixtures.

Expected contents are versioned asset manifests and revisions of this README
when the selected survey contract changes. A new corpus or firmware identity
requires a new manifest version; it must not silently replace the v1 bytes.

The staged testbench's upstream source revision is unresolved. The manifest
therefore pins the exact locally audited bytes without claiming a recovered
upstream commit. The reference images are software-comparison evidence, not a
uniform physical-hardware capture set.
