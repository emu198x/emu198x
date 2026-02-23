# C64 6510 Assembly Common Errors and Pitfalls

**Purpose:** Document common mistakes in 6510 assembly programming on C64
**Audience:** C64 assembly programmers and curriculum designers
**Last Updated:** 2025-11-16

---

## Critical Hardware Constraints

### 1. Zero Page Conflicts with KERNAL/BASIC

**Problem:** Zero page ($00-$FF) is shared with KERNAL and BASIC. Using reserved locations causes crashes.

**Impact:**
- System crashes
- BASIC commands malfunction
- Interrupts break
- File I/O fails

**Reserved Zero Page Locations:**
```
$00-$01: Processor I/O port and data direction (CRITICAL - memory banking)
$90-$FF: KERNAL and BASIC workspace (approx 100 bytes used)
$A0-$A2: Floating point accumulator
$C0-$C5: BASIC input buffer pointer
$D0-$F2: KERNAL variables
```

**Safe Zero Page for User Code:**
```
$02-$8F: Generally safe if not using KERNAL/BASIC (142 bytes)
$FB-$FE: Free for user (4 bytes, commonly used)
```

**Wrong:**
```asm
; WRONG - Overwriting KERNAL pointers
    LDA #$00
    STA $90     ; KERNAL status - causes I/O failure!
    STA $C0     ; BASIC pointer - crashes BASIC!
```

**Correct:**
```asm
; Use safe zero page locations
ZP_TEMP = $FB   ; Safe user zero page
ZP_PTR  = $FC   ; Safe pointer (2 bytes: $FC-$FD)

    LDA #$00
    STA ZP_TEMP
    LDA #<ScreenData
    STA ZP_PTR
    LDA #>ScreenData
    STA ZP_PTR+1
```

**Rule:** Use $FB-$FE for quick work, or $02-$8F if not calling KERNAL. Document all zero page usage clearly.

---

### 2. Stack Overflow/Underflow

**Problem:** 6510 stack is only 256 bytes ($0100-$01FF). Excessive JSR nesting or missing RTS causes crashes.

**Symptoms:**
- System crashes after deep recursion
- RTS jumps to wrong address
- Stack pointer wraps ($00 → $FF)

**Wrong:**
```asm
; WRONG - RTS without JSR (underflow)
Start:
    LDA #$00
    RTS         ; Stack empty - returns to garbage address!

; WRONG - Infinite recursion (overflow)
Recurse:
    JSR Recurse ; Stack fills after ~128 calls
    RTS
```

**Correct:**
```asm
; Balanced JSR/RTS
Main:
    JSR InitSprites  ; Push return address
    JSR MainLoop
    RTS              ; Return from Main

InitSprites:
    LDA #$00
    ; ... init code ...
    RTS              ; Return to Main

; Check stack depth for debugging
CheckStack:
    TSX         ; X = stack pointer
    CPX #$F0    ; Warn if stack > 16 bytes used
    BCS StackOK
    ; Handle stack overflow warning
StackOK:
    RTS
```

**Stack Usage Per JSR:**
- Each JSR: 2 bytes (return address)
- Each PHA: 1 byte
- IRQ: 3 bytes (PC + Status)

**Rule:** Keep JSR nesting shallow (<10 levels). Balance every JSR with RTS. Monitor stack pointer in complex code.

---

### 3. Decimal Mode (Not Disabled by Default!)

**Problem:** Unlike NES, C64 boots with decimal mode potentially enabled. Affects ADC/SBC unless explicitly cleared.

**Impact:**
- ADC/SBC produce BCD results instead of binary
- Calculations produce wrong values
- Loop counters behave incorrectly

**Wrong Assumption:**
```asm
; WRONG - Assuming binary mode
Start:
    LDA #$09
    ADC #$01    ; May produce $10 (BCD) instead of $0A (binary)!
    STA Counter
```

**Correct:**
```asm
; ALWAYS clear decimal mode at startup
Start:
    CLD         ; Clear Decimal mode - MANDATORY!
    LDA #$09
    ADC #$01    ; Now produces $0A (binary)
    STA Counter
```

**Rule:** FIRST instruction in any standalone program should be `CLD`. NEVER assume binary mode.

---

## Critical Interrupt Handling

### 4. Forgetting SEI/CLI Around Critical Sections

**Problem:** IRQs can interrupt mid-operation, corrupting multi-byte values or I/O operations.

**Symptoms:**
- Flickering sprites (position updated mid-frame)
- Corrupted 16-bit values
- Race conditions

