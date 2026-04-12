# 6510 Microprocessor Quick Reference

**Purpose:** Quick instruction set lookup for lesson creation
**For comprehensive hardware details:** See 6510-MICROPROCESSOR-REFERENCE.md
**Audience:** Curriculum designers creating C64 assembly lessons

---

## Registers

The 6510 has 6 registers:

| Register | Size | Purpose |
|----------|------|---------|
| **A** (Accumulator) | 8-bit | Main data register for arithmetic/logic |
| **X** (Index) | 8-bit | Counter, offset, loop control |
| **Y** (Index) | 8-bit | Counter, offset, loop control |
| **PC** (Program Counter) | 16-bit | Address of next instruction |
| **SP** (Stack Pointer) | 8-bit | Points to top of stack ($0100-$01FF) |
| **P** (Status) | 8-bit | Processor status flags (see below) |

### Status Register Flags (P)

```
Bit 7: N (Negative)   - Set if result is negative (bit 7 = 1)
Bit 6: V (Overflow)   - Set on signed overflow
Bit 5: - (Unused)     - Always 1
Bit 4: B (Break)      - Set when BRK instruction executed
Bit 3: D (Decimal)    - Decimal mode (BCD arithmetic)
Bit 2: I (Interrupt)  - Interrupt disable (1 = disabled)
Bit 1: Z (Zero)       - Set if result is zero
Bit 0: C (Carry)      - Set on carry/borrow from bit 7
```

---

## Addressing Modes

| Mode | Example | Description | Bytes |
|------|---------|-------------|-------|
| **Implied** | `TAX` | No operand needed | 1 |
| **Accumulator** | `LSR A` | Operate on accumulator | 1 |
| **Immediate** | `LDA #$05` | Use literal value | 2 |
| **Zero Page** | `LDA $80` | Address $00-$FF (faster) | 2 |
| **Zero Page,X** | `LDA $80,X` | Zero page + X register | 2 |
| **Zero Page,Y** | `LDX $80,Y` | Zero page + Y register | 2 |
| **Absolute** | `LDA $C000` | Full 16-bit address | 3 |
| **Absolute,X** | `LDA $C000,X` | Absolute address + X | 3 |
| **Absolute,Y** | `LDA $C000,Y` | Absolute address + Y | 3 |
| **Indirect** | `JMP ($0080)` | Address stored at pointer | 3 |
| **Indexed Indirect** | `LDA ($80,X)` | Zero page pointer + X, then load | 2 |
| **Indirect Indexed** | `LDA ($80),Y` | Zero page pointer, then + Y (common) | 2 |
| **Relative** | `BNE LABEL` | Branch offset (-128 to +127) | 2 |

---

## Instruction Set by Category

### Load and Store

| Instruction | Flags | Description |
|-------------|-------|-------------|
| **LDA** | NZ | Load Accumulator |
| **LDX** | NZ | Load X Register |
| **LDY** | NZ | Load Y Register |
| **STA** | - | Store Accumulator |
| **STX** | - | Store X Register |
| **STY** | - | Store Y Register |

**Examples:**
```assembly
LDA #$00        ; Load A with 0
LDX $FB         ; Load X from zero page $FB
LDY $C000,X     ; Load Y from $C000 + X
STA $D020       ; Store A to border color
```

### Transfer Between Registers

| Instruction | Flags | Description |
|-------------|-------|-------------|
| **TAX** | NZ | Transfer A to X |
| **TAY** | NZ | Transfer A to Y |
| **TXA** | NZ | Transfer X to A |
| **TYA** | NZ | Transfer Y to A |
| **TSX** | NZ | Transfer Stack Pointer to X |
| **TXS** | - | Transfer X to Stack Pointer |

**Examples:**
```assembly
LDA #5
TAX             ; X = 5
TXA             ; A = 5 (copy X back to A)
```

### Arithmetic

| Instruction | Flags | Description |
|-------------|-------|-------------|
| **ADC** | NZCV | Add with Carry |
| **SBC** | NZCV | Subtract with Carry |
| **INC** | NZ | Increment Memory |
| **INX** | NZ | Increment X |
| **INY** | NZ | Increment Y |
| **DEC** | NZ | Decrement Memory |
| **DEX** | NZ | Decrement X |
| **DEY** | NZ | Decrement Y |

