# Third small-issue fix batch

Lane: engineering-frontier validation and host reliability.
Base: 07097e0f2abbed44e1fbc83e528ce4869cef303d. Work only in the isolated
fix/small-issue-sweep-3 checkout; preserve the primary checkout and other tasks.

1. **Neo Geo Metal Slug (#2133).** Reproduce the blue-screen result using the
   identified original AES BIOS and cartridge. Compare the same identities with
   the held MAME loader and an executable reference. Inspect program-region
   headers and mapped bytes before attributing the failure to CPU or video.
   Candidate files: crates/format-snk-neo-geo-romset/src/{lib,tests}.rs and the
   owning runtime/application tests. If a bounded dump-layout correction is
   confirmed, retain standard-layout/NAM-1975 controls and synthetic regressions,
   then verify original-game execution. No guest patch, timing, dependency or
   snapshot-layout change is planned. Stop for design if layout selection needs
   a new public contract rather than an unambiguous existing header.
2. Assess the next small issues after the first regression passes. Prefer fixes
   through existing contracts; retain explicit design gates on mapper snapshots,
   hardware timing and host-output schemas.

Each change gets a tested commit and retained evidence. Run owning runtime and
full default-feature application tests, strict Clippy, formatting and hygiene.
Push the batch; require completed successful CI before merge, verify closures,
then remove task-owned worktrees, caches, staged media and background processes.

## Results

The unchanged runner reproduces #2133 exactly: 900 requested frames report
901 emitted frames, with the final solid #0000E9 picture last painted at 119.
The original 2 MiB P1 has no recognition header in its first MiB. SNK's manual
requires that header at $100; held MAME source and installed software metadata
map the dump's second MiB first. The corrected loader recognises that existing
header only for one 2 MiB P1, with version zero, nonzero ID and matching ROM
size. It preserves an existing lower-half recognition code, other sizes,
multiple P-ROMs and ambiguous/unrecognised layouts. No game-name exception.

The final whole-bank regression fails before the fix (nine controls pass), then
all ten parser tests pass. Sixteen runtime tests and all five UI-enabled app
tests pass. Strict owning Clippy, formatting and fixture/doc hygiene pass.
The original-game fixture test is explicitly enabled locally; its normal
fixture-gated ignore is classified in the suite output.

Both original Asia and Japan AES BIOSes now reach the Metal Slug PUSH START
attract picture. Each captured picture matches all 71,680 MAME RGB pixels:
zero differing pixels. MAME captures frame 900; Emu requests 900 and reports
901, as before. This is loader/game-state evidence, not a native-frame timing
qualification. NAM-1975 still reaches its attract picture and its entire PNG
is byte-identical before/after. Receipt hashes, captured reports and a tested
MAME Lua reproducer are retained beside this plan; no original ROM bytes or
game screenshots are committed.

The second selected fix is the approved Asm198x PRG-codec consumer migration,
tracked in that repository's 2026-10-28-published-prg-codec plan. It completes
Emu198x #1994 after that consumer lands. Browser #1906 still needs presentation
and legacy WASM aspect-metadata integration; this batch makes no browser claim.
