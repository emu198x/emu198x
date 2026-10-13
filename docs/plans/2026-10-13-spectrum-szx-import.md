# Restore SZX import through scripts and MCP

Lane: engineering-frontier validation. The GUI already uses the existing SZX parser, while the shared portable-snapshot reader classifies SZX but rejects it. Published v0.32.4 rejects both a synthetic 48K SZX and its ZIP wrapper before execution.

1. Reproduce with synthetic firmware and constructed CPU/RAM chunks; preserve the original input. No proprietary bytes are needed.
2. Add a regression in `crates/emu198x-spectrum/tests/script_load_szx.rs` that loads the generated snapshot, checks restored PC/RAM, executes the restored program, and checks its new memory write. Confirm it fails before changing the dispatcher.
3. Add the missing existing-parser branch in `crates/emu198x-spectrum/src/portable_snapshot.rs` and update shared documentation and MCP help. Keep malformed input errors intact.
4. Run the Spectrum tests with and without UI, format checks and Clippy. Replay direct and ZIP-wrapped snapshots through the built CLI, and prove malformed input is rejected. This verifies import/resume, not ROM media loading or every SZX chunk.
5. Review, push, open a narrow PR, and rebase merge after checks. Coordinate any release with the parent task. Retain evidence before removing this worktree and its build cache.
