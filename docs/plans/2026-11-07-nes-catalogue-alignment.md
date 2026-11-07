# Explain the SMB catalogue frame mismatch

Lane: engineering-frontier validation corpus work. Continue the saved nightly
accuracy investigation (#2307), without changing PPU, CPU or capture timing.

The prior session bisected Super Mario Bros. and Zelda II frame mismatches to
79f4e4225 (#1748/#1749), the independently qualified scanline-tail correction.
Contra changed at a separate frame-boundary fix. Zelda II's current audio also
differs. These are separate claims; only SMB is in this bounded change.

1. Inspect the saved SMB old/new captures and the existing #1748 evidence.
   Read the NES oracle decision and vendored Mesen2's tail-fetch path.
2. Reproduce SMB's current failure using a build from this worktree and the
   existing private NES corpus. Capture its exact image and audio hashes.
3. Independently execute the same ROM with installed FCEUX, power-on and
   bounded frame counts. Capture raw palette indices and compare the pixels
   changed by 79f4e4225, using Emu198x's palette for an RGB comparison.
   A title screen alone is not proof: measure changed pixels against both
   the old and new image and retain commands and numerical results.
4. Only if evidence shows the baseline pins the old rendering defect, update
   SMB's frame hash in `crates/emu198x-catalogue/manifest/nes.toml`, with a
   concise comment. Leave all audio hashes, Contra and Zelda II untouched.
   Run the entry before/after and make a deliberately wrong hash fail.
5. Commit/push a self-contained PR including the plan and a small evidence
   report (no copyrighted captures or ROMs), then clean task-owned worktree,
   build cache and temporary reference runs after verifying remote retention.

The existing Claude bisect worktrees, captures and private corpus remain
untouched. #2317's TAP fix is reviewed/merged separately once every current
check has passed; do not conflate it with NES catalogue validation.

## Evidence and bounded adjustment

The fresh reference attempt produced no observations: FCEUX is now 2.7.0;
inside the sandbox SDL reported `The video driver did not add any displays`,
the offscreen route exited -11, and the native route timed out after 45
seconds. No further launch attempts were made and none counts as evidence.

The independent oracle for this correction is the retained [#1748
qualification](https://github.com/stevehill1981/198x/tree/5e8b60fc/ops/qualification/evidence/2026-10-13-nes):
FCEUX at frames 120, 121 and 240, the Mesen2 tail-fetch source, a regression
that failed before the fix, and 1,474,560 asymmetric/fine-scroll assertions.
This is previous qualification of the hardware effect, not a fresh execution
of the commercial game. The coordinator approved this narrower evidence route.

Current SMB capture reproduces the saved post-fix capture exactly. Only 168
pixels differ from the pre-fix capture, all in columns 0–7 and rows 196–239;
the remaining 61,272 pixels and the complete audio hash are unchanged. On the
bottom 32 rows the current first eight columns match the same repeated brick
tile 16 pixels to the right; the old image had 128 mismatches. The current PPU
suite passes all 112 tests, including the alignment regression. The adjacent
JSON report retains the identities and measurements without copying media.

## Outcome checks

- Before the one-hash update: `[FAIL] super-mario-bros`, exit 1, observed
  `xxh64:1d51307237226514` against `xxh64:45173ac7294ac723`.
- After: `[PASS] super-mario-bros`, exit 0.
- Deliberately wrong `xxh64:0000000000000000`: `[FAIL] super-mario-bros`,
  exit 1. The existing checker demonstrably still detects bad fingerprints.
- Full NES catalogue: three pass (SMB, Tetris, TMNT II); two remain red:
  `[FAIL] contra (Contra)` and
  `[FAIL] zelda-2 (Zelda II - The Adventure of Link)`, exit 1. Their frame
  and audio fingerprints are unchanged by this task.
- Catalogue crate suite: 45 passed, 0 failed, 2 media-backed tests ignored;
  the explicit private-corpus runs above exercise the affected entry.
- `cargo fmt --all -- --check` and `git diff --check` pass.
