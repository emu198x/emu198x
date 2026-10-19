# Read Track missing-track and opcode contrasts

Scope: Spectrum launch-hardening, continuation of #1870 and PR #1879. Base 7222f23075ccec1db5ecb431421ef72139640e4f. No new dependencies, production interfaces or snapshot changes are planned.

1. Freeze 16 cases: 0x42/0x62/0xc2/0xe2 on physical IDs [1,2], [2,1], an empty represented track and a missing track entry inside a present image. Keep missing container entries distinct from unformatted raw tracks. Missing-entry references are explicitly inapplicable; no invented flux equivalence.
2. Extend only the existing test probe to select a missing track entry. Probe opcode rejection before sending remaining parameters, drain the actual one-byte invalid-command or seven-byte accepted-command result, and record the number of command bytes actually sent. Reuse the existing raw field/MFI encoder, independent decoder, Fuse event host and runner.
3. Capture every applicable engine/case twice with unchanged raw bytes, all payload/result bytes, idle and next-command recovery, source/tool/firmware/recipe identities and host/emulated bounds. Keep fresh vs replay and installed-binary/source identities explicit. Read Track MT/SK are command-bit contrasts, not supported multi-track qualification. NEC p8 fixes MT=0 but shows SK as a variable bit; p14 says multi-track/skip operations are not allowed. Preserve that ambiguity.
4. Audit exact row coverage, independent raw CRC/clocks, opcode/parameter identities, rejection framing, repeatability and explicit expected-vs-observed differences. Negative controls must reject altered command length/result/payload, missing/duplicate rows, wrong expectations, mutation and a missing prerequisite. Preserve source and logs without firmware.
5. Run controller tests, Clippy, format and Python checks. Push one reviewable continuation commit/PR, update #1870/#84 and the visual report. Do not close the whole operation or overwrite the earlier 39-case packet. Retain broader state/timing and released-guest gates.

Source inspection already finds Fuse's Read Track mask is 0x9f, while Emu and inspected MAME decode low five bits. Therefore the host must observe an early one-byte rejection, not time out waiting to send eight parameters or pretend it received seven result bytes. This is an unresolved reference contrast until captured and adjudicated, not an upstream defect claim.

The three historical donor directories specified in RULES.md are absent. Reuse the current committed probes. Preserve other worktrees and the shared release branch. Before cleanup, verify exact source/evidence export, remote commit retention, disposable files and task process exit.

Exact touched paths: `crates/nec-upd765a/tests/read_track_fields.rs`; this directory’s `cases.py`, `run.py`, `fuse_probe.c`, `mame-probe.lua`, `verify.py`, `test_verify.py`, `test_prerequisites.py`, `report.py` and derived source/evidence/report records. Existing runner/raw helpers and production code remain unchanged.
