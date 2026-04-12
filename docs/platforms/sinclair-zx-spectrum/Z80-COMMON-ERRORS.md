# ZX Spectrum Z80 Assembly Common Errors

**Purpose:** Document common mistakes in Z80 assembly programming on ZX Spectrum
**Audience:** ZX Spectrum assembly programmers and curriculum designers
**Last Updated:** 2025-10-30

---

## Critical Hardware Constraints

### 1. Contended Memory (ULA Contention)

**Problem:** RAM at $4000-$7FFF is shared with ULA. CPU must wait for ULA during screen rendering.

**Impact:**
- Instructions take variable time depending on screen position
- Code in contended RAM runs slower
- Timing becomes unpredictable

**Contended Area:**
```
$4000-$57FF: Screen bitmap (6144 bytes) - HEAVILY contended
$5800-$5AFF: Screen attributes (768 bytes) - HEAVILY contended
$5B00-$7FFF: Remaining lower RAM - Still contended
```

**Solution:**
```asm
; Place time-critical code in upper RAM ($8000+)
    ORG $8000       ; Non-contended RAM
FastRoutine:
    ; Tight loops here won't be slowed by ULA
    LD B,100
Loop:
    DJNZ Loop
    RET

; Or execute during vertical blank when ULA not accessing RAM
```

**Rule:** Time-critical code belongs in $8000+ or runs during VBlank.

---

### 2. Screen Memory Thirds Formula (Still Applies)

**Problem:** Z80 code still needs the thirds formula for direct screen pixel access.

**Wrong (Linear):**
```asm
; WRONG - Linear calculation
    LD HL,$4000
    LD DE,32
    LD B,192        ; 192 rows
RowLoop:
    ; ... plot row ...
    ADD HL,DE       ; WRONG - rows not consecutive!
    DJNZ RowLoop
```

**Correct (Thirds):**
```asm
; CORRECT - Calculate screen address for pixel (B=Y, C=X)
CalcScreen:
    LD A,B          ; A = Y coordinate
    AND $07         ; A = Y AND 7 (line in character)
    OR $40          ; Set high byte ($40 = $4000 base)
    LD H,A

    LD A,B
    RRCA
    RRCA
    RRCA
    AND $E0         ; Isolate bits for third
    LD L,A

    LD A,B
    AND $18
    OR L
    LD L,A          ; HL = screen address for row

    LD A,C
    SRL A
    SRL A
    SRL A
    ADD A,L
    LD L,A          ; HL = final screen address
    RET
```

**Rule:** Use thirds formula for bitmap, linear for attributes.

---

### 3. Interrupts and IM Modes

**Problem:** Spectrum supports 3 interrupt modes. Default (IM 1) may not be what you want.

**Interrupt Modes:**
- **IM 0:** Execute instruction on bus (rarely used)
- **IM 1:** Jump to $0038 (ROM routine, default)
- **IM 2:** Use I register + vector table (custom handler)

**Wrong (Assuming IM 2 without setting it):**
```asm
; WRONG - Setting up IM 2 handler without IM 2
    LD HL,MyISR
    LD ($FFF4),HL   ; Set vector
    EI              ; Enable interrupts
    ; WRONG! Still in IM 1, jumps to ROM $0038
```

**Correct (IM 2 Setup):**
```asm
; Set up IM 2 interrupt
    DI              ; Disable interrupts during setup
    LD A,$3F        ; High byte of vector table
    LD I,A
    IM 2            ; Set interrupt mode 2
    EI              ; Enable interrupts

; Vector table at $3F00 (must be aligned to 256-byte page)
    ORG $3F00
IntVector:
    DEFB $FD,$FD,$FD,...  ; 257 bytes pointing to $FDFD (handler)

; Interrupt handler at $FDFD
    ORG $FDFD
MyISR:
    PUSH AF
    ; ... interrupt code ...
    POP AF
    EI
    RETI
```

**Rule:** Always use `IM` instruction. IM 1 = ROM, IM 2 = custom.

---

### 4. Stack Pointer Must Be Initialized

**Problem:** SP starts at $5CB6 (ROM default). This is in system variables area!

**Symptoms:**
- Crashes on CALL/RET
- Corrupted system variables
- Unpredictable behavior

**Wrong (Using ROM default):**
```asm
; WRONG - No SP initialization
Start:
    CALL DoSomething  ; Uses ROM default SP!
    RET
```

