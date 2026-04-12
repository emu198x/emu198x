# C64 Machine Language Overview

**Reference Source:** C64 Programmer's Reference Guide, Chapter 5: BASIC to Machine Language
**Purpose:** Educational reference for lesson creation and verification
**Audience:** Curriculum designers creating C64 assembly language lessons

---

## What is Machine Language?

Machine language is the native instruction set of the 6510 microprocessor. Unlike BASIC (which is interpreted line-by-line), machine language executes directly on the CPU at hardware speed.

**Key Concepts:**

- **Speed**: Machine language runs ~100x faster than BASIC
- **Direct hardware access**: Full control over VIC-II, SID, CIA chips
- **Memory efficiency**: Compact code compared to BASIC
- **No interpreter overhead**: Instructions execute directly

---

## The 6510 Microprocessor

The C64's CPU is a modified 6502 with an on-chip I/O port.

### Registers (6 total)

| Register | Size | Purpose |
|----------|------|---------|
| **A** (Accumulator) | 8-bit | Primary data register for arithmetic/logic |
| **X** (Index) | 8-bit | Counter, offset, temporary storage |
| **Y** (Index) | 8-bit | Counter, offset, temporary storage |
| **PC** (Program Counter) | 16-bit | Address of next instruction |
| **SP** (Stack Pointer) | 8-bit | Points to top of stack (page 1: $0100-$01FF) |
| **P** (Status/Flags) | 8-bit | Processor status flags |

### Status Register Flags

```
Bit 7: N (Negative) - Set if result is negative (bit 7 = 1)
Bit 6: V (Overflow) - Set on signed arithmetic overflow
Bit 5: - (Unused)
Bit 4: B (Break) - Set when BRK instruction executed
Bit 3: D (Decimal) - Decimal mode flag (BCD arithmetic)
Bit 2: I (Interrupt Disable) - When set, IRQs ignored
Bit 1: Z (Zero) - Set if result is zero
Bit 0: C (Carry) - Set on carry/borrow from bit 7
```

---

## Addressing Modes

The 6510 supports 13 addressing modes determining how instructions access data:

### Common Modes

| Mode | Example | Description |
|------|---------|-------------|
| **Immediate** | `LDA #$05` | Use literal value |
| **Zero Page** | `LDA $80` | Address $0000-$00FF (faster) |
| **Absolute** | `LDA $C000` | Full 16-bit address |
| **Indexed (X)** | `LDA $C000,X` | Address + X register |
| **Indexed (Y)** | `LDA $C000,Y` | Address + Y register |
| **Indirect** | `JMP ($0080)` | Address stored at pointer |
| **Indexed Indirect** | `LDA ($80,X)` | Zero page pointer + X |
| **Indirect Indexed** | `LDA ($80),Y` | Pointer + Y (common) |

**Lesson Design Tip:** Introduce immediate and absolute first, then zero page, then indexed modes progressively.

---

## The Stack

- **Location**: $0100-$01FF (page 1)
- **Type**: Descending (grows downward)
- **Stack Pointer**: Points to next free location
- **Operations**:
  - `PHA` - Push accumulator
  - `PLA` - Pull accumulator
  - `PHP` - Push status
  - `PLP` - Pull status
  - `JSR` - Pushes return address (PC+2)
  - `RTS` - Pulls return address
  - `RTI` - Pulls status + return address

**Stack Requirements:** KERNAL routines document their stack usage (2-13 bytes). Always ensure adequate stack space.

---

## Hexadecimal Notation

All C64 addresses and many values use hexadecimal (base 16):

```
Decimal:     0  1  2  3  4  5  6  7  8  9  10  11  12  13  14  15
Hexadecimal: 0  1  2  3  4  5  6  7  8  9   A   B   C   D   E   F
```

**Conversions:**
- Decimal 53248 = Hex $D000 (VIC-II base)
- Decimal 54272 = Hex $D400 (SID base)
- Decimal 65535 = Hex $FFFF (maximum 16-bit value)

**Notation:**
- `$D000` - Commodore/assembly convention
- `0xD000` - C/modern convention
- `D000` (hex) - Documentation convention

---

## The KERNAL

The KERNAL is the C64's operating system, located at $E000-$FFFF (57344-65535).

### Jump Table Design

**Key Insight:** KERNAL routines use a jump table at $FF81-$FFF3 with fixed entry points. This allows Commodore to update KERNAL internals without breaking compatibility - programs always call the same address.

### Device Numbers

| Device | Number | Description |
|--------|--------|-------------|
| Keyboard | 0 | Default input device |
| Datasette | 1 | Cassette tape drive |
| RS-232 | 2 | Serial communications |
| Screen | 3 | Default output device |
| Printer | 4 | Serial bus printer |
| Disk | 8 | Serial bus disk drive (typically) |

### File I/O Pattern

Standard sequence for file operations:

