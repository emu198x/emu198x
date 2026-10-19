# Make Einstein disk boot reachable from runtime input

Engineering-frontier validation, fixing #1871. No chip timing or CPU changes.

1. Add a failing runtime integration test in `crates/runtime-tatung-einstein/tests/disk_boot.rs`. Use stock MOS, a synthetic extended DSK containing an original seven-byte-header boot program, and runtime key events. Assert a nonzero marker written only by the executed program. Verify the unchanged runtime fails.
2. Map BREAK to row 0 bit 0 in `crates/runtime-tatung-einstein/src/input.rs`, corroborated by MAME LINE0. Expose it as host Pause in `crates/emu198x-tatung-einstein/src/ui.rs`, update controls and the mapping test.
3. Run runtime tests, the firmware-gated execution regression, native UI mapping test, affected Clippy and formatting. Rebuild the native headless executable and qualify the complete sample via stock MOS and disk.
4. Commit and push for review, attach the PR, and retain sample source/firmware/executable hashes. No automatic merge.

Archives named by RULES.md are absent from this host. Existing DSK construction in runtime `tests/media.rs` supplies the container convention; no parser or dependency is added.
