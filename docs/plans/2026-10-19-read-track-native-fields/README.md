# Read Track native field checkpoints

13 fixtures, 180 native checkpoints per fresh run, two runs: 360 successful restores. Wrong restored-continuation expectation rejected at the intended assertion.

**Native-state preservation passes for this finite matrix. All 15 FDC operations remain partial.** This is not a new Spectrum configuration qualification or a physical timing result.

## What passed

- Actual native v6 state is saved and restored into a fresh blank +3 runtime with no disk. Every restore is byte-identical to the checkpoint. All remaining payload and result bytes match uninterrupted execution, as do final native state, idle status and a subsequent Sense Drive Status command.
- The mounted live image deliberately differs from cached source before Read Track begins. Short/long fields keep their lengths; an empty-field fixture has an unvisited canary. The missing-entry fixture removes the live track while retaining a valid cached source. Each checkpoint checks original source bytes and the complete serialized live image, so cached-source replay cannot substitute for restoration.
- Payload checkpoints cover first/interior/final-byte and applicable field/wrap boundaries at RQM low and high. Every fixture has result checkpoints after 0/1/3/6 result bytes. Missing entry has immediate result-only checkpoints, not fictitious payload assertions.
- Two independent fresh test processes produce matching observations, fixtures and checkpoint bytes. The Rust test asserts exactly 13 cases and 180 checkpoints. The offline auditor independently enumerates those keys and compares raw logs, byte vectors, identities and repetitions.
- A wrong expected byte is deliberately introduced into the restored continuation. The same compiled test exits 101 with the intended restored-continuation assertion; this is negative-control success, not a passing execution. Nineteen auditor controls also reject missing/duplicate/zero checkpoints, wrong vectors/sizes, corrupt native states after rehashing, lost recovery and incorrect failure reasons.

## Configuration and provenance

Source-built Emu crate version 0.38.3, base d92b7ca3828efc438f335a2dbdb337c80cf7f346 plus the exact archived snapshot.rs test edits. Compiler/executable/recipe/source and all asset hashes are in the campaign and manifests. Rust/Cargo and Python executable identities are retained. This uses SpectrumPlus3Runtime::blank(), the existing EDSK codec and existing direct FDC tick helpers. There is no firmware input: the machine has zero-filled ROM banks. Retained native states therefore contain blank machine memory, not proprietary firmware.

The live edit is fixture construction through the existing controller insertion API. It is not an emulated Write Data result. Direct FDC service is not a CPU/ROM disk-loading run. Baseline expectations describe current model continuation; they do not adjudicate unresolved DTL/field-size reference differences or establish silicon deadlines. No production behaviour, native schema, dependency or support surface changed.

Independent reference replay of this private native-v6 format is not applicable. The earlier fresh MAME/Fuse/controller studies in PR #1879 remain separate evidence; they were not rerun or promoted into native-state qualification here. Native preservation does not qualify magnetic gaps, flux, drive wiring, paging or unsupported hardware variants.

## Verification

The matrix passed twice and its wrong-expectation control failed as intended. The auditor passed under optimized Python and all 19 adversarial controls passed. Scoped runtime/controller regression: 477 libtest-reported passes, zero failures, 62 explicitly ignored tests; ignores receive no qualification credit. Scoped all-target Clippy, Rust format and packet Ruff checks pass. Supported build/full-workspace checks remain CI gates.

```sh
python3.14 -B -O docs/plans/2026-10-19-read-track-native-fields/verify.py
python3.14 -B -O -m unittest discover -s docs/plans/2026-10-19-read-track-native-fields -p test_verify.py -v
```

For fresh execution in a full source checkout, move the packet’s previous runs/ directory aside, then run `python3.14 -u docs/plans/2026-10-19-read-track-native-fields/run.py`. Rust/Cargo/rustup and Python 3.14 are required; no ROMs or reference emulator installation are needed. The runner refuses an existing output directory. Evidence includes authored EDSK source, serialized live fixtures, all blank-runtime checkpoint files, logs and exact capture source. No source fixture path is overwritten.

## Remaining closure gates

- Review/CI and merge of this native-state gate and comparison PR #1879.
- Applicable released guest execution for boundary fixtures. The previously released basic ROM-loader diagnostic does not exercise this matrix.
- Adjudication of the retained size/DTL/opcode/reference disagreements and capability-specific hardware/clock/revision evidence (#1831 and #1876). No upstream Fuse defect report follows from disagreement alone.

## Every fixture

| Fixture | Command / recorded N | DTL | IDs | Stored bytes | EOT | RQM low | RQM high | Result checkpoints |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| n0-dtl1 | 0 / 0 | 1 | 1 | 128 | 1 | 4 | 4 | 4 |
| n0-dtl127 | 0 / 0 | 127 | 1 | 128 | 1 | 4 | 4 | 4 |
| n5 | 5 / 5 | 255 | 1 | 4096 | 1 | 4 | 4 | 4 |
| command0-recorded2 | 0 / 2 | 255 | 1 | 512 | 1 | 4 | 4 | 4 |
| command2-recorded0 | 2 / 0 | 255 | 1 | 128 | 1 | 4 | 4 | 4 |
| represented0 | 2 / 2 | 255 | 1 | 0 | 1 | 4 | 4 | 4 |
| represented128 | 2 / 2 | 255 | 1 | 128 | 1 | 4 | 4 | 4 |
| represented768 | 2 / 2 | 255 | 1 | 768 | 1 | 4 | 4 | 4 |
| reversed-eot2 | 2 / 2 | 255 | 2,1 | 512,512 | 2 | 6 | 6 | 4 |
| duplicate-eot2 | 2 / 2 | 255 | 1,1 | 512,512 | 2 | 6 | 6 | 4 |
| forward-eot3 | 2 / 2 | 255 | 1,2 | 512,512 | 3 | 10 | 10 | 4 |
| reversed-eot3 | 2 / 2 | 255 | 2,1 | 512,512 | 3 | 10 | 10 | 4 |
| missing-entry | 2 / 2 | 255 | 1 | 512 | 1 | 0 | 0 | 4 |