**Correct (Initialize SP):**
```asm
Start:
    DI
    LD SP,$FF00     ; Set SP to safe area (top of RAM)
    ; Now safe to use CALL/RET
    CALL DoSomething
    RET
```

**Safe SP Locations:**
- **48K:** $FF00-$FFFF (top 256 bytes)
- **128K:** Bank-dependent, but $FF00 usually safe

**Rule:** ALWAYS initialize SP before using CALL/PUSH/POP.

---

### 5. DJNZ Range Limitation

**Problem:** DJNZ can only jump -126 to +129 bytes. Exceeding this fails silently.

**Symptoms:**
- Loop doesn't loop
- Unexpected jumps
- No assembler error (just wrong code)

**Wrong (Jump too far):**
```asm
    LD B,10
LongLoop:
    ; ... 200 bytes of code ...
    DJNZ LongLoop   ; ERROR - Too far!
```

**Correct (Use JP for long jumps):**
```asm
    LD B,10
LongLoop:
    ; ... 200 bytes of code ...
    DEC B
    JP NZ,LongLoop  ; Use JP for long distances
```

**Rule:** DJNZ for short loops only (<100 bytes). Use DEC+JP NZ for longer.

---

### 6. Register Preservation in Subroutines

**Problem:** Not preserving registers corrupts caller's data.

**Wrong:**
```asm
Multiply:
    ; Uses BC but doesn't preserve it!
    LD B,A
    LD C,0
MulLoop:
    ADD C,A
    DJNZ MulLoop
    LD A,C
    RET             ; BC destroyed!

Main:
    LD BC,$1234     ; Important value
    LD A,5
    CALL Multiply   ; BC now corrupted!
```

**Correct:**
```asm
Multiply:
    PUSH BC         ; Preserve BC
    LD B,A
    LD C,0
MulLoop:
    ADD C,A
    DJNZ MulLoop
    LD A,C
    POP BC          ; Restore BC
    RET

Main:
    LD BC,$1234
    LD A,5
    CALL Multiply   ; BC safe
```

**Rule:** PUSH any registers you modify, POP before RET.

---

## Z80-Specific Issues

### 7. Flags Not Set By All Instructions

**Problem:** Not all instructions affect all flags. Some preserve them.

**Instructions That DON'T Set Flags:**
- `LD` (any form) - No flags affected
- `EX` - No flags affected
- `PUSH/POP` - No flags affected

**Example Mistake:**
```asm
; WRONG - LD doesn't set Zero flag
    LD A,0
    JP Z,IsZero     ; WRONG! Z flag unchanged by LD
```

**Correct:**
```asm
; Use CP or OR to set flags
    LD A,0
    OR A            ; Set Z flag based on A
    JP Z,IsZero     ; Now correct
```

**Rule:** Use `OR A` or `CP 0` to set flags after LD.

---

### 8. 16-Bit Arithmetic Is Limited

**Problem:** Z80 has limited 16-bit arithmetic. No ADD HL,HL for example.

**Available 16-bit:**
- `ADD HL,BC/DE/HL/SP` - Only HL as destination
- `ADC HL,BC/DE/HL/SP` - With carry
- `SBC HL,BC/DE/HL/SP` - With borrow
- `INC/DEC BC/DE/HL/SP` - No flags except INC/DEC IX/IY

**Wrong (Trying to add to DE):**
```asm
; WRONG - Can't ADD DE,HL
    ADD DE,HL       ; Doesn't exist!
```

**Correct:**
```asm
; Add HL to DE by swapping
    EX DE,HL        ; Swap DE <-> HL
    ADD HL,DE       ; Now add (HL = HL + old HL = old HL + old DE)
    EX DE,HL        ; Swap back if needed
```

**Rule:** 16-bit ADD/ADC/SBC only work with HL as destination.

---

### 9. IX/IY Are Slow

**Problem:** IX and IY index registers are slower than HL.

**Timing Comparison:**
```asm
LD A,(HL)    ; 7 T-states
LD A,(IX+0)  ; 19 T-states (nearly 3x slower!)
```

**Wrong (Using IX in tight loops):**
```asm
; SLOW - IX in pixel loop
    LD B,192
PixelLoop:
    LD A,(IX+0)     ; 19 T-states per pixel!
    INC IX
    DJNZ PixelLoop
```

**Correct (Use HL for speed):**
```asm
; FAST - HL in pixel loop
    LD B,192
PixelLoop:
    LD A,(HL)       ; 7 T-states
    INC HL
    DJNZ PixelLoop
```

