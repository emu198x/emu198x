---
category: implementation
module: mos-6502
tags:
  - 6502
  - illegal-opcodes
  - undocumented
  - nes
  - c64
status: durable
resolved: 2026-02-03
---

# 6502 illegal opcodes for game compatibility

## Summary

The emulator implements the stable NMOS 6502 illegal opcodes that matter for
real NES and C64 software. The work covers the common read-modify-write
families, load/store combinations, immediate oddities, undocumented NOPs, and
the halt-style JAM instructions.

## Why It Matters

These opcodes are not optional if the goal is real software compatibility. NES
and C64 programs use them for compact code, optimized memory updates, and in
some cases copy protection or hardware-specific behavior. The important part is
not just the final arithmetic result, but the bus pattern and flag behavior.

## Approach

The implementation is organized by opcode family rather than by one-off cases:

| Family                  | Examples                               | Notes                                |
| ----------------------- | -------------------------------------- | ------------------------------------ |
| Combined RMW operations | `SLO`, `RLA`, `SRE`, `RRA`             | Memory change plus accumulator logic |
| Combined compare/update | `DCP`, `ISC`                           | RMW timing with compare/subtract     |
| Load/store combinations | `LAX`, `SAX`                           | Combined register load/store paths   |
| Immediate oddities      | `ANC`, `ALR`, `ARR`, `AXS`             | Weird flag rules, especially `ARR`   |
| Undocumented NOPs       | single-, two-, and three-byte variants | Consume bytes and cycles only        |
| Halt opcodes            | `JAM`/`KIL`                            | Stop the CPU until reset             |

The main architectural addition was new RMW addressing-mode helpers for opcode
forms that do not exist in the documented instruction set:

- `addr_aby_rmw`
- `addr_izx_rmw`
- `addr_izy_rmw`

Those handlers preserve the real NMOS RMW shape:

1. fetch the effective address
2. read the original value
3. write the original value back
4. write the modified value

That allows combined operations like `SLO` and `ISC` to reuse the same timing
pattern while keeping the accumulator and flag updates instruction-specific.

## Edge Cases

- `ARR` has special flag behavior: `C` comes from bit 6, `V` comes from bit 6
  xor bit 5.
- Memory RMW forms must perform the dummy original write, not just the final
  write.
- JAM opcodes halt execution and require reset to recover.
- The implementation intentionally targets the stable, compatibility-relevant
  illegal opcodes rather than every unstable silicon quirk.

## Regression Coverage

Coverage includes:

- 13 targeted illegal-opcode unit tests
- the broader instruction test suite
- Klaus Dormann functional test: pass
- Klaus Dormann decimal-mode test: pass

That combination protects both the illegal opcode surface and the documented
instruction set from regressions.

## Related

- [6502 BRK stale interrupt vector](../logic-errors/6502-brk-stale-addr-vector.md)
- [6502 Decimal Test Setup](../testing/6502-decimal-test-setup.md)
- [docs/inventory.md](../../inventory.md)
