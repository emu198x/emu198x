# Original 5200 v20 continuations

Generated before extraction on source d97b0d6f25b7128a2642309685823e7e6b06ad84.
The retained generator ran against the original shared four-port engine.
Each controller selector retains the exact warm state, final state, frame
packets and float audio bytes after held/released controller events.

Generic profiles are retired. Tests replace only the fixture envelope model
ID to compare the original engine with each explicit four-port board; this
is test setup, not a production snapshot migration. Original generic saves
must reject atomically. These synthetic inputs contain no third-party ROM.

SKCTL is 2; the probe asserts that the guest observed its held keypad switch.
Previously produced audio is consumed before the checkpoint, so the frozen
stream contains future output only. Separate existing tests retain debounce
and scan-phase coverage. `SHA256SUMS` binds all sixteen outputs and the exact
original generator; any missing or changed fixture is a verification failure.
