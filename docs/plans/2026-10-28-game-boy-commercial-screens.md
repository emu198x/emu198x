# Commercial Game Boy screenshot regressions (#320)

Lane: engineering-frontier. Based on the exact-frame work for #2136.

1. Identify local original images and the original DMG boot ROM by complete
   SHA-256 and length. Read the TOSEC ZIP sources without modifying them; stage
   ignored local copies for repeatable fixture runs.
2. Build the pinned local SameBoy 1.0.3 core with all outputs outside vendored
   source. Retain the probe, source-file identities and capture protocol.
3. Anchor both engines at the first real LCD frame after the boot ROM unmaps.
   Ignore artificial/LCD-off reference callbacks and explicit `NoFrame` attempts.
   With no input, compare all 160×144 DMG shade bytes at 240/480/720 fields.
4. Add the approved workspace `sha2` 0.11 dev-dependency only to
   `crates/runtime-nintendo-game-boy/Cargo.toml`; update only its lock entry.
5. Add `tests/commercial_screens.rs` and a strict expected manifest under
   `test-data/game-boy/commercial-screens/`. Tetris Rev 1 and Super Mario Land
   Rev 1 agree with SameBoy at all three checkpoints. Verify fixture identities,
   exact native packet counts, all framebuffer bytes and frame-480 save/replay.
   Keep tests fixture-ignored by default; explicitly configured missing or wrong
   inputs must fail. No ROM, firmware, snapshot or screenshot bytes are committed.
6. Retain Kirby's observed disagreement separately: 201/135/177 pixels at the
   same waypoints. It is a reference-emulator disagreement, not a physical
   hardware verdict or an approved replacement golden. Record it for follow-up.
7. Demonstrate that a bad ROM identity and a corrupted framebuffer both fail.
   Restore source and rerun both real-game tests, existing affected suites,
   strict Clippy and formatting. Rebase after #2136 merges, push, obtain green
   CI, merge and verify #320 closes.
8. Retain the evidence, verify remote commits and remove task worktrees/caches.

The user approved the test-only SHA-256 dependency. There are no production,
snapshot-layout or hardware-timing changes in this task. Archive paths named
by rule 25 are absent; this is new fixture-specific verification rather than
an archive port. All reference-emulator source remains read-only.

## Verification

Both explicitly enabled commercial tests pass, including field-480 save/replay.
A one-byte ROM mutation fails SHA-256 verification. A one-pixel framebuffer
mutation fails the field-240 expected hash. The test source was restored and
both commercial tests pass again. Existing affected suites: 63 passed, 0
failed, 11 fixture-ignored (including the two new commercial checks). Strict
all-target Clippy passes. A fresh second SameBoy run reproduces all nine hashes.
Only the approved runtime dev-dependency entry changes the lockfile; there are
no added registry packages and no production or snapshot changes.
