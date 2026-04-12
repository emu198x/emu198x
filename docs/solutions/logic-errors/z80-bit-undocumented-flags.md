---
category: logic-errors
module: zilog-z80
tags:
  - z80
  - undocumented-flags
  - bit-instruction
  - zexall
status: durable
resolved: 2026-02-03
---

# Z80 BIT X/Y flags depend on operand type

## Summary

The Z80 `BIT` instruction does not always source undocumented `X` and `Y` flags
from the tested value. Register forms use the register value, but memory forms
use bits from the effective address. Treating every form the same breaks
`ZEXALL` even when documented behavior appears correct.

## Symptoms

- `ZEXALL` reported a CRC mismatch on `BIT` tests
- `ZEXDOC` still passed because it masks undocumented flags
- `BIT n,r` looked correct while `BIT n,(HL)` and indexed variants failed

## Root Cause

The broken implementation copied `X` and `Y` from the value being tested:

```rust
flags |= value & (XF | YF);
```

That is only correct for register operands. Memory operands leak address bits
through the internal flag path:

| Instruction form | Tested value   | X/Y flag source                |
| ---------------- | -------------- | ------------------------------ |
| `BIT n,r`        | register       | register value                 |
| `BIT n,(HL)`     | memory at HL   | high byte of `HL`              |
| `BIT n,(IX+d)`   | memory at IX+d | high byte of effective address |
| `BIT n,(IY+d)`   | memory at IY+d | high byte of effective address |

This is one of those cases where the Z80 exposes an internal address path, not
just the final logical result.

## Fix

Split the two inputs apart:

- `value`: the byte being tested
- `flag_source`: the byte that owns `X` and `Y`

```rust
fn execute_cb_operation(&mut self, op: u8, value: u8, flag_source: u8) -> Option<u8> {
    // ...
    flags |= flag_source & (XF | YF);
    // ...
}
```

Then feed the correct source for each form:

- register operations pass `value` as both inputs
- `(HL)` uses the high byte of `self.addr`
- `(IX+d)` and `(IY+d)` use the high byte of the computed effective address

## Why It Can Recur

This class of bug is durable, not just historical:

- undocumented flags often come from internal buses rather than visible results
- address-dependent side effects recur in Z80 indexed operations
- emulator code tends to overfit to "result determines flags" even when the
  silicon does something stranger

## Regression Coverage

Keep both documented and undocumented suites in place:

- `ZEXDOC`: confirms the documented surface still works
- `ZEXALL`: catches the address-dependent `X`/`Y` behavior

Useful verification command:

```bash
cargo test -p emu-z80 --test zex -- --ignored --nocapture
```

## Related

- [6502 BRK stale interrupt vector](6502-brk-stale-addr-vector.md)
- [docs/systems/spectrum.md](../../systems/spectrum.md)
- [docs/inventory.md](../../inventory.md)
- [The Undocumented Z80 Documented](http://www.z80.info/zip/z80-documented.pdf)