```assembly
; 1. Set up logical file
LDA #1          ; Logical file number
LDX #8          ; Device number (disk)
LDY #15         ; Secondary address (command channel)
JSR SETLFS      ; $FFBA

; 2. Set filename
LDA #4          ; Name length
LDX #<FNAME     ; Name address (low)
LDY #>FNAME     ; Name address (high)
JSR SETNAM      ; $FFBD

; 3. Open file
JSR OPEN        ; $FFC0

; 4. Set input/output channel
LDX #1          ; Logical file number
JSR CHKIN       ; $FFC6 (input) or CHKOUT $FFC9 (output)

; 5. Read/write data
JSR CHRIN       ; $FFCF (input) or CHROUT $FFD2 (output)

; 6. Close channel and file
JSR CLRCHN      ; $FFCC
LDA #1
JSR CLOSE       ; $FFC3
```

---

## Error Handling

KERNAL routines return errors two ways:

### 1. Accumulator + Carry Flag

```assembly
JSR ROUTINE
BCS ERROR       ; Branch if carry set (error occurred)
; Carry clear = success
```

Error codes in accumulator:
- 0: Routine terminated by STOP key
- 1: Too many open files
- 2: File already open
- 3: File not open
- 4: File not found
- 5: Device not present
- 6: Not an input file
- 7: Not an output file
- 8: Missing file name
- 9: Illegal device number

### 2. Status Word (ST)

For I/O operations, call `READST` ($FFB7):

```assembly
JSR READST
AND #$40        ; Check bit 6 (EOF)
BNE EOF_REACHED
```

Status bits documented in KERNAL-ROUTINES-REFERENCE.md.

---

## Memory Organization

### For Machine Language Programs

**Best location:** $C000-$CFFF (49152-53247)
- 4K of RAM
- Not used by BASIC
- Safe for ML routines

**For larger programs:** Reserve memory from top:

```basic
POKE 51,LO: POKE 52,HI: POKE 55,LO: POKE 56,HI: CLR
```

Example - reserve $9000-$9FFF:
```basic
10 POKE 51,0: POKE 52,144: POKE 55,0: POKE 56,144: CLR
```

---

## For Lesson Creation

### Progressive Skill Building

1. **Lesson 1-3:** Basic concepts, registers, simple instructions (LDA, STA, RTS)
2. **Lesson 4-6:** Addressing modes, zero page, absolute
3. **Lesson 7-9:** Branches, loops, comparisons
4. **Lesson 10-12:** Subroutines, stack, parameters
5. **Lesson 13-15:** KERNAL usage, file I/O
6. **Lesson 16+:** Hardware (VIC-II, SID, CIA)

### Example Complexity Levels

**Simple (Early lessons):**
```assembly
LDA #$05        ; Load 5
STA $0400       ; Store to screen
RTS             ; Return to BASIC
```

**Intermediate:**
```assembly
LDX #0          ; Counter
LOOP:
LDA DATA,X      ; Load from table
STA $0400,X     ; Store to screen
INX
CPX #40         ; Compare to 40
BNE LOOP        ; Loop if not equal
RTS
```

**Advanced:**
```assembly
; Use KERNAL to print string
LDA #<MSG
STA $FB
LDA #>MSG
STA $FC
LDY #0
LOOP:
LDA ($FB),Y
BEQ DONE
JSR CHROUT      ; $FFD2
INY
BNE LOOP
DONE:
RTS
```

---

## Common Pitfalls for Beginners

1. **Forgetting RTS** - Always end ML routines with RTS when called from BASIC
2. **Zero page conflicts** - Locations $00-$FF used by system; use $FB-$FE for user pointers
3. **Stack overflow** - Nested JSRs without balancing RTS
4. **Decimal mode** - SED sets decimal mode; use CLD to clear
5. **Register preservation** - Some KERNAL routines affect X/Y; save if needed

---

## Quick Reference: Common Operations

### Load and Store
```assembly
LDA #$00        ; Load accumulator with zero
LDX #$00        ; Load X with zero
LDY #$00        ; Load Y with zero
STA $D020       ; Store A to address
STX $D021       ; Store X to address
STY $0400       ; Store Y to address
```

### Arithmetic
```assembly
CLC             ; Clear carry before add
ADC #$05        ; Add 5 to accumulator
SEC             ; Set carry before subtract
SBC #$02        ; Subtract 2 from accumulator
```

### Comparisons and Branches
```assembly
CMP #$10        ; Compare A with 16
BEQ EQUAL       ; Branch if equal (Z=1)
BNE NOTEQUAL    ; Branch if not equal (Z=0)
BCC LESS        ; Branch if less (C=0)
BCS GREATER     ; Branch if greater/equal (C=1)
```

### Subroutines
```assembly
JSR ROUTINE     ; Jump to subroutine
RTS             ; Return from subroutine
```

---

## See Also

- **KERNAL-ROUTINES-REFERENCE.md** - All 39 KERNAL routines
- **C64-MEMORY-MAP.md** - Complete memory layout
- **BASIC-TO-ML-INTEGRATION.md** - Calling ML from BASIC
- **PETSCII-SCREEN-CODES.md** - Character codes

---

**Document Version:** 1.0
**Source Material:** Commodore 64 Programmer's Reference Guide (1982)
**Synthesized:** 2025 for Code Like It's 198x curriculum
