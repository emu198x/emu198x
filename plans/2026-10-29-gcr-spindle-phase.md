# Preserve the spindle phase during DOS logical-drive bookkeeping

The Chapter 5 certification job stalls on Emu198x while VICE succeeds. The
reduced ROM-driven case repeatedly decodes sector header 14, never enters the
write routine, and leaves job $90 busy. DOS's main loop scans RAM $7F=0/1 for
its dual-drive heritage. The GCR engine currently treats logical selection 1
as a missing physical track and rewinds its head offset to zero.

The fixture-free regression compares two otherwise identical running motors,
with RAM $7F changed in one: baseline advances 1234 -> 1259 while the other
rewinds to zero. VICE rotation.c gates spindle rotation by motor state; its
via2d.c reads mechanism pins without resetting angular position from DOS RAM.

1. Keep the existing electrical selection gates, but derive spindle wrap and
   advancement from the mounted physical track. In common-commodore-drive-gcr,
   do not stop or rewind the spindle on logical deselection; suppress writes
   while electrically deselected. Keep signatures and snapshot schema stable.
2. Add the invariant regression to both drive wrappers using this shared engine.
   Keep a bounded, ignored full-ROM direct write/verify integration test for
   installations with local ROMs, without committing ROM bytes.
3. Run drive/GCR tests and Clippy, build a separate C64 executable, then rerun
   certification with its original 500 polls plus B-E against VICE. Compare
   actual RAM results and full exported disks, not just the process exit.
4. Commit/push the focused fix and retain the tested executable and evidence.
   Remove isolated worktree/build cache after verifying the remote commit.

Validation: the spindle invariant was red (0 versus 1259), then green. The
ROM-driven direct write went from job $90 still busy after five million
clocks to successful write/verify and correct exported data. Both wrappers'
selection, write-suppression and snapshot checks pass. All 153 drive/GCR unit
tests, the explicit local-ROM direct-write test, two SAVE/LOAD round trips,
formatting and Clippy with warnings denied pass. The full book certification
scenario also passes with its unchanged 500-poll limit. Full Chapter 5 evidence
is retained by the umbrella experiment; ROMs are not committed.
