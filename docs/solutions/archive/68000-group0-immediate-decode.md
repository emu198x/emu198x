---
category: logic-errors
module: motorola-68000
tags:
  - 68000
  - opcode-decode
  - immediate-operations
status: archived
resolved: 2026-02-03
---

# 68000 Group 0 immediate decode

## Summary

`CMPI` and `EORI` were being routed into the static bit-operation path because
the decoder tested bit 11 in isolation instead of decoding the full bits 11-9
field.

## Symptoms

- `CMPI` failed to set comparison flags correctly
- `EORI` left destination values unchanged
- the instructions appeared to execute without obvious errors

## Root Cause

The broken decoder used a broad "bit 11 is set" check, which matched:

- static bit operations
- `EORI`
- `CMPI`
- illegal or reserved combinations

The fix was to dispatch on the full three-bit operation field instead.

## Fix

Replace the single-bit gate with a full `(op >> 9) & 7` dispatch:

- values `0-3`: immediate arithmetic and logic
- value `4`: static bit operations
- value `5`: `EORI`
- value `6`: `CMPI`

## Archived Note

This incident is archived because the durable lesson now lives in the broader
decode-structure notes: prefer full-field dispatch over convenience bit tests.

## Regression Coverage

Keep at least one test each for:

- `ORI`
- static bit ops
- `EORI`
- `CMPI`

## Related

- [68000 JMP/JSR decode dispatch](68000-jmp-jsr-decode-dispatch.md)
- [68000 Micro-Op Architecture](../implementation/68000-micro-op-architecture.md)
