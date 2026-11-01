# Original SG-1000 v5 continuations

Captured before any clock or owner change, on the a14c6a7d hardware/runtime
baseline with metadata-only edits. The generator is
crates/runtime-sega-sg-1000/src/board_baseline_tests.rs. Each of the three
existing profiles has a full envelope and three native fields of video,
audio sample bits, elapsed clocks and final complete state.

All bytes come from a project-written cartridge. It programs VDP pattern,
colour and name tables, produces a PSG tone, reads both controllers and runs
a hold-button NMI handler. Captures retain held controls/NMI, RAM and a partial
field. They contain no third-party firmware or cartridge software.

These fixtures preserve current v5 behaviour, including the known PAL
clock/audio mismatch. They are compatibility evidence, not a hardware oracle.
Any approved PAL timing correction must preserve the original bytes as
rejection/legacy evidence rather than regenerate them to hide the change.