**Wrong:**
```asm
; WRONG - No interrupt protection
UpdateSprite:
    LDA SpriteX      ; Read low byte
    ; *** IRQ COULD FIRE HERE - corrupts X position! ***
    STA $D000        ; Write low byte
    LDA SpriteX+1
    STA $D010        ; MSB wrong if IRQ modified SpriteX
```

**Correct:**
```asm
UpdateSprite:
    SEI              ; Disable interrupts
    LDA SpriteX
    STA $D000
    LDA SpriteX+1
    AND #$01
    STA $D010
    CLI              ; Re-enable interrupts
    RTS
```

**Rule:** Use SEI/CLI around multi-byte updates or time-critical operations. Keep SEI sections SHORT (<1ms).

---

### 5. Using BRK Instead of RTI in IRQ Handlers

**Problem:** BRK is for software interrupts. IRQ handlers must end with RTI, not RTS.

**Symptoms:**
- System crashes after interrupt
- Stack corrupted
- IRQs stop working

**Wrong:**
```asm
IRQHandler:
    INC $D020    ; Change border colour
    RTS          ; WRONG - RTS instead of RTI!
```

**Correct:**
```asm
IRQHandler:
    PHA          ; Save A
    TXA
    PHA          ; Save X
    TYA
    PHA          ; Save Y

    INC $D020    ; Do IRQ work

    PLA
    TAY          ; Restore Y
    PLA
    TAX          ; Restore X
    PLA          ; Restore A
    RTI          ; Return from interrupt (not RTS!)
```

**Rule:** IRQ handlers MUST end with RTI. ALWAYS preserve A/X/Y registers.

---

## Memory Banking Errors

### 6. Forgetting to Restore Memory Configuration

**Problem:** $01 controls memory banking (KERNAL/BASIC/I/O visibility). Changing without restoring breaks system.

**Memory Map Controlled by $01:**
```
$01 = $37 (default): BASIC ROM, KERNAL ROM, I/O visible
$01 = $36: I/O and KERNAL visible, BASIC ROM hidden (RAM)
$01 = $35: I/O visible, KERNAL/BASIC hidden (max RAM)
$01 = $34: All RAM, no ROM/I/O (DANGEROUS - can't access VIC/SID!)
```

**Wrong:**
```asm
; WRONG - Switching to all RAM, can't switch back!
    LDA #$34
    STA $01      ; All RAM mode
    ; No way to access $D000-$DFFF (VIC/SID) or KERNAL!
```

**Correct:**
```asm
; Save and restore memory configuration
    LDA $01      ; Save current config
    PHA

    LDA #$35     ; Switch to I/O + RAM
    STA $01

    ; ... do work ...

    PLA
    STA $01      ; Restore original config
```

**Rule:** ALWAYS save $01 before changing, restore after. Be careful with $34 (all RAM) - can't access I/O!

---

## Common Instruction Errors

### 7. Absolute vs. Zero Page Addressing

**Problem:** Using absolute addressing for zero page wastes bytes and cycles.

**Impact:**
- Code larger than necessary
- Slower execution (extra cycle)
- Less efficient

**Wrong:**
```asm
; WRONG - Using absolute mode for zero page
    LDA $00FB    ; 3 bytes, 4 cycles
    STA $00FC    ; 3 bytes, 4 cycles
```

**Correct:**
```asm
; CORRECT - Zero page addressing
    LDA $FB      ; 2 bytes, 3 cycles
    STA $FC      ; 2 bytes, 3 cycles
```

**Rule:** Use zero page addressing ($00-$FF) when possible - faster and smaller. Most assemblers auto-select if you use explicit zero page symbols.

---

### 8. Branch Range Limitations (±127 bytes)

**Problem:** Branch instructions (BEQ, BNE, BCC, etc.) have limited range of -128 to +127 bytes.

**Symptoms:**
- `Branch out of range` assembler error
- Cannot branch to distant labels

**Wrong:**
```asm
Start:
    LDA Status
    BEQ FarAway  ; ERROR if FarAway > 127 bytes away

    ; ... 200 bytes of code ...

FarAway:
    RTS
```

**Correct (Use JMP for long branches):**
```asm
Start:
    LDA Status
    BNE NotZero  ; Branch to nearby label
    JMP FarAway  ; Use JMP for distant target
NotZero:
    ; ... continue ...

    ; ... 200 bytes of code ...

FarAway:
    RTS
```

