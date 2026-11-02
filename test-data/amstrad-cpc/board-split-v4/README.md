# Original CPC v4 states

Captured from the engine at `4bec5f901` before board extraction, using the
project-written `test-data/synthetic-firmware/amstrad-cpc.rom`. These files
contain no commercial firmware. Each state has a partial field, active AY
sound, held keys, a running tape, fitted upper ROM and two discs on disc
models, and banked RAM on the 6128.

Each `.output` is a postcard tuple of three native-field records (elapsed
T-states, full `CapturedFrame`, audio sample bits) and the final complete
runtime snapshot. `runtime-amstrad-cpc/src/board_baseline_tests.rs` compares
both direct and restored execution with these original bytes. The capture
mode refuses to overwrite files. Runtime-only reset-cache repair does not
change any of these serialized bytes.

## Since 2 November 2026

These are now the evidence that older saves still load. Frames end where the
monitor retraces (emu198x/emu198x#2229), and a version 4 save carries no
monitor scan, so the outputs here no longer describe the machine and the
specimen no longer reproduces these states. `original_v4_saves_still_restore_and_run`
restores each state, and checks that once saved again as version 5 it resumes
exactly. The full baseline moved to `../save-v5`.
