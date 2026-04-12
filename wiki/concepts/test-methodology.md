# Test Methodology

Tests are integrated from day one, not added later. Each system has CPU-level and system-level test suites. See [cycle accuracy feedback](../decisions/fresh-start-rationale.md).

## CPU test suites

### Tom Harte tests

Per-instruction tests with full before/after state. Generated from a known-good reference. Available for Z80, 6502, 6809, 68000.

- Run format: JSON files, one test per instruction variant
- Each test: initial state → execute one instruction → expected final state + bus activity
- Bus activity includes reads, writes, and contention events

### ZEXDOC / ZEXALL

CP/M test programs that exercise documented (ZEXDOC) and undocumented (ZEXALL) Z80 behaviour. Run as self-hosted programs within the emulated system. CRC-based pass/fail.

### FUSE test suite

1,356 tests at `fuse-emulator-fuse/z80/tests/`. Test contention timing, I/O timing, interrupt timing. Background memory: DEADBEEF repeating. 6 event types: MR/MW/MC/PR/PW/PC.

Parse notes: `-1` is a sentinel (>= 0x10000 for addresses, >= 0x100 for bytes).

## System test suites

Per-system tests verify correct behaviour at the integration level:

- **Boot tests**: each variant reaches its menu with correct screen content
- **Screen rendering tests**: pixel-level accuracy without visual inspection
- **Audio tests**: waveform verification (beeper tone, AY output)
- **Tape tests**: loading from TAP/TZX produces correct results

## Current status

See per-system test pages: [Spectrum](../tests/spectrum.md)
