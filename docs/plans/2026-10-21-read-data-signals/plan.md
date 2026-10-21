# Read Data transfer-mode and interrupt investigation

Lane: Spectrum launch-hardening, operation qualification.

Base: `ce2d755aff02770c1bc9561f7211b30fadbbd255` (current main).

1. Inspect the command, read-byte, result, tick and reset paths in
   `crates/nec-upd765a/src/lib.rs`; read NEC manufacturer descriptions and
   vendored MAME/Fuse counterparts. No production changes.
2. Add a bounded register-level probe alongside this plan, temporarily install
   it as `crates/nec-upd765a/tests/read_data_signals_probe.rs`, and run with
   Rust/Cargo1.99.0, a task-owned target directory and a120second host timeout.
   Preserve actual failures and passing controls, then remove the temporary
   test. The ordinary supported suite must remain green.
3. Record current behavior and the smallest persistence/API design needed for
   correct ND, per-byte IRQ and result IRQ acknowledgement. Do not infer
   missing completion assertion from a helper call without reading the helper.
4. Obtain approval before any new persistent field or native schema change.
   Do not claim DMA handshake or timer accuracy from correct digital mode bits.
5. Retain hashes/logs/source recipe, run existing controller tests, commit
   reviewable research, and remove disposable build caches. Retain the isolated
   worktree only while an approved implementation is pending.

Owning library: `crates/nec-upd765a/`. Native version guard, if needed, belongs
in `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs`. No fixture or umbrella
changes in this investigation. DTL/MT research is in a separate workspace.
