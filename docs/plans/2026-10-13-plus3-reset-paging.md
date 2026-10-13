# Restore Amstrad-class paging on reset

Scope: fix #1753 in the shared Amstrad paging controller. This is a runtime regression, not tape-loading qualification of +3 or sibling models.

1. Reproduce published v0.32.2 through script `reset` for hard and soft kinds, starting from locked nondefault normal and all-RAM mappings. Record local +3 ROM hashes; retain no ROM copies.
2. Consult the Amstrad +3 manual chapter 8 part 24 (paging lock, pp. 189–191) and Fuse 1.7.0 `specplus3_plus2a_common_reset` (zero paging latches, normal ROM 0 / RAM 5,2,0 mapping).
3. Add controller reset, call it from `SpectrumAmstradClassCore::reset`, replace the incorrect boot invariant, and test mapping, subsequent paging and RAM/ROM preservation. Files: `crates/common-sinclair-zx-spectrum-amstrad-class/src/{memory,core}.rs`, `crates/runtime-sinclair-zx-spectrum/tests/boot_invariants.rs`.
4. Prove regression red before fix; run affected crate tests, formatting, Clippy and public script replay against the source-built binary. Keep assertions about reset distinct from full boot/device reset.
5. Review, push and open a narrow PR. Coordinate merge only after the already agreed v0.32.3 release; do not expand its scope. Remove the worktree/build cache after evidence and commits are retained.