**Correct (Invert condition):**
```asm
Start:
    LDA Status
    BNE Skip     ; Invert: branch if NOT equal
    JMP FarAway  ; Jump if equal
Skip:
    ; ... continue ...

FarAway:
    RTS
```

**Rule:** Use JMP for long-distance branches. Invert condition + JMP if necessary.

---

## Self-Modifying Code Pitfalls

### 9. Forgetting That Code is Data

**Problem:** Self-modifying code must account for instruction structure and addressing modes.

**Wrong:**
```asm
; WRONG - Modifying wrong byte
    LDA #$00
ModifyHere:
    STA $D020      ; Want to change $D020 to different address

    ; Later, trying to modify:
    LDA #$21
    STA ModifyHere ; WRONG - Overwrites STA opcode, not address!
```

**Correct:**
```asm
ModifyHere:
    LDA #$00
    STA $D020      ; STA absolute = 3 bytes: $8D $20 $D0

    ; Modify the address (bytes 2-3 of STA instruction)
    LDA #$21       ; New low byte
    STA ModifyHere+1  ; Modify byte after STA opcode
    LDA #$D0       ; New high byte
    STA ModifyHere+2  ; Modify high byte

    ; Now STA targets $D021 instead of $D020
```

**Instruction Structure:**
```
STA $D020 assembles to:
  $8D      (STA absolute opcode)
  $20      (low byte of address)
  $D0      (high byte of address)

To modify target address:
  ModifyHere+0 = opcode (don't change)
  ModifyHere+1 = low byte of address
  ModifyHere+2 = high byte of address
```

**Rule:** Self-modifying code must know instruction encoding. Document byte offsets clearly.

---

## KERNAL Usage Pitfalls

### 10. Not Preserving Registers When Calling KERNAL

**Problem:** KERNAL routines destroy register values. Failing to save/restore causes bugs.

**KERNAL Routines Destroy:**
- Most KERNAL calls: A, X, Y (all registers)
- Some preserve X or Y (check documentation)

**Wrong:**
```asm
; WRONG - Not preserving registers
    LDX #$05       ; X = sprite number
    LDA #$FF
    JSR $FFD2      ; CHROUT - destroys A, X, Y!
    ; X no longer $05 - sprite code breaks!
    STX $D000      ; Wrong sprite position!
```

**Correct:**
```asm
    LDX #$05       ; X = sprite number
    PHA            ; Save A
    TXA
    PHA            ; Save X

    LDA #$FF
    JSR $FFD2      ; CHROUT

    PLA
    TAX            ; Restore X
    PLA            ; Restore A

    STX $D000      ; X still $05 - correct!
```

**Rule:** ALWAYS preserve A/X/Y before KERNAL calls unless you know the specific routine preserves them.

---

## Timing and Synchronisation

### 11. Busy-Wait Loops Without Volatile Checks

**Problem:** Waiting for hardware register changes requires checking hardware, not cached values.

**Wrong:**
```asm
; WRONG - May be optimised away or cached
WaitRaster:
    LDA $D012      ; Read raster line
    CMP #$FF       ; Wait for line $FF
    BNE WaitRaster
```

**Correct (Though above code usually works):**
```asm
; Better practice - explicit volatile read
WaitRaster:
    LDA $D012      ; Read raster line (volatile hardware register)
    CMP #$FF
    BNE WaitRaster

; Or wait for change:
WaitRasterChange:
    LDA $D012
WaitLoop:
    CMP $D012      ; Wait until raster line changes
    BEQ WaitLoop
```

**Rule:** Hardware registers are volatile - always read from hardware address. Don't cache in variables for timing-critical checks.

---

## References

- **6510 Instruction Reference:** [../hardware/6510-QUICK-REFERENCE.md](../hardware/6510-QUICK-REFERENCE.md)
- **VIC-II Hardware:** [../hardware/VIC-II-QUICK-REFERENCE.md](../hardware/VIC-II-QUICK-REFERENCE.md)
- **Memory Map:** [../memory-and-io/C64-MEMORY-MAP.md](../memory-and-io/C64-MEMORY-MAP.md)
- **KERNAL Routines:** [KERNAL-ROUTINES-REFERENCE.md](KERNAL-ROUTINES-REFERENCE.md)
- **Raster Interrupts:** [RASTER-INTERRUPTS-REFERENCE.md](RASTER-INTERRUPTS-REFERENCE.md)

---

**Document Version:** 1.0
**Last Updated:** 2025-11-16
**Based on:** 6510 CPU specification and C64 system architecture
