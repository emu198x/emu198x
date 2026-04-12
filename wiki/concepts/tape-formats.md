# Tape Formats

Tape loading reduces all formats to a common representation: a sequence of pulse durations in T-states. The player toggles the EAR level after each pulse elapses.

## Supported formats

### TAP

Standard ROM timing. Each block has a flag byte, data bytes, and checksum. Pulse timings follow the Spectrum ROM loader constants (pilot, sync, data 0/1).

**Crate**: `format-tap`

### TZX

Arbitrary timing per block. Supports custom pilot tones, pause blocks, direct recording, and hardware-specific blocks. Parses to `Vec<u32>` of pulse durations.

**Crate**: `format-tzx`

## Tape motor

Runs from the master clock, not a separate accumulator. This was a [fresh start decision](../decisions/fresh-start-rationale.md) — the old codebase used an independent timer that drifted.
