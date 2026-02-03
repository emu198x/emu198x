# 6502 Decimal Mode Test Setup

---
title: "Setting up Klaus Dormann's 6502 decimal mode test"
tags:
  - 6502
  - emulation
  - decimal-mode
  - bcd
  - testing
  - ca65
  - assembler
category: testing
module: emu-6502
symptoms:
  - "Need to verify BCD/decimal mode ADC/SBC implementation"
  - "AS65 assembler is Linux-only, won't run on macOS"
  - "Decimal test binary download fails or gets HTML"
root_cause: "Test source uses AS65 syntax which requires conversion to ca65 for cross-platform assembly"
date_solved: 2026-02-03
---

## Problem Description

Klaus Dormann's decimal mode test verifies the 6502's BCD (Binary-Coded Decimal) arithmetic. The original source uses AS65 assembler syntax, which is a Linux-only ELF binary that won't run on macOS.

## Solution: Convert to ca65 Syntax

The test source can be converted to ca65 (part of the cc65 toolchain) which runs on all platforms.

### Key Syntax Differences

| AS65 Syntax | ca65 Syntax |
|-------------|-------------|
| `bss` | `.segment "ZEROPAGE"` |
| `code` | `.segment "CODE"` |
| `org $addr` | `.org $addr` |
| `ds N` | `.res N` |
| `db $XX` | `.byte $XX` |
| `if cond ... endif` | `.if cond ... .endif` |
| `LABEL` | `LABEL:` (with colon) |
| `end START` | (no equivalent needed) |
| `macro ... endm` | (inline or convert to code) |

### Configuration File (decimal.cfg)

```
MEMORY {
    RAM: start = $0000, size = $10000, type = rw, file = %O, fill = yes;
}

SEGMENTS {
    ZEROPAGE: load = RAM, type = zp, start = $0000;
    CODE:     load = RAM, type = ro, start = $0200;
}
```

### Test Configuration Options

The test source has configuration flags:

```asm
cputype = 0         ; 0 = 6502, 1 = 65C02, 2 = 65C816
vld_bcd = 0         ; 0 = allow invalid bcd, 1 = valid bcd only
chk_a   = 1         ; check accumulator
chk_n   = 0         ; check sign (negative) flag
chk_v   = 0         ; check overflow flag
chk_z   = 0         ; check zero flag
chk_c   = 1         ; check carry flag
```

For NMOS 6502 testing, use `cputype = 0` and `vld_bcd = 0` to test all 256×256 input combinations including invalid BCD values.

### Trap Instruction

The original test uses `$DB` (65C02 STP instruction) as the trap. For NMOS 6502, change to a `JMP` self-loop:

```asm
; Original (65C02 only):
DONE    db $db    ; STP instruction

; For NMOS 6502:
DONE:   jmp DONE  ; Infinite loop as trap
```

### Assembly Commands

```bash
# Assemble
ca65 -o 6502_decimal_test.o 6502_decimal_test.s

# Link
ld65 -C decimal.cfg -o 6502_decimal_test.bin 6502_decimal_test.o
```

### Zero-Page Layout

The test uses 17 bytes of zero page starting at $00:

| Address | Variable | Purpose |
|---------|----------|---------|
| $00 | N1 | First operand |
| $01 | N2 | Second operand |
| $02 | HA | Binary arithmetic accumulator result |
| $03 | HNVZC | Binary arithmetic flags |
| $04 | DA | Decimal mode accumulator result |
| $05 | DNVZC | Decimal mode flags |
| $06 | AR | Predicted accumulator result |
| $07 | NF | Predicted N flag |
| $08 | VF | Predicted V flag |
| $09 | ZF | Predicted Z flag |
| $0A | CF | Predicted C flag |
| $0B | ERROR | Test result (0=pass, 1=fail) |
| $0C | N1L | N1 low nibble |
| $0D | N1H | N1 high nibble |
| $0E | N2L | N2 low nibble |
| $0F-$10 | N2H | N2 high nibble + offset |

### Test Harness Requirements

The test harness must:

1. **Load binary at $0000** - The zero-page variables are at $00-$10
2. **Start execution at $0200** - The CODE segment entry point
3. **Detect trap at instruction boundaries** - Not on every tick
4. **Check ERROR flag at $0B** - Returns 0 for pass, 1 for fail

**Critical**: Trap detection must check PC at instruction boundaries, not during mid-instruction execution. A `JMP $XXXX` instruction changes PC multiple times during execution (fetching the address bytes), so checking PC on every tick will never see the same PC twice in a row.

### Example Test Harness (Rust)

```rust
fn run_decimal_test(binary: &[u8]) -> bool {
    let mut bus = SimpleBus::new();
    bus.load(0x0000, binary);

    let mut cpu = Mos6502::new();
    cpu.regs.pc = 0x0200;  // Start at CODE segment

    let mut prev_pc: u16 = 0xFFFF;
    let mut same_pc_count = 0;

    loop {
        let start_pc = cpu.pc();

        // Detect trap at instruction boundaries
        if start_pc == prev_pc {
            same_pc_count += 1;
            if same_pc_count > 2 {
                let error = bus.peek(0x000B);
                return error == 0;
            }
        } else {
            same_pc_count = 0;
            prev_pc = start_pc;
        }

        // Execute complete instruction
        cpu.tick(&mut bus);
        while !cpu.is_instruction_complete() {
            cpu.tick(&mut bus);
        }
    }
}
```

## Expected Results

For NMOS 6502 with `vld_bcd=0` (invalid BCD allowed):

- **~14.5 million instructions**
- **~46 million cycles**
- Tests all 65,536 combinations of N1×N2 for both addition and subtraction
- Tests with both carry flag states (Y=0 and Y=1)

## Troubleshooting

### Test loops forever without trapping

The trap detection checks PC too frequently (every tick instead of at instruction boundaries). Fix by checking PC only when `is_instruction_complete()` returns true.

### ERROR flag is 1 (test failed)

Dump the test state to see which operands failed:
- N1 at $00, N2 at $01 - the operands
- DA at $04 - actual decimal result
- AR at $06 - predicted result
- DNVZC at $05 - actual flags
- CF at $0A - predicted carry

### Binary download fails

The GitHub raw URLs sometimes serve HTML. Assemble locally using ca65/ld65 instead.

## Related Documentation

- [6502 BRK stale address bug](../logic-errors/6502-brk-stale-addr-vector.md)
- [Klaus Dormann's original test](http://www.6502.org/tutorials/decimal_mode.html)
- [cc65 toolchain documentation](https://cc65.github.io/doc/)
