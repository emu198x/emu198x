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

The harness should preserve the exerciser's own labelled progress blocks as checkpoints. That gives a concrete failure boundary such as "checkpoint 37 failed" instead of only "zexall failed somewhere." Checkpoint-targeted reruns are useful for diagnosis, but unless resume support exists they still replay the suite from reset.

For routine local reruns, use `--release`. The full exerciser suites are CPU-bound enough that debug builds distort turnaround time and make checkpoint-level work look worse than it is.

### FUSE test suite

1,356 tests at `fuse-emulator-fuse/z80/tests/`. Test contention timing, I/O timing, interrupt timing. Background memory: DEADBEEF repeating. 6 event types: MR/MW/MC/PR/PW/PC.

Parse notes: `-1` is a sentinel (>= 0x10000 for addresses, >= 0x100 for bytes).

## Reference adjudication

These suites do not all answer the same question, and they are not treated as interchangeable:

- **Tom Harte** is the primary per-instruction oracle for CPU before/after state and instruction-level bus-visible behaviour.
- **ZEXDOC / ZEXALL** are program-level regression suites for the Z80 core running real software in a simple host environment.
- **FUSE** is a strong secondary reference for Spectrum-visible timing, contention, I/O, and interrupt behaviour, especially where machine integration matters more than isolated opcode semantics.

If Tom Harte and FUSE disagree, do not "average" them or silently pick whichever is more convenient. Record the disagreement, identify what behaviour is actually being measured, and resolve it against additional evidence such as hardware documentation, other trusted emulators, captured traces, and whether the difference is CPU-generic or Spectrum-specific.

## System test suites

Per-system tests verify correct behaviour at the integration level:

- **Boot tests**: each variant reaches its menu with correct screen content
- **Screen rendering tests**: pixel-level accuracy without visual inspection
- **Audio tests**: waveform verification (beeper tone, AY output)
- **Tape tests**: loading from TAP/TZX produces correct results

## Current status

See per-system test pages: [Spectrum](../tests/spectrum.md)