**Rule:** Use HL for speed-critical code. Save IX/IY for occasional use.

---

### 10. Undocumented Instructions

**Problem:** Z80 has undocumented but functional instructions. May not work on clones.

**Undocumented but Common:**
- `SLL` (shift left logical with bit 0 set) - Works on real Z80
- `IN F,(C)` - Read port, discard result, set flags
- Half of IX/IY (e.g., `LD A,IXH`) - Access high/low bytes

**Risk:**
```asm
; Undocumented - may fail on clones
    SLL A           ; Works on Spectrum, fails on some clones
```

**Safer:**
```asm
; Documented equivalent
    SLA A
    SET 0,A         ; Explicit bit set
```

**Rule:** Avoid undocumented instructions unless necessary, document usage.

---

## Performance Pitfalls

### 11. Unrolling Loops vs Memory

**Problem:** Loop unrolling speeds up code but uses more memory.

**Tradeoff:**
```asm
; Compact (slow)
    LD B,8
Loop:
    LD A,(HL)
    INC HL
    DJNZ Loop       ; 13 T-states per iteration

; Unrolled (fast but large)
    LD A,(HL)       ; 7 T-states
    INC HL          ; 6 T-states
    LD A,(HL)       ; 7 T-states
    INC HL
    ; ... repeat 6 more times
    ; Total: 8 × 13 = 104 T-states saved
    ; Cost: 8 × 2 = 16 bytes vs 4 bytes
```

**Rule:** Unroll inner loops for speed if memory allows.

---

### 12. Self-Modifying Code

**Problem:** ROM on Spectrum is read-only, but RAM code can modify itself.

**Use Case:**
```asm
; Modify constant in code for speed
PlotPixel:
    LD A,$FF        ; Modified at runtime
SMC_Value EQU $-1   ; Label points to value byte
    LD (HL),A
    RET

; Caller modifies the constant
SetPixelValue:
    LD A,B
    LD (SMC_Value),A  ; Change LD A,$ value
    RET
```

**Caution:**
- Only works in RAM
- Cache issues on some systems (not Spectrum)
- Hard to debug

**Rule:** Self-modifying code is valid on Spectrum but document it clearly.

---

## Assembler-Specific Issues

### 13. Expression Evaluation Order

**Problem:** Different assemblers evaluate expressions differently.

**Example:**
```asm
; May be ambiguous
    LD A,10+20*2    ; Is it (10+20)*2 or 10+(20*2)?
```

**Solution:**
```asm
; Use parentheses for clarity
    LD A,10+(20*2)  ; Clear: 10+40 = 50
```

**Rule:** Always use parentheses in complex expressions.

---

### 14. ORG and Phase Errors

**Problem:** Forgetting ORG causes code to assemble at wrong address.

**Wrong:**
```asm
; No ORG - assembler uses default ($0000)
Start:
    LD A,1
    RET             ; Will be at $0000 (ROM!)
```

**Correct:**
```asm
    ORG $8000       ; Explicit start address
Start:
    LD A,1
    RET             ; Now at $8000
```

**Rule:** Always use ORG to specify code location.

---

### 15. Relative vs Absolute Jumps

**Problem:** JR is relative (2 bytes), JP is absolute (3 bytes).

**Optimization:**
```asm
; Use JR for nearby jumps (saves 1 byte)
    JR Z,Nearby     ; 2 bytes, -126 to +129 range
    JP Z,FarAway    ; 3 bytes, full 64K range
```

**Rule:** Use JR for short jumps (<100 bytes), JP for long or if unsure.

---

## Summary of Critical Rules

1. ✅ **Code at $8000+ avoids contention** - or run during VBlank
2. ✅ **Screen bitmap uses thirds formula** - always
3. ✅ **Set interrupt mode with IM** - don't assume
4. ✅ **Initialize SP before CALL/PUSH** - mandatory
5. ✅ **DJNZ range = ±127 bytes** - use JP for longer
6. ✅ **PUSH/POP registers in subroutines** - preserve caller state
7. ✅ **LD doesn't set flags** - use OR A to test
8. ✅ **16-bit ADD only works with HL** - as destination
9. ✅ **IX/IY are 3x slower than HL** - avoid in loops
10. ✅ **Undocumented instructions risky** - may fail on clones
11. ✅ **Always use ORG** - specify code location
12. ✅ **JR for short, JP for long** - JR saves 1 byte

---

**This document should be consulted before writing any Z80 assembly lesson code.**
