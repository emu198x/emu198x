# CPC version 5 saves and continuation

Captured on 2 November 2026 by `runtime-amstrad-cpc/src/board_baseline_tests.rs`
(`EMU198X_CPC_CAPTURE=<new directory>`), from the same specimen as
`../board-split-v4` and the same project-written synthetic firmware. These
files contain no commercial firmware.

Version 5 saves carry the monitor's scan beside the machine
(emu198x/emu198x#2229). A frame ends where the monitor retraces: at a vertical
sync it accepts, or by itself when there is none in range. The specimen's CRTC
raises no vertical sync, so its monitor free-runs, and a restore can resume it
exactly only because the save records where it was in its scan.

Each `.output` is a postcard tuple of three native-field records (elapsed
T-states, full `CapturedFrame`, audio sample bits) and the final complete
runtime snapshot. The test compares both direct and restored execution with
these bytes. The capture mode refuses to overwrite files.

- `amstrad-cpc464.output`: SHA-256 `42ff2b56b8e923989b0191d7878adbd343311a2a32690b3fc26b24753e902d14`
- `amstrad-cpc464.state`: SHA-256 `6a27266f3b597eb57db9d1a443dd007e632eb0cc68da7662c4ad1bab75e9481b`
- `amstrad-cpc6128.output`: SHA-256 `ac7a9f02fcfe0a109b798db549207e7d345597ba07f4e5d5cbfd8b0f68b2eb6b`
- `amstrad-cpc6128.state`: SHA-256 `63496cdc2b5fa0c3b12b362153adeaa6d2cab19a90526a39cc97ed7a46cb0146`
- `amstrad-cpc664.output`: SHA-256 `9db1c8ca5b5609311b6f27f8fad090b82f77342a08a2b29a6a943f097180f954`
- `amstrad-cpc664.state`: SHA-256 `5954246eee59a82e1ec3101ce9ccbf76790e45622b0b6599d628574fbd450af5`
