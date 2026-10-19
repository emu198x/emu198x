# Preserve requested ID H in Read Data results

Lane: Spectrum launch-hardening. Isolate the result-H disagreement retained in
#1841/PR #1842; do not change the selected physical side or other commands.

1. Inspect `nec-upd765a::read_result`, NEC December 1978 definitions/Table 2,
   stock Fuse data-register/result paths and MAME Read Data command results.
2. Reproduce successful/error result-H leakage through public registers in
   `crates/nec-upd765a/tests/read_result_head.rs`: both data marks, independent
   physical HD/ID H, completion, CRC, overrun, initial/later missing ID, exact
   result drain, recovery and unchanged mounted source.
3. Report the cause/design before changing production: preserve command H for
   Read Data/Read Deleted Data only; leave ST0.HD physical and Read Track alone.
4. Make the narrow result-field correction without API/dependency/state changes.
   Run the new red/green tests, full controller suite, Clippy and rustfmt.
5. Retain exact source/test/tool identities, logs and reference observations;
   file the owning issue, push one reviewed commit and a PR stacked on #1842.
   Parent owns runtime/ROM regression and release sequencing.
6. Remove this task's worktree/build after commits/evidence are retained remotely;
   otherwise report the precise retention condition. No upstream Fuse reports.