**Examples:**
```assembly
CLC             ; Clear carry before add
LDA #5
ADC #3          ; A = 8

SEC             ; Set carry before subtract
LDA #10
SBC #3          ; A = 7

INC $D020       ; Increment border color
INX             ; Increment X register
```

### Logic

| Instruction | Flags | Description |
|-------------|-------|-------------|
| **AND** | NZ | Logical AND |
| **ORA** | NZ | Logical OR |
| **EOR** | NZ | Logical XOR (Exclusive OR) |
| **BIT** | NZV | Test Bits (doesn't change A) |

**Examples:**
```assembly
LDA #%11110000
AND #%00001111  ; A = %00000000 (mask off high bits)

LDA $DC00
AND #$1F        ; Mask joystick bits

LDA $D011
ORA #%00100000  ; Set bit 5 (bitmap mode)

EOR #$FF        ; Invert all bits (NOT operation)
```

### Shifts and Rotates

| Instruction | Flags | Description |
|-------------|-------|-------------|
| **ASL** | NZC | Arithmetic Shift Left |
| **LSR** | NZC | Logical Shift Right |
| **ROL** | NZC | Rotate Left through Carry |
| **ROR** | NZC | Rotate Right through Carry |

**Examples:**
```assembly
LDA #%00000001
ASL             ; A = %00000010 (multiply by 2)
ASL             ; A = %00000100 (multiply by 4)

LDA #%10000000
LSR             ; A = %01000000 (divide by 2)

; Rotate 16-bit value left
ROL $FB         ; Low byte
ROL $FC         ; High byte (carry from low byte)
```

### Comparisons

| Instruction | Flags | Description |
|-------------|-------|-------------|
| **CMP** | NZC | Compare with Accumulator |
| **CPX** | NZC | Compare with X |
| **CPY** | NZC | Compare with Y |

**Branch after compare:**
- **BEQ** - Branch if Equal (Z=1)
- **BNE** - Branch if Not Equal (Z=0)
- **BCC** - Branch if Less Than (C=0)
- **BCS** - Branch if Greater or Equal (C=1)

**Examples:**
```assembly
LDA $DC00       ; Read joystick
AND #$10        ; Fire button
BEQ FIRE_PRESSED ; Branch if 0 (pressed)

LDX #10
LOOP:
    ; Do something
    DEX
    BNE LOOP    ; Loop while X != 0

CMP #100        ; Compare A with 100
BCC LESS_THAN   ; Branch if A < 100
BEQ EQUAL       ; Branch if A = 100
; Otherwise A > 100
```

### Branches (Conditional Jumps)

| Instruction | Condition | Description |
|-------------|-----------|-------------|
| **BEQ** | Z=1 | Branch if Equal (zero) |
| **BNE** | Z=0 | Branch if Not Equal |
| **BCC** | C=0 | Branch if Carry Clear |
| **BCS** | C=1 | Branch if Carry Set |
| **BMI** | N=1 | Branch if Minus (negative) |
| **BPL** | N=0 | Branch if Plus (positive) |
| **BVC** | V=0 | Branch if Overflow Clear |
| **BVS** | V=1 | Branch if Overflow Set |

**Range:** -128 to +127 bytes from the branch instruction

**Examples:**
```assembly
LDA $DC00
AND #$01        ; Up
BEQ MOVE_UP     ; If pressed, branch

LDA COUNTER
CMP #10
BNE NOT_TEN     ; Branch if not equal to 10
```

### Jumps and Subroutines

| Instruction | Description |
|-------------|-------------|
| **JMP abs** | Jump to absolute address |
| **JMP (ind)** | Jump to address stored in pointer |
| **JSR** | Jump to Subroutine (pushes return address) |
| **RTS** | Return from Subroutine (pops return address) |

**Examples:**
```assembly
JMP $C000       ; Jump to $C000 (never returns)

JSR MY_SUB      ; Call subroutine (will return here)
; Continue after JSR

MY_SUB:
    ; Subroutine code
    RTS         ; Return to caller

; Indirect jump (vector)
JMP ($0314)     ; Jump to address stored at $0314-$0315
```

### Stack Operations

| Instruction | Description |
|-------------|-------------|
| **PHA** | Push Accumulator |
| **PLA** | Pull Accumulator |
| **PHP** | Push Processor Status |
| **PLP** | Pull Processor Status |

**Examples:**
```assembly
; Save A before KERNAL call
PHA             ; Push A to stack
JSR $FFD2       ; CHROUT (might change A)
PLA             ; Pull A back from stack

; Save all registers
PHA
TXA
PHA
TYA
PHA
; Do something
; Restore in reverse order
PLA
TAY
PLA
TAX
PLA
```

### Flag Control

| Instruction | Description |
|-------------|-------------|
| **CLC** | Clear Carry (C=0) |
| **SEC** | Set Carry (C=1) |
| **CLD** | Clear Decimal mode (D=0) |
| **SED** | Set Decimal mode (D=1) |
| **CLI** | Clear Interrupt disable (enable IRQs) |
| **SEI** | Set Interrupt disable (disable IRQs) |
| **CLV** | Clear Overflow (V=0) |

**Examples:**
```assembly
CLC             ; Always clear carry before ADC
ADC #5

SEC             ; Always set carry before SBC
SBC #3

SEI             ; Disable interrupts (for critical code)
; Critical section
CLI             ; Re-enable interrupts

CLD             ; Ensure binary mode (C64 default)
```

### Other

| Instruction | Description |
|-------------|-------------|
| **NOP** | No Operation (do nothing) |
| **BRK** | Break (software interrupt) |
| **RTI** | Return from Interrupt |

---

## Common Instruction Patterns

### Pattern 1: 16-Bit Addition

```assembly
; Add 16-bit value in $FB-$FC to value in $FD-$FE
; Result in $FB-$FC
CLC             ; Clear carry first
LDA $FB         ; Low byte
ADC $FD
STA $FB

LDA $FC         ; High byte
ADC $FE         ; Add with carry from low byte
STA $FC
```

### Pattern 2: Clear Memory Block

```assembly
; Clear $C000-$C0FF (256 bytes)
LDA #0          ; Value to fill
LDX #0          ; Counter
CLEAR_LOOP:
    STA $C000,X ; Store 0 at $C000 + X
    INX
    BNE CLEAR_LOOP ; Loop 256 times (0-255)
```

### Pattern 3: Copy Memory Block

```assembly
; Copy 256 bytes from $C000 to $D000
LDX #0
COPY_LOOP:
    LDA $C000,X ; Load from source
    STA $D000,X ; Store to destination
    INX
    BNE COPY_LOOP
```

### Pattern 4: Delay Loop

```assembly
; Simple delay
LDX #$FF
DELAY:
    DEX
    BNE DELAY
; Delay complete (256 iterations)
```

### Pattern 5: 16-Bit Comparison

```assembly
; Compare 16-bit value at $FB-$FC with $FD-$FE
; Branch to GREATER if $FB-$FC > $FD-$FE

LDA $FC         ; High byte first
CMP $FE
BCC LESS        ; High byte less? Then value is less
BNE GREATER     ; High byte greater? Then value is greater
; High bytes equal, check low bytes
LDA $FB
CMP $FD
BCC LESS
BEQ EQUAL
GREATER:
    ; FB-FC > FD-FE
EQUAL:
    ; FB-FC = FD-FE
LESS:
    ; FB-FC < FD-FE
```

---

## Cycle Counts (for timing)

**Common instructions:**

| Instruction | Mode | Cycles |
|-------------|------|--------|
| LDA #$xx | Immediate | 2 |
| LDA $xx | Zero Page | 3 |
| LDA $xxxx | Absolute | 4 |
| LDA $xxxx,X | Absolute,X | 4* |
| STA $xx | Zero Page | 3 |
| STA $xxxx | Absolute | 4 |
| JMP $xxxx | Absolute | 3 |
| JSR $xxxx | Absolute | 6 |
| RTS | Implied | 6 |
| BNE label | Relative | 2** |
| INC $xx | Zero Page | 5 |
| INX | Implied | 2 |
| NOP | Implied | 2 |

*Add 1 cycle if page boundary crossed
**Add 1 cycle if branch taken

**C64 clock speed:** 1.022727 MHz (NTSC) ≈ 1 cycle per microsecond

---

## Lesson Design Progression

### Beginner (Lessons 1-5)

**Focus on:**
- LDA, STA with immediate and absolute addressing
- Simple register operations (TAX, INX)
- Basic comparisons and branches
- RTS (critical!)

**Example:**
```assembly
LDA #5          ; Load immediate
STA $D020       ; Store to border
RTS
```

### Intermediate (Lessons 6-10)

**Add:**
- Indexed addressing (LDA $C000,X)
- Loops with counters
- Arithmetic (ADC, SBC)
- Logic operations (AND, ORA)

**Example:**
```assembly
LDX #0
LOOP:
    LDA DATA,X
    STA $0400,X
    INX
    CPX #40
    BNE LOOP
RTS
```

### Advanced (Lessons 11-15)

**Add:**
- Zero page indirect addressing
- 16-bit arithmetic
- Stack operations
- Subroutine calls

**Example:**
```assembly
; Print string using pointer
LDA #<MESSAGE
STA $FB
LDA #>MESSAGE
STA $FC

LDY #0
PRINT:
    LDA ($FB),Y
    BEQ DONE
    JSR $FFD2   ; CHROUT
    INY
    BNE PRINT
DONE:
    RTS
```

---

## Common Mistakes

| Problem | Cause | Fix |
|---------|-------|-----|
| **Infinite loop** | Forgot RTS | Always end with RTS |
| **Wrong branch** | Used BCC instead of BCS | Check carry flag meaning |
| **Decimal mode bugs** | SED not cleared | Use CLD at start |
| **Comparison backwards** | Wrong branch after CMP | A < value = BCC, A >= value = BCS |
| **16-bit overflow** | Forgot CLC before add | Always CLC before ADC |
| **Page boundary** | Crossed $xxFF boundary | Use zero page or add cycle |

---

## Quick Reference Cards

### Load/Store/Transfer
```
LDA LDX LDY     ; Load
STA STX STY     ; Store
TAX TAY TXA TYA ; Transfer
```

### Arithmetic
```
ADC SBC         ; Add/Subtract with carry
INC DEC         ; Increment/Decrement memory
INX INY         ; Increment X/Y
DEX DEY         ; Decrement X/Y
```

### Logic
```
AND ORA EOR     ; AND, OR, XOR
BIT             ; Test bits
ASL LSR         ; Shift left/right
ROL ROR         ; Rotate left/right
```

### Control Flow
```
JMP JSR RTS     ; Jump, call, return
BEQ BNE         ; Branch if equal/not equal
BCC BCS         ; Branch if carry clear/set
BMI BPL         ; Branch if minus/plus
```

### Flags
```
CLC SEC         ; Clear/Set carry
CLI SEI         ; Clear/Set interrupt disable
CLD SED         ; Clear/Set decimal mode
CLV             ; Clear overflow
```

---

## Opcode Chart Summary

**See 6510-MICROPROCESSOR-REFERENCE.md for complete opcode table with hex values.**

Total valid opcodes: **151** (out of 256 possible)

**Undocumented/illegal opcodes:** Exist but not recommended for lessons

---

## See Also

- **6510-MICROPROCESSOR-REFERENCE.md** - Complete opcode table, timing, hardware specs
- **C64-MACHINE-LANGUAGE-OVERVIEW.md** - ML concepts and lesson design
- **BASIC-TO-ML-INTEGRATION.md** - Calling ML from BASIC
- **C64-MEMORY-MAP.md** - Memory layout and safe locations

---

**Document Version:** 1.0
**Synthesized:** 2025 for Code Like It's 198x curriculum
**Focus:** Instruction set and programming patterns only
