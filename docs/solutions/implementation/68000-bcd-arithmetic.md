---
title: "Implement BCD arithmetic instructions (ABCD, SBCD, NBCD)"
date: 2026-02-03
category: implementation
tags:
  - 68000
  - bcd
  - arithmetic
  - packed-decimal
  - multi-precision
  - condition-codes
module: emu-68000
severity: medium
symptoms:
  - BCD arithmetic operations unavailable
  - Multi-precision decimal arithmetic not supported
  - Cannot emulate programs using packed decimal math
---

# BCD Arithmetic Instructions

The 68000 provides three BCD (Binary Coded Decimal) instructions for packed decimal arithmetic. These were essential for financial and business applications requiring exact decimal math.

## What is Packed BCD

Packed BCD stores two decimal digits per byte: the high nibble contains the tens digit, the low nibble contains the units digit. Decimal `42` is stored as `0x42` (not binary `0x2A`).

## The Three Instructions

| Instruction | Operation | Description |
|-------------|-----------|-------------|
| **ABCD** | Dy + Dx + X → Dy | Add decimal with extend |
| **SBCD** | Dy - Dx - X → Dy | Subtract decimal with extend |
| **NBCD** | 0 - ea - X → ea | Negate decimal (ten's complement) |

All support two addressing modes:
- Register: `Dx,Dy`
- Memory with predecrement: `-(Ax),-(Ay)`

## BCD Addition Algorithm

```rust
fn bcd_add(&self, src: u8, dst: u8, extend: u8) -> (u8, bool) {
    // Add low nibbles
    let mut low = (dst & 0x0F) + (src & 0x0F) + extend;

    // BCD correction: if nibble > 9, add 6
    if low > 9 {
        low += 6;
    }

    // Add high nibbles plus carry from low
    let mut high = (dst >> 4) + (src >> 4) + u8::from(low > 0x0F);

    // BCD correction for high nibble
    let carry = high > 9;
    if carry {
        high += 6;
    }

    let result = ((high & 0x0F) << 4) | (low & 0x0F);
    (result, carry)
}
```

**Why add 6?** Hex values A-F are invalid BCD. Adding 6 converts them: `9 + 1 = 0xA`, then `0xA + 6 = 0x10`, giving digit 0 with carry.

## BCD Subtraction Algorithm

```rust
fn bcd_sub(&self, dst: u8, src: u8, extend: u8) -> (u8, bool) {
    // Use signed arithmetic to detect borrows
    let low_dst = i16::from(dst & 0x0F);
    let low_src = i16::from(src & 0x0F) + i16::from(extend);
    let mut low = low_dst - low_src;

    // BCD correction: if negative, add 10
    let low_borrow = low < 0;
    if low_borrow {
        low += 10;
    }

    let high_dst = i16::from(dst >> 4);
    let high_src = i16::from(src >> 4) + i16::from(low_borrow);
    let mut high = high_dst - high_src;

    let borrow = high < 0;
    if borrow {
        high += 10;
    }

    let result = ((high as u8 & 0x0F) << 4) | (low as u8 & 0x0F);
    (result, borrow)
}
```

**Why add 10?** Wraps negative results: `0 - 1 = -1`, then `-1 + 10 = 9`.

## Flag Handling

BCD instructions share the unique Z flag behaviour with ADDX/SUBX:

| Flag | Behaviour |
|------|-----------|
| **Z** | Cleared if non-zero, **unchanged if zero** |
| **C** | Set on decimal carry/borrow |
| **X** | Mirrors C flag |
| **N** | Undefined (set from MSB for consistency) |
| **V** | Undefined |

The Z flag behaviour enables multi-byte BCD chains:

```rust
// Z: cleared if non-zero, unchanged otherwise
if result != 0 {
    sr &= !Z;
}
// C and X: set if decimal carry/borrow
sr = Status::set_if(sr, C, carry);
sr = Status::set_if(sr, X, carry);
```

## Multi-Byte BCD Arithmetic

The predecrement mode and X flag enable multi-byte operations:

```asm
; Add two 4-digit BCD numbers
    ABCD    D2,D0       ; Add low bytes, sets X if carry
    ABCD    D3,D1       ; Add high bytes, includes carry
; Z flag indicates if entire result is zero
```

## Implementation Status

| Mode | ABCD | SBCD | NBCD |
|------|------|------|------|
| Register Dx,Dy | ✓ | ✓ | ✓ |
| Memory -(Ax),-(Ay) | Stub | Stub | Stub |

## Test Coverage

10 tests covering:
- Simple BCD addition/subtraction
- Carry/borrow generation
- Extend flag propagation
- Low nibble correction (9+8=17)
- Negation and ten's complement
- Zero handling with Z flag

## Related Documentation

- [68000 Multi-Precision Arithmetic](68000-multi-precision-arithmetic.md) - Same X/Z flag patterns
- [68000 Micro-Op Architecture](68000-micro-op-architecture.md) - Overall 68000 patterns
