---
category: logic-errors
module: motorola-68000
tags:
  - 68000
  - opcode-decode
  - jmp
  - jsr
status: archived
resolved: 2026-02-03
---

# 68000 JMP and JSR decode dispatch

## Summary

`JMP` and `JSR` were masked by the broader `0x4Exx` miscellaneous decode path.
The decoder reached TRAP, LINK, and UNLK handling first and never dispatched the
control-transfer instructions.

## Symptoms

- `JMP` did not move `pc` to the target
- `JSR` did not push a return address or branch
- nearby `0x4Exx` instructions still appeared to work

## Root Cause

The decoder keyed too early on the shared high bits of the `0x4Exx` family.
`JMP` and `JSR` need the lower distinguishing bits checked before the fallback
miscellaneous dispatch.

## Fix

Within the `0x4Exx` group:

1. inspect bits 7-6 first
2. dispatch `JSR` and `JMP` before miscellaneous handlers
3. let TRAP, LINK, UNLK, and MOVE USP remain in the fallback path

## Archived Note

This note is archived because the useful long-term lesson is generic: decode the
most specific subpatterns before the catch-all path inside a shared opcode
family.

## Regression Coverage

Keep direct tests for:

- `JMP` absolute long
- `JSR` absolute long
- one representative TRAP/LINK/UNLK path in the same family

## Related

- [68000 Group 0 immediate decode](68000-group0-immediate-decode.md)
- [68000 Micro-Op Architecture](../implementation/68000-micro-op-architecture.md)
