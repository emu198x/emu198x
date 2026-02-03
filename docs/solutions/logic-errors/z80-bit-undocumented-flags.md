# Z80 BIT Instruction X/Y Flag Source Varies by Operand Type

---
title: "Z80 BIT instruction X/Y flag source varies by operand type"
tags:
  - z80
  - emulation
  - bit-instruction
  - undocumented-flags
  - zexall
  - flag-calculation
  - xy-flags
category: logic-errors
module: emu-z80
symptoms:
  - "ZEXALL BIT test CRC mismatch (expected:5e020e98 found:fe7d580c)"
  - "Incorrect X/Y flag values after BIT instruction"
  - "BIT n,(HL) produces wrong flags while BIT n,r passes"
  - "ZEXDOC passes but ZEXALL fails on BIT tests"
root_cause: "X/Y flags (bits 3 and 5) taken from wrong source for memory operand variants"
date_solved: 2026-02-03
---

## Problem Description

The Z80 has two "undocumented" flags in the F register:
- **X flag (bit 3)** - also called F3
- **Y flag (bit 5)** - also called F5

These flags exist in the silicon but aren't documented in official Zilog manuals. ZEXALL specifically tests these undocumented behaviors, causing emulators with naive implementations to fail.

**Symptoms observed:**
- ZEXALL reports CRC mismatch on BIT instruction tests
- ZEXDOC passes (it masks undocumented flags)
- BIT n,r tests might pass while BIT n,(HL) fails

## Root Cause Analysis

A naive implementation copies X/Y flags from the value being tested:

```rust
// WRONG: X/Y always come from the tested value
flags |= value & (XF | YF);
```

This fails because the actual Z80 hardware sources X/Y flags differently based on operand type:

| Instruction Form | Value Tested | X/Y Flag Source |
|------------------|--------------|-----------------|
| `BIT n,r` (register) | Register value | Register value (same) |
| `BIT n,(HL)` | Memory at HL | High byte of HL (H register) |
| `BIT n,(IX+d)` | Memory at IX+d | High byte of effective address |
| `BIT n,(IY+d)` | Memory at IY+d | High byte of effective address |

The key insight: for memory operands, X/Y flags come from the **address**, not the **value read from memory**.

## The Fix

**File:** `crates/emu-z80/src/cpu/execute.rs`

### 1. Add `flag_source` Parameter

Modified `execute_cb_operation` to accept a separate source for X/Y flags:

```rust
/// Execute CB operation, returns Some(result) for write-back or None for BIT.
/// `flag_source` is used for undocumented X/Y flags in BIT instruction.
fn execute_cb_operation(&mut self, op: u8, value: u8, flag_source: u8) -> Option<u8> {
    match op & 0xF8 {
        // ... rotate/shift operations ...

        // BIT instructions: 0x40-0x7F
        0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
            let bit = (op >> 3) & 7;
            let mask = 1 << bit;
            let is_zero = value & mask == 0;

            let mut flags = self.regs.f & CF; // Preserve carry
            flags |= HF; // H is set
            if is_zero {
                flags |= ZF | PF; // Z and P/V are set if bit is 0
            }
            if bit == 7 && !is_zero {
                flags |= SF; // S is set if bit 7 is tested and is 1
            }
            // Undocumented: X and Y flags from flag_source, NOT value
            flags |= flag_source & (XF | YF);
            self.regs.f = flags;
            None // BIT doesn't write back
        }
        // ... RES/SET operations ...
    }
}
```

### 2. Register Operations (BIT n,r)

For register operands, `flag_source` equals `value`:

```rust
// Register operations - flag_source is the register value
let value = self.get_reg8(r);
let result = self.execute_cb_operation(op, value, value);
```

### 3. Memory Operations (BIT n,(HL))

For (HL) operands, `flag_source` is the H register:

```rust
// Memory operations - flag_source is high byte of address (H register)
let value = self.data_lo;  // Value read from memory
let flag_source = (self.addr >> 8) as u8;  // H register
let result = self.execute_cb_operation(op, value, flag_source);
```

### 4. Indexed Operations (BIT n,(IX+d) / BIT n,(IY+d))

For indexed operands, X/Y come from the high byte of the effective address:

```rust
// BIT for indexed: X/Y from high byte of computed address
0x40..=0x7F => {
    let bit = (op >> 3) & 7;
    let mask = 1 << bit;
    let is_zero = value & mask == 0;

    let mut flags = self.regs.f & CF;
    flags |= HF;
    if is_zero {
        flags |= ZF | PF;
    }
    if bit == 7 && !is_zero {
        flags |= SF;
    }
    // Undocumented: X and Y flags from high byte of address
    flags |= ((self.addr >> 8) as u8) & (XF | YF);
    self.regs.f = flags;
    return; // BIT doesn't write back
}
```

## Verification

**Tests passed:**
- ZEXDOC: 67/67 tests (documented behavior)
- ZEXALL: 67/67 tests (including undocumented flags)

Run verification:
```bash
cargo test -p emu-z80 --test zex -- --ignored --nocapture
```

## Technical Background

### Why This Behavior Exists

The Z80 has an internal register called MEMPTR (or WZ) that holds addresses during memory operations. For BIT n,(HL), the hardware internally:

1. Loads HL into MEMPTR
2. Reads memory at MEMPTR
3. Performs the bit test
4. Copies bits 3 and 5 from MEMPTR's high byte to X/Y flags

This "leaks" the address into the flags - a quirk of the silicon implementation that ZEXALL tests.

### Flag Summary for BIT Instruction

| Flag | Behavior |
|------|----------|
| S | Set if bit 7 tested and is 1 |
| Z | Set if tested bit is 0 |
| H | Always set |
| P/V | Same as Z |
| N | Always reset |
| C | Preserved |
| X (bit 3) | From `flag_source` bit 3 |
| Y (bit 5) | From `flag_source` bit 5 |

## Prevention Strategies

1. **Test with ZEXALL early** - It catches undocumented flag issues that ZEXDOC misses
2. **Separate value and flag source** - When implementing instructions with unusual flag behavior, consider if flags come from a different source than the computed value
3. **Reference Sean Young's documentation** - "The Undocumented Z80 Documented" is the authoritative source

## Related Documentation

### Internal
- [docs/milestones.md](../../milestones.md) - M2 Z80 verification criteria
- [docs/systems/spectrum.md](../../systems/spectrum.md) - ZX Spectrum Z80A usage
- [6502 BRK stale address bug](./6502-brk-stale-addr-vector.md) - Similar pattern of address affecting flags/behavior

### External
- [The Undocumented Z80 Documented (Sean Young)](http://www.z80.info/zip/z80-documented.pdf)
- [ZEXALL Test Suite](https://github.com/agn453/ZEXALL/)
- [Z80 Undocumented Flags Wiki](https://github.com/hoglet67/Z80Decoder/wiki/Undocumented-Flags)
- [Ken Shirriff's Z80 Register Analysis](http://www.righto.com/2014/10/how-z80s-registers-are-implemented-down.html)
