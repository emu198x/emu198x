# 6502 Microprocessor Quick Reference (NES)

**Purpose:** Quick instruction set lookup for NES assembly programming
**Audience:** NES curriculum designers and students learning 6502 assembly
**Note:** NES uses standard 6502 (no decimal mode like C64's 6510)

---

## Registers

The 6502 has 6 registers:

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
Bit 3: D (Decimal)    - Decimal mode (NOT USED ON NES - always 0)
Bit 2: I (Interrupt)  - Interrupt disable (1 = disabled)
Bit 1: Z (Zero)       - Set if result is zero
Bit 0: C (Carry)      - Set on carry/borrow from bit 7
```

**NES Note:** The D (Decimal) flag is non-functional on NES. Use binary arithmetic only.

---

## Addressing Modes

| Mode | Example | Description | Bytes | Cycles |
|------|---------|-------------|-------|--------|
| **Implied** | `TAX` | No operand needed | 1 | 2 |
| **Accumulator** | `LSR A` | Operate on accumulator | 1 | 2 |
| **Immediate** | `LDA #$05` | Use literal value | 2 | 2 |
| **Zero Page** | `LDA $80` | Address $00-$FF (fast) | 2 | 3 |
| **Zero Page,X** | `LDA $80,X` | Zero page + X register | 2 | 4 |
| **Zero Page,Y** | `LDX $80,Y` | Zero page + Y register | 2 | 4 |
| **Absolute** | `LDA $C000` | Full 16-bit address | 3 | 4 |
| **Absolute,X** | `LDA $C000,X` | Absolute address + X | 3 | 4+ |
| **Absolute,Y** | `LDA $C000,Y` | Absolute address + Y | 3 | 4+ |
| **Indirect** | `JMP ($0080)` | Address stored at pointer | 3 | 5 |
| **Indexed Indirect** | `LDA ($80,X)` | ZP pointer + X, then load | 2 | 6 |
| **Indirect Indexed** | `LDA ($80),Y` | ZP pointer, then + Y | 2 | 5+ |
| **Relative** | `BNE LABEL` | Branch offset (-128 to +127) | 2 | 2+ |

**Note:** + indicates extra cycle if page boundary crossed or branch taken

---

## Instruction Set by Category

### Load and Store

| Instruction | Flags | Cycles | Description |
|-------------|-------|--------|-------------|
| **LDA** | NZ | 2-6 | Load Accumulator |
| **LDX** | NZ | 2-6 | Load X Register |
| **LDY** | NZ | 2-6 | Load Y Register |
| **STA** | - | 3-6 | Store Accumulator |
| **STX** | - | 3-5 | Store X Register |
| **STY** | - | 3-5 | Store Y Register |

**Examples:**
```asm
LDA #$00        ; Load A with 0 (2 cycles)
LDX $FB         ; Load X from zero page $FB (3 cycles)
LDY $C000,X     ; Load Y from $C000 + X (4+ cycles)
STA $2007       ; Store A to PPUDATA (4 cycles)
```

**NES Common Usage:**
```asm
; Setting PPU address
LDA #$3F
STA $2006       ; PPUADDR high byte
LDA #$00
STA $2006       ; PPUADDR low byte

; Writing to OAM buffer
LDA paddle_y
STA $0200       ; Sprite 0 Y position
```

### Transfer Between Registers

| Instruction | Flags | Cycles | Description |
|-------------|-------|--------|-------------|
| **TAX** | NZ | 2 | Transfer A to X |
| **TAY** | NZ | 2 | Transfer A to Y |
| **TXA** | NZ | 2 | Transfer X to A |
| **TYA** | NZ | 2 | Transfer Y to A |
| **TSX** | NZ | 2 | Transfer Stack Pointer to X |
| **TXS** | - | 2 | Transfer X to Stack Pointer |

**Examples:**
```asm
LDA #5
TAX             ; X = 5 (2 cycles)
TXA             ; A = 5 (copy X back to A)
```

**NES Common Usage:**
```asm
; Controller reading loop
LDX #$08        ; 8 buttons to read
ReadLoop:
    LDA $4016   ; Read button
    LSR         ; Shift into carry
    ROL buttons ; Rotate into buttons variable
    DEX
    BNE ReadLoop
```

### Arithmetic

| Instruction | Flags | Cycles | Description |
|-------------|-------|--------|-------------|
| **ADC** | NZCV | 2-6 | Add with Carry |
| **SBC** | NZCV | 2-6 | Subtract with Carry |
| **INC** | NZ | 5-7 | Increment Memory |
| **INX** | NZ | 2 | Increment X |
| **INY** | NZ | 2 | Increment Y |
| **DEC** | NZ | 5-7 | Decrement Memory |
| **DEX** | NZ | 2 | Decrement X |
| **DEY** | NZ | 2 | Decrement Y |

**Examples:**
```asm
; Adding two numbers
CLC             ; Clear carry before add
LDA #10
ADC #5          ; A = 15

; Subtracting
SEC             ; Set carry before subtract
LDA #20
SBC #7          ; A = 13

; Incrementing/decrementing
INX             ; X = X + 1
DEY             ; Y = Y - 1
INC $0200       ; Increment sprite Y position
```

**NES Common Usage:**
```asm
; Moving sprite
LDA sprite_y
CLC
ADC #2          ; Move down 2 pixels
STA sprite_y

; Boundary check with subtraction
LDA sprite_y
SEC
SBC #8          ; Check if < 8
BCC too_high    ; Branch if result negative
```

### Logical Operations

| Instruction | Flags | Cycles | Description |
|-------------|-------|--------|-------------|
| **AND** | NZ | 2-6 | Logical AND |
| **ORA** | NZ | 2-6 | Logical OR |
| **EOR** | NZ | 2-6 | Logical XOR |
| **BIT** | NZV | 3-4 | Test Bits (doesn't change A) |

**Examples:**
```asm
; Masking bits
LDA buttons
AND #%10000000  ; Isolate A button (bit 7)
BEQ not_pressed

; Setting bits
LDA $2000
ORA #%10000000  ; Enable NMI (bit 7)
STA $2000

; Toggling bits
LDA sprite_attr
EOR #%01000000  ; Flip horizontal (bit 6)
STA sprite_attr

; Testing without modifying A
BIT $2002       ; Read PPUSTATUS
BPL not_vblank  ; Branch if bit 7 clear
```

**NES Common Usage:**
```asm
; Button detection
LDA buttons
AND #%00001000  ; Test Up button (bit 3)
BEQ no_up       ; Branch if not pressed

; PPU status check
BIT $2002       ; Check VBlank flag
BPL :-          ; Loop until VBlank (bit 7 set)
```

### Shifts and Rotates

| Instruction | Flags | Cycles | Description |
|-------------|-------|--------|-------------|
| **ASL** | NZC | 2-7 | Arithmetic Shift Left |
| **LSR** | NZC | 2-7 | Logical Shift Right |
| **ROL** | NZC | 2-7 | Rotate Left through Carry |
| **ROR** | NZC | 2-7 | Rotate Right through Carry |

**Examples:**
```asm
; Multiply by 2
LDA #5
ASL             ; A = 10 (shifts left)

; Divide by 2
LDA #10
LSR             ; A = 5 (shifts right)

; Rotate bits
LDA #%10000001
ROL             ; A = %00000011 (bit 7 → carry → bit 0)
```

**NES Common Usage:**
```asm
; Controller reading (shift button into carry)
LDA $4016
LSR             ; Bit 0 → carry flag
ROL buttons     ; Carry → bit 0 of buttons

; Fast multiplication by powers of 2
LDA value
ASL             ; × 2
ASL             ; × 4
ASL             ; × 8
```

### Comparison

| Instruction | Flags | Cycles | Description |
|-------------|-------|--------|-------------|
| **CMP** | NZC | 2-6 | Compare with Accumulator |
| **CPX** | NZC | 2-4 | Compare with X |
| **CPY** | NZC | 2-4 | Compare with Y |

**Comparison Results:**
- **Equal:** Z flag set
- **A >= operand:** C flag set
- **A < operand:** C flag clear

**Examples:**
```asm
; Check if equal
LDA #5
CMP #5
BEQ equal       ; Branch if A = 5

; Check if greater or equal
LDA paddle_y
CMP #200
BCS at_bottom   ; Branch if Y >= 200

; Check if less than
LDA paddle_y
CMP #8
BCC at_top      ; Branch if Y < 8
```

**NES Common Usage:**
```asm
; Boundary checking
LDA sprite_y
CMP #8
BCC clamp_top   ; If Y < 8, clamp to top
CMP #216
BCS clamp_bottom ; If Y >= 216, clamp to bottom
```

### Branching

| Instruction | Condition | Cycles | Description |
|-------------|-----------|--------|-------------|
| **BEQ** | Z = 1 | 2-4 | Branch if Equal (Zero) |
| **BNE** | Z = 0 | 2-4 | Branch if Not Equal |
| **BCS** | C = 1 | 2-4 | Branch if Carry Set (>=) |
| **BCC** | C = 0 | 2-4 | Branch if Carry Clear (<) |
| **BMI** | N = 1 | 2-4 | Branch if Minus (Negative) |
| **BPL** | N = 0 | 2-4 | Branch if Plus (Positive) |
| **BVS** | V = 1 | 2-4 | Branch if Overflow Set |
| **BVC** | V = 0 | 2-4 | Branch if Overflow Clear |

**Branch Range:** -128 to +127 bytes from the instruction after the branch

**Examples:**
```asm
; Loop example
    LDX #10
loop:
    ; Do something
    DEX
    BNE loop        ; Loop until X = 0

; Waiting for VBlank
:   BIT $2002       ; Check PPUSTATUS
    BPL :-          ; Loop while bit 7 = 0

; Forward label (ca65 syntax)
    LDA buttons
    AND #%10000000
    BEQ :+          ; Skip if A button not pressed
    ; A button pressed code
:   ; Continue here
```

**NES Common Usage:**
```asm
; Wait for NMI flag
MainLoop:
:   LDA nmi_ready
    BEQ :-          ; Loop until flag set
    ; Process frame
    JMP MainLoop

; PPU warmup (wait 2 VBlanks)
:   BIT $2002
    BPL :-          ; First VBlank
:   BIT $2002
    BPL :-          ; Second VBlank
```

### Jumps and Subroutines

| Instruction | Cycles | Description |
|-------------|--------|-------------|
| **JMP** | 3-5 | Jump to Address |
| **JSR** | 6 | Jump to Subroutine |
| **RTS** | 6 | Return from Subroutine |
| **RTI** | 6 | Return from Interrupt |
| **BRK** | 7 | Software Interrupt |

**Examples:**
```asm
; Infinite loop
Forever:
    JMP Forever

; Subroutine call
    JSR UpdateSprites
    ; Continues here after RTS

UpdateSprites:
    ; Sprite code
    RTS             ; Return to caller

; Interrupt handler
NMI:
    PHA             ; Save A
    ; Handle interrupt
    PLA             ; Restore A
    RTI             ; Return from interrupt
```

**NES Common Usage:**
```asm
; Main game loop
Reset:
    ; Init code
MainLoop:
    JSR ReadController
    JSR UpdateGame
    JSR WaitForNMI
    JMP MainLoop

; NMI handler
NMI:
    PHA
    TXA
    PHA
    TYA
    PHA

    ; Update graphics (OAM DMA, etc.)

    PLA
    TAY
    PLA
    TAX
    PLA
    RTI
```

### Stack Operations

| Instruction | Cycles | Description |
|-------------|--------|-------------|
| **PHA** | 3 | Push Accumulator |
| **PLA** | 4 | Pull Accumulator |
| **PHP** | 3 | Push Processor Status |
| **PLP** | 4 | Pull Processor Status |

**Stack grows downward:** $01FF → $0100

**Examples:**
```asm
; Save/restore A
PHA             ; Push A onto stack
; Do something that changes A
PLA             ; Restore A

; Preserve registers in interrupt
NMI:
    PHA         ; Save A
    TXA
    PHA         ; Save X
    TYA
    PHA         ; Save Y
    
    ; Interrupt code
    
    PLA
    TAY         ; Restore Y
    PLA
    TAX         ; Restore X
    PLA         ; Restore A
    RTI
```

### Flag Operations

| Instruction | Cycles | Description |
|-------------|--------|-------------|
| **CLC** | 2 | Clear Carry Flag |
| **SEC** | 2 | Set Carry Flag |
| **CLI** | 2 | Clear Interrupt Disable |
| **SEI** | 2 | Set Interrupt Disable |
| **CLV** | 2 | Clear Overflow Flag |
| **CLD** | 2 | Clear Decimal (no effect on NES) |
| **SED** | 2 | Set Decimal (no effect on NES) |

**Examples:**
```asm
CLC             ; Clear carry before ADC
SEC             ; Set carry before SBC
SEI             ; Disable interrupts
CLI             ; Enable interrupts
```

**NES Reset Sequence:**
```asm
Reset:
    SEI         ; Disable interrupts
    CLD         ; Clear decimal (no effect but good practice)
    ; Continue init
```

### No Operation

| Instruction | Cycles | Description |
|-------------|--------|-------------|
| **NOP** | 2 | No Operation (does nothing) |

**Uses:**
- Timing delays
- Code padding
- Placeholder for future code

---

## Common NES Patterns

### Waiting for VBlank
```asm
:   BIT $2002       ; Check PPUSTATUS
    BPL :-          ; Loop while bit 7 = 0 (not in VBlank)
```

### Controller Reading (Full Pattern)
```asm
ReadController:
    LDA #$01
    STA $4016       ; Strobe on
    LDA #$00
    STA $4016       ; Strobe off (latch buttons)

    LDX #$08        ; 8 buttons to read
:   LDA $4016       ; Read one button
    LSR             ; Shift bit 0 into carry
    ROL buttons     ; Rotate carry into buttons variable
    DEX
    BNE :-
    RTS
```

### OAM DMA (Sprite Update)
```asm
    LDA #$02        ; High byte of $0200 (OAM buffer)
    STA $4014       ; Trigger DMA (513 cycles)
```

### Setting PPU Address and Writing Data
```asm
    LDA $2002       ; Reset PPU address latch
    LDA #$3F        ; High byte of $3F00
    STA $2006       ; PPUADDR
    LDA #$00        ; Low byte
    STA $2006       ; PPUADDR
    LDA #$29        ; Color value
    STA $2007       ; PPUDATA (auto-increments)
```

### Negating a Value (Two's Complement)
```asm
    EOR #$FF        ; Flip all bits
    CLC
    ADC #$01        ; Add 1
```

### 16-Bit Addition
```asm
    CLC
    LDA value_lo
    ADC addend_lo
    STA result_lo
    LDA value_hi
    ADC addend_hi   ; Add with carry from low byte
    STA result_hi
```

### Efficient Loops
```asm
    ; Count down (more efficient than count up)
    LDX #10
:   ; Loop body
    DEX
    BNE :-          ; Loop while X != 0
    
    ; Vs. count up (requires CMP)
    LDX #0
:   ; Loop body
    INX
    CPX #10
    BNE :-
```

---

## Cycle Timing

**NTSC NES:** 1.79 MHz CPU
- **1 cycle = 558.73 nanoseconds**
- **~29780 cycles per scanline** (varies slightly)
- **~29780 × 262 = 7,802,360 cycles per frame**
- **60 Hz = 16.67ms per frame**

**VBlank Period:** ~2273 CPU cycles (20 scanlines)

**Important for:**
- Fitting code within VBlank window
- Precise timing for effects
- Understanding performance limits

---

## NES-Specific Considerations

### No Decimal Mode
The D flag exists but does nothing. All arithmetic is binary.

```asm
SED             ; Does nothing on NES
; Still use CLC/SEC for ADC/SBC
```

### Stack Location
Stack is fixed at $0100-$01FF. SP starts at $FF (points to $01FF).

**Reserve stack space:**
- Don't use $0100-$01FF for variables
- Deep subroutine calls consume stack
- NMI saves 3 registers (6 bytes)

### Zero Page is Critical
Fastest memory access. Use for hot variables.

**NES Memory Map (Quick):**
- $0000-$00FF: Zero page (fast access)
- $0100-$01FF: Stack
- $0200-$02FF: OAM buffer (sprite data)
- $0300-$07FF: General RAM
- $8000-$FFFF: PRG-ROM (program code)

---

## Quick Lookup Tables

### Flags Set by Instructions

| Instruction | N | Z | C | V |
|-------------|---|---|---|---|
| ADC, SBC | ✓ | ✓ | ✓ | ✓ |
| AND, ORA, EOR | ✓ | ✓ | - | - |
| ASL, LSR, ROL, ROR | ✓ | ✓ | ✓ | - |
| BIT | ✓ | ✓ | - | ✓ |
| CMP, CPX, CPY | ✓ | ✓ | ✓ | - |
| DEC, INC | ✓ | ✓ | - | - |
| LDA, LDX, LDY | ✓ | ✓ | - | - |
| TAX, TAY, TXA, TYA | ✓ | ✓ | - | - |

### Branch Instructions Quick Reference

| Mnemonic | Flag Check | Use Case |
|----------|------------|----------|
| BEQ | Z = 1 | After CMP: equal, After math: zero |
| BNE | Z = 0 | After CMP: not equal, Loop while not zero |
| BCS | C = 1 | After CMP: A >= value |
| BCC | C = 0 | After CMP: A < value |
| BMI | N = 1 | Negative result, bit 7 set |
| BPL | N = 0 | Positive result, bit 7 clear (VBlank loop) |

---

**Version:** 1.0
**Created:** 2025-10-24
**For:** NES Phase 1 Assembly Programming

**Next Steps:**
- PPU Programming Quick Reference (registers, nametables, OAM)
- NES Memory Map Quick Reference
- Controller & Input Quick Reference

**See also:**
- NES Phase 1 Tier 1 Lessons (001-016)
- NESDev Wiki for comprehensive documentation
