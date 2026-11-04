# CPC version 6 saves and continuation

Fresh execution with published Z80 0.9, captured on 4 November 2026 by
`runtime-amstrad-cpc/src/board_baseline_tests.rs` using
`EMU198X_CPC_CAPTURE=<new directory>`. The specimen is unchanged from v5 and
uses only project-written synthetic firmware, with active raster, PSG,
keyboard, tape, ROM and (on disc models) drive state.

Version 6 marks corrected I/O WAIT execution (#2284). Versions 4 and 5 are
rejected before payload decoding; their original specimens remain unchanged
in the adjacent directories. No CPU fields were added or reordered.

Each `.output` contains three native-field records (elapsed T-states, full
`CapturedFrame`, audio sample bits) and the final complete snapshot. Tests
compare fresh and restored execution against every byte, including the first
field. Capture mode refuses to overwrite files.

- `amstrad-cpc464.output`: SHA-256 `719b20a1559586f6155dd019dbaa627ab0d69a12baea05485d87436e06aeec48`
- `amstrad-cpc464.state`: SHA-256 `02513eb7d9986e37a6e6e99565936344189396b14f53d5408c9105ae829a625f`
- `amstrad-cpc6128.output`: SHA-256 `b7d0006e01394e93cb000f3751b45235c88f963e97ee1c7112c1abe41857b773`
- `amstrad-cpc6128.state`: SHA-256 `fe6d67cd6bc1d7735cf0e8db12817c89cb359cdaffc0573d7b9e866ff94c2de7`
- `amstrad-cpc664.output`: SHA-256 `9775ee1b8431f415d044d9f9a0470051b6b5d8faf3ffc0745757f1c10087a068`
- `amstrad-cpc664.state`: SHA-256 `9f859630513c05167a7c9eac93544e99ce2df3be090ac8b0b4bbf6bbc41fbfb0`
