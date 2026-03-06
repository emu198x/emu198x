---
category: testing
module: mos-6502
tags:
  - 6502
  - decimal-mode
  - ca65
  - test-harness
status: durable
resolved: 2026-02-03
---

# 6502 decimal-mode test setup

## Summary

This note captures the stable setup for running Klaus Dormann's decimal-mode
test against the NMOS 6502 core on macOS. The key adaptation is converting the
original AS65-oriented source into a `ca65` and `ld65` flow and using a trap
loop that an instruction-aware harness can detect.

## Problem

The original test source assumes tooling and trap behavior that do not fit the
local environment:

- AS65 is Linux-only
- the original trap instruction is not valid for an NMOS 6502 target
- a cycle-accurate harness can miss the trap if it samples `pc` at the wrong
  time

## Setup

Use the `cc65` toolchain and a flat RAM layout:

- assemble with `ca65`
- link with `ld65`
- load the result at `$0000`
- start execution at `$0200`
- inspect the error byte at `$000B`

The test needs the expected zero-page scratch layout at `$00-$10`, with
`ERROR` stored at `$0B`.

## Workflow

1. Convert AS65 syntax to `ca65` syntax.
2. Replace the original stop instruction with a self-jump trap.
3. Assemble and link with a config that places zero page and code in RAM.
4. Run the program with instruction-boundary trap detection.
5. Stop when the self-jump trap stabilizes and check `$000B`.

Useful commands:

```bash
ca65 -o 6502_decimal_test.o 6502_decimal_test.s
ld65 -C decimal.cfg -o 6502_decimal_test.bin 6502_decimal_test.o
```

## Failure Signals

- infinite execution usually means trap detection is still tick-based
- `ERROR` at `$0B` equal to `1` means the arithmetic or flag result failed
- an HTML download in place of the source or binary means the fixture pipeline
  is wrong before the emulator even runs

## Regression Coverage

Expected successful runs are on the order of:

- about 14.5 million instructions
- about 46 million cycles

Keep this note paired with the instruction-boundary trap-detection note so the
fixture and harness assumptions do not drift apart.

## Related

- [6502 trap detection at instruction boundaries](6502-trap-detection-instruction-boundary.md)
- [6502 BRK stale interrupt vector](../logic-errors/6502-brk-stale-addr-vector.md)
- [cc65 documentation](https://cc65.github.io/doc/)
