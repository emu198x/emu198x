---
title: "68000 JMP/JSR Opcode Decode Masked by TRAP/LINK/UNLK Dispatch"
category: logic-errors
tags:
  - m68000
  - cpu-emulation
  - opcode-decode
  - instruction-dispatch
  - bit-field-parsing
  - rust
module: emu-68000
symptoms:
  - JMP instruction does not update PC
  - JSR instruction does not update PC or push return address
  - PC remains at instruction following JMP/JSR instead of target
  - Tests pass for TRAP, LINK, UNLK but fail for JMP, JSR
severity: critical
date: 2026-02-03
---

# 68000 JMP/JSR Opcode Decode Masked by TRAP/LINK/UNLK Dispatch

## Problem

JMP and JSR instructions were silently ignored - the PC was not updated to the target address. Tests showed PC at 0x1002 (after the instruction) instead of the expected 0x2000 (the target).

## Root Cause

In `decode_group_4`, opcodes with bits 11-8 = 0xE (0x4Exx) were dispatched to a TRAP/LINK/UNLK handler. However, JMP and JSR also fall in this range:

| Instruction | Opcode Range | Bits 11-8 | Bits 7-6 |
|-------------|--------------|-----------|----------|
| TRAP        | 0x4E40-0x4E4F| 0xE       | 01       |
| LINK        | 0x4E50-0x4E57| 0xE       | 01       |
| UNLK        | 0x4E58-0x4E5F| 0xE       | 01       |
| JSR         | 0x4E80-0x4EBF| 0xE       | 10       |
| JMP         | 0x4EC0-0x4EFF| 0xE       | 11       |

The original code only checked bits 11-8, so JMP (0x4EFF for JMP $2000.L) fell through to the TRAP/LINK/UNLK handler and was ignored.

## Before (Broken)

```rust
fn decode_group_4(&mut self, op: u16) {
    let mode = ((op >> 3) & 7) as u8;
    let ea_reg = (op & 7) as u8;

    match (op >> 8) & 0xF {
        // ... other cases ...
        0xE => {
            // TRAP, LINK, UNLK, MOVE USP, misc
            match (op >> 4) & 0xF {
                0x4 => self.exec_trap((op & 0xF) as u8),
                0x5 if op & 0x8 == 0 => self.exec_link(ea_reg),
                0x5 => self.exec_unlk(ea_reg),
                // JMP and JSR never reached!
                _ => {}
            }
        }
        // ... other cases ...
    }
}
```

## After (Fixed)

```rust
fn decode_group_4(&mut self, op: u16) {
    let mode = ((op >> 3) & 7) as u8;
    let ea_reg = (op & 7) as u8;

    match (op >> 8) & 0xF {
        // ... other cases ...
        0xE => {
            // JSR, JMP, TRAP, LINK, UNLK, MOVE USP, etc.
            // Check bits 7-6 FIRST to distinguish JMP/JSR from others
            let subop = (op >> 6) & 3;
            if subop == 2 {
                // JSR <ea> (0x4E80-0x4EBF)
                self.exec_jsr(mode, ea_reg);
            } else if subop == 3 {
                // JMP <ea> (0x4EC0-0x4EFF)
                self.exec_jmp(mode, ea_reg);
            } else {
                // TRAP, LINK, UNLK, MOVE USP, misc (0x4E00-0x4E7F)
                match (op >> 4) & 0xF {
                    0x4 => self.exec_trap((op & 0xF) as u8),
                    0x5 if op & 0x8 == 0 => self.exec_link(ea_reg),
                    0x5 => self.exec_unlk(ea_reg),
                    _ => {}
                }
            }
        }
        // ... other cases ...
    }
}
```

## Key Insight

68000 opcode decoding is hierarchical by bit fields, but those fields aren't always aligned with logical instruction groupings. When multiple instructions share the same high-order bits, you must check distinguishing low-order bits **before** dispatching to a catch-all handler.

## Prevention Strategies

1. **Decode order matters**: Check more specific bit patterns before general ones within the same case.

2. **Document opcode ranges**: Add comments showing the full opcode range each case handles, including edge cases.

3. **Test representative instructions**: When adding a new instruction group (TRAP/LINK/UNLK), also verify that nearby opcodes (JMP/JSR) still work.

4. **Visualize bit fields**: Draw out the opcode bit layout to see which bits distinguish instructions:
   ```
   0x4Exx: 0100 1110 xxxx xxxx
                     ^^ bits 7-6 distinguish JMP/JSR/misc
   ```

5. **Use exhaustive matching**: Instead of catch-all `_ => {}`, enumerate all valid cases and panic on truly invalid opcodes during development.

6. **Reference canonical docs**: The M68000 Programmer's Reference Manual groups opcodes by encoding, not mnemonic. Follow its decode structure.

7. **Cross-check with other emulators**: Musashi, UAE, and other 68000 emulators have battle-tested decode logic to reference.

## Test Case

```rust
#[test]
fn test_jmp_absolute_long() {
    let mut cpu = M68000::new();
    let mut bus = TestBus::new();

    // JMP $00002000 (absolute long)
    bus.load_words(0x1000, &[0x4EF9, 0x0000, 0x2000]);
    cpu.reset_to(&mut bus, 0x1000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.pc, 0x2000, "PC should be at jump target");
}
```

## Related

- `docs/solutions/implementation/68000-micro-op-architecture.md` - Overall 68000 implementation patterns
- M68000 Programmer's Reference Manual, Section 8 - Instruction Set Summary
