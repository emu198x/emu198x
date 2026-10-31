# Corrected PET version-5 states

Project-written loop firmware, patterned screen cells, a held logical Plus key,
VIA shift-register audio and a pending PRG immediately before the autoload
boundary. No manufacturer firmware is included.

Each `.state` is a complete v5 runtime envelope. Each `.output` contains the
first three native frame packets, exact elapsed clocks, audio sample bits and
final complete state. These freeze the corrected model, not the old hybrid.
`runtime-commodore-pet/src/board_baseline_tests.rs` compares direct and restored
continuations. Capture into a new directory with `EMU198X_PET_CAPTURE`; the
capture path refuses to overwrite files.

The old v4 red probes remain in the docs plan's evidence directory. v4 is
intentionally rejected before decoding, under the approved compatibility policy.
