# Stop Read Data at a bad ID CRC

Read Data and Read Deleted Data now return an ID CRC error before delivering payload from a selected matching ID known to be corrupt. The check precedes the data mark and SK decision. Read Track still continues past its represented ID error; data-only CRC errors still transfer payload before returning DE/DD.

This fixes [#1852](https://github.com/emu198x/emu198x/issues/1852). It does not extend the qualified machine inventory. Every record here has `qualified=false`.

From the repository root, audit the retained evidence with:

```sh
python3.14 docs/plans/2026-10-19-read-data-id-crc/verify.py
python3.14 -m unittest discover -s docs/plans/2026-10-19-read-data-id-crc -v
python3.14 -m unittest discover -s docs/plans/2026-10-19-read-data-id-crc/reference -v
```

These commands inspect captured evidence; they do not execute an emulator. The result record is [evidence/result.json](evidence/result.json). To rerun current source tests:

```sh
cargo test --locked -p nec-upd765a -p runtime-sinclair-zx-spectrum -p machine-sinclair-zx-spectrum-plus3
```

## What ran

The baseline bug reproduced through public registers in Emu and unchanged Fuse, using clean, ID-only CRC and data-only CRC cases for both read commands. Two repeats of six cases gave twelve observations per implementation. The raw Fuse fixture changes the actual CRC bytes, independently checked by the auditor.

| CRC condition | Baseline Emu payload | Stock Fuse payload | Baseline Emu ST1/ST2 | Stock Fuse ST1/ST2 |
|---|---:|---:|---|---|
| Clean | 512 bytes | 512 bytes | 80/00 | 80/00 |
| ID only | 512 bytes | 0 bytes | 20/00 | 05/00 |
| Data only | 512 bytes | 512 bytes | 20/20 | 20/20 |

The status bytes above are hexadecimal. Both references and the primary manual support stopping before payload, but Fuse's complete ID-error status differs: its bounded search loses DE and returns MA/ND. The correction follows NEC's DE error and MAME's header-CRC termination; it does not copy Fuse's whole result.

The current source passes 480 controller, runtime and +3 machine tests, with 63 existing ignored tests. Public-register tests cover zero payload on ID error, ID-before-SK even with a mismatched data mark, a later bad ID, clean/data-only transfer, recovery and unchanged media. Existing Read Track and marginal data-read tests pass. Clippy, rustfmt and the normal locked headless release build pass.

Eight small controller-only fixtures were captured from the unchanged baseline before the fix. They contain no CPU, firmware or external disk image. They cover both commands before payload and after 179 bytes, with RQM low and high. Each still decodes and re-encodes exactly in the existing v6 layout. Restoring an envelope containing that faulty active transfer now produces the ID-error result before the host sees status. Eighteen current native cases cover valid data continuations and result prefixes; existing Read Track checkpoints remain unchanged. A separate synthetic old-state control checks that an already visited data mark does not leak CM into the ID-error result and that normalization is idempotent.

Only a currently pending ID-only error can be normalized. Errors already skipped or discarded by the old implementation cannot be reconstructed. Already consumed payload cannot be undone. The snapshot version and field layout remain unchanged.

Two fresh real-ROM coldboots of the existing +3 disk diagnostic passed loading and execution. The wrong expected signature failed after loading passed, while the emulator process itself exited successfully. The normal-ROM runs record the exact PAL +3 configuration, all four legitimate local firmware hashes, executable identity, commands, signature transition, CPU writes and screen hash. The firmware is not included. Their assembly and mastering artifacts came from an explicitly labelled captured fixture; this is a runtime regression, not a new complete source-to-disk qualification.

## Identities and retained evidence

The executed source is `e72ca90260bf788720ad8fe88b51d0e9b6a665f4`, based on merged Read Track fix `3ac10e612e24eed2e7a58aa56bad11a6b436c195`. The original pre-refresh task commit `d69a00b3` has the same complete tree and is retained on `review/fdc-read-data-id-crc-original`. Later changes in this packet affect documentation and auditing only. The four executed Rust files are independently hashed in [current-source.json](evidence/current-source.json).

The suite manifest says 0.38.1; this is a source build containing later changes, not a claim about the distributed 0.38.1 binary. The executable hash and compiler versions are in that record and repeated in the raw ROM results. The standard build command was:

```sh
cargo build --release --locked -p emu198x-spectrum --no-default-features
```

[evidence/triage.tar.gz](evidence/triage.tar.gz) retains the baseline study, authored adapters, raw tracks, individual logs, controller fixtures and source identities. [evidence/runtime-rom.tar.gz](evidence/runtime-rom.tar.gz) retains the three ROM runs, wrapper commands, logs, session scripts, artifacts and captured wrapper sources. Neither archive contains firmware or a native machine snapshot. The visible reference adapters match the captured authored-source hashes. Baseline capture instructions are in the archive and fixture README; the temporary capture test is retained as [reference/capture_legacy.rs](reference/capture_legacy.rs).

The top auditor checks source hashes, required unique raw runtime artifacts, meaningful stage assertions, signature transition, negative-control failure, source-built runtime identity and all eight baseline fixtures. Ten main auditor controls and five reference controls pass. The main controls deliberately reject empty evidence, missing logs, duplicate artifacts, skipped loading, zero assertions, wrong source/transition, false qualification and a wrong-control success exit.

The NEC December 1978 primary datasheet describes ID CRC termination in Read Data, refers Read Deleted Data back to that command, and distinguishes DE from DD. Fuse `seek_id` rejects the bad header before `read_datamark`; MAME `read_data_continue` ends at header CRC before data/FIFO setup. Exact held source/PDF identities are retained in the baseline study. MAME was inspected, not executed. No upstream report was filed.

## Boundaries

This change handles a selected matching ID whose metadata has DE set and DD clear. DD-set metadata cannot establish that the ID is independently corrupt, so it keeps the data-error behavior. It does not change duplicate-ID searches, wrong-CHRN handling, MT, DTL, raw gaps, index/flux details or timing. Stock-Fuse controller observations are not original-hardware or ROM qualification. All broader configuration, format and hardware coverage remains open work.
