# Amiga 68000 Assembly Common Errors

**Purpose:** Document common mistakes in 68000 assembly programming on Amiga
**Audience:** Amiga assembly programmers and curriculum designers
**Last Updated:** 2025-10-30

---

## Critical Chip RAM Requirements

### 1. Chip RAM for DMA Operations

**Problem:** Custom chips (Paula, Agnus, Denise) can ONLY access Chip RAM via DMA.

**Must Be in Chip RAM:**
- Graphics bitplanes
- Sprite data
- Audio samples
- Copper lists
- Blitter source/destination

**Wrong:**
```asm
; Code allocates graphics in Fast RAM (if available)
    SECTION graphics,DATA
Sprites:
    DC.W $0000,$0000  ; Sprite data
    ; ... more sprite data ...

; Try to use sprite
    MOVE.L #Sprites,SPR0PTH  ; WRONG if in Fast RAM!
```

**Correct:**
```asm
; Explicitly place in Chip RAM
    SECTION graphics,DATA_C  ; _C = Chip RAM
Sprites:
    DC.W $0000,$0000
    ; ... sprite data ...

; Now safe to use
    MOVE.L #Sprites,SPR0PTH  ; OK - in Chip RAM
```

**Rule:** Graphics/audio data = Chip RAM always. Use SECTION with _C suffix.

---

### 2. Address Error on Odd Addresses

**Problem:** 68000 requires word/long access to be on even addresses.

**Symptoms:**
- Address Error exception
- Crash
- Reset

**Wrong:**
```asm
; Odd-aligned data
Data:
    DC.B $12         ; 1 byte - next address is ODD
Value:
    DC.W $1234       ; WRONG - word at odd address!
```

**Correct:**
```asm
Data:
    DC.B $12
    EVEN             ; Align to even address
Value:
    DC.W $1234       ; OK - word at even address

; OR use CNOP
Data2:
    DC.B $12
    CNOP 0,2         ; Align to 2-byte boundary
Value2:
    DC.W $5678       ; OK
```

**Rule:** Use EVEN or CNOP after DC.B before DC.W/DC.L.

---

## Privileged Instructions

### 3. Supervisor vs User Mode

**Problem:** Some instructions only work in supervisor mode. Games usually run in user mode.

**Privileged Instructions:**
- `MOVE to/from SR` - Status register
- `MOVE to/from USP` - User stack pointer
- `RTE` - Return from exception
- `RESET` - Hardware reset
- `STOP` - Stop CPU

**Wrong (User Mode):**
```asm
; Trying to modify status register in user mode
    MOVE.W #$2700,SR  ; PRIVILEGE VIOLATION!
```

**Correct (Take Over System):**
```asm
; Proper Amiga system takeover
    MOVE.L 4.W,A6          ; ExecBase
    JSR -132(A6)           ; Forbid()

    LEA $DFF000,A6         ; Custom chips base
    MOVE.W INTENAR(A6),SaveInts
    MOVE.W #$7FFF,INTENA(A6)  ; Disable interrupts

    ; Now can access hardware freely
```

**Rule:** Taking over system = disable OS, access hardware directly. Or use OS functions.

---

## Register Conventions

### 4. A7 Is the Stack Pointer

**Problem:** A7 is SP. Modifying it corrupts the stack.

**Wrong:**
```asm
; Using A7 as general purpose register
    MOVE.L #MyData,A7   ; WRONG - corrupts stack!
    MOVE.L (A7),D0
```

**Correct:**
```asm
; Use A0-A6 for addressing
    MOVE.L #MyData,A0
    MOVE.L (A0),D0

; Only modify A7 deliberately (e.g., switching stacks)
    MOVE.L #NewStack,A7  ; Intentional stack change
```

**Rule:** A7 = SP. Don't use as general register.

---

### 5. A6 Often Reserved for Library Base

**Problem:** AmigaOS uses A6 for library base pointers.

**Convention:**
```asm
; Calling OS functions
    MOVE.L 4.W,A6       ; ExecBase in A6
    JSR -132(A6)        ; Call function (offset from A6)
```

**When to Preserve:**
- If calling OS functions, A6 = library base
- In standalone code (no OS), A6 free to use

**Rule:** If using OS, treat A6 as scratch only after preserving it.

---

## Performance and Optimization

### 6. LEA vs MOVE.L for Addresses

**Problem:** MOVE.L #address,An is slower and larger than LEA.

**Comparison:**
```asm
; Slow and large
    MOVE.L #MyData,A0   ; 6 bytes, 12 cycles

; Fast and small
    LEA MyData,A0       ; 4 bytes, 8 cycles
```

**Rule:** Use LEA to load addresses into address registers.

---

### 7. MOVEM for Multiple Registers

**Problem:** Multiple MOVE instructions waste bytes and cycles.

**Inefficient:**
```asm
; Save registers individually
    MOVE.L D0,-(SP)     ; 2 bytes each
    MOVE.L D1,-(SP)
    MOVE.L D2,-(SP)
    MOVE.L A0,-(SP)
    MOVE.L A1,-(SP)
    ; Total: 10 bytes, 40 cycles
```

**Efficient:**
```asm
; Save registers together
    MOVEM.L D0-D2/A0-A1,-(SP)  ; 4 bytes, 32 cycles

; Restore
    MOVEM.L (SP)+,D0-D2/A0-A1  ; 4 bytes, 32 cycles
```

**Rule:** Use MOVEM for saving/restoring multiple registers.

---

### 8. DBcc Loops Are Efficient

**Problem:** Not using DBcc wastes cycles in loops.

**Inefficient:**
```asm
; Manual loop
    MOVE.W #99,D0
Loop:
    ; ... loop body ...
    SUBQ.W #1,D0
    BPL Loop           ; Branch if ≥ 0
```

**Efficient:**
```asm
; DBcc loop (count+1 iterations)
    MOVE.W #99,D0
Loop:
    ; ... loop body ...
    DBRA D0,Loop       ; Decrement and branch
```

**DBcc Behavior:**
- Decrements register
- If register ≠ -1, branches
- Executes (count+1) iterations total

**Rule:** Use DBcc (DBRA/DBF) for counted loops.

---

## Blitter Operations

### 9. Blitter Must Be Free Before Use

**Problem:** Starting blitter operation while busy corrupts data.

**Wrong:**
```asm
; Start blitter without checking
    MOVE.L #$09F00000,BLTCON0  ; Configure
    MOVE.W #64*64,BLTSIZE      ; Start - may be busy!
```

**Correct:**
```asm
; Wait for blitter to finish
WaitBlit:
    BTST #6,DMACONR     ; Test blitter busy bit
    BNE.S WaitBlit      ; Loop while busy

    ; Now safe to use blitter
    MOVE.L #$09F00000,BLTCON0
    MOVE.W #64*64,BLTSIZE
```

**Rule:** Always wait for blitter before new operation.

---

### 10. Blitter Locks Out CPU (Even Cycles)

**Problem:** Blitter uses even memory cycles. CPU can only access odd cycles.

**Impact:**
- CPU slows down during blitter operation
- Code execution unpredictable

**Mitigation:**
```asm
; Keep blitter operations short
; Or structure code to work during blitter busy time
    ; Start blit
    MOVE.W #64*64,BLTSIZE

    ; Do CPU work while blit runs
    ; (calculations, etc. - not memory intensive)

    ; Then wait for completion
WaitBlit:
    BTST #6,DMACONR
    BNE.S WaitBlit
```

**Rule:** Blitter and CPU share memory bandwidth. Plan accordingly.

---

## Copper Programming

### 11. Copper WAIT Limitations

**Problem:** Copper WAIT can only wait for specific beam positions.

**Copper WAIT:**
- Waits for horizontal/vertical position
- Limited to positions < $E0 horizontal
- Can wait for specific lines

**Wrong:**
```asm
; Trying to wait for position $FF,0
    DC.W $FF01,$FFFE   ; WRONG - horizontal position limited
```

**Correct:**
```asm
; Wait for scanline 100
    DC.W $6401,$FFFE   ; Wait for line $64 (100), position $01

; Multiple waits for precision
    DC.W $6401,$FFFE
    DC.W $6481,$FFFE   ; Wait for same line, later position
```

**Rule:** Copper WAIT horizontal positions limited. Use multiple waits if needed.

---

### 12. Copper List Must Be in Chip RAM

**Problem:** Copper reads list via DMA = must be Chip RAM.

**Wrong:**
```asm
    SECTION code,CODE    ; Code section (may be Fast RAM)
CopperList:
    DC.W $0180,$0F00     ; Color 0 = red
    DC.W $FFFF,$FFFE     ; End
```

**Correct:**
```asm
    SECTION copper,DATA_C  ; Chip RAM section
CopperList:
    DC.W $0180,$0F00
    DC.W $FFFF,$FFFE
```

**Rule:** Copper lists = Chip RAM always.

---

## Common Instruction Mistakes

### 13. BSR vs JSR Range

**Problem:** BSR has limited range (±32KB). JSR has full range.

**BSR Range:**
- 16-bit displacement
- ±32,768 bytes from current PC

**Wrong (If Target Far):**
```asm
    BSR FarRoutine   ; May be >32KB away - ERROR!
```

**Correct:**
```asm
; Use JSR for far calls
    JSR FarRoutine   ; Full 32-bit address

; Use BSR for nearby calls (saves 2 bytes)
    BSR NearRoutine  ; <32KB away
```

**Rule:** BSR for local calls, JSR for far or if unsure.

---

### 14. Bit Numbering Is 0-31 (Not 1-32)

**Problem:** Bits numbered 0-31, not 1-32.

**Wrong:**
```asm
; Testing bit "1" thinking it's first bit
    BTST #1,D0       ; Tests bit 1 (second bit!)
```

**Correct:**
```asm
; Test bit 0 (first bit)
    BTST #0,D0

; Bit numbering:
; Bit 0 = rightmost (LSB)
; Bit 31 = leftmost (MSB) for long
```

**Rule:** Bits numbered 0-31 (or 0-15 for words, 0-7 for bytes).

---

### 15. CLR vs MOVEQ #0

**Problem:** CLR reads before writing. MOVEQ is faster.

**Inefficient:**
```asm
    CLR.L D0         ; Read-modify-write (8 cycles)
```

**Efficient:**
```asm
    MOVEQ #0,D0      ; Direct write (4 cycles)
```

**Rule:** Use MOVEQ #0,Dn to clear data registers (faster).

---

## System-Specific Issues

### 16. System Takeover and Restore

**Problem:** Taking over hardware requires proper system shutdown and restore.

**Minimal Takeover:**
```asm
; Disable OS
    MOVE.L 4.W,A6           ; ExecBase
    JSR -132(A6)            ; Forbid()

    LEA $DFF000,A6
    MOVE.W INTENAR(A6),OldInt
    MOVE.W DMACONR(A6),OldDMA

    MOVE.W #$7FFF,INTENA(A6)   ; Disable interrupts
    MOVE.W #$7FFF,DMACON(A6)   ; Disable DMA

    ; ... use hardware ...

; Restore OS
    MOVE.W OldInt,D0
    OR.W #$C000,D0
    MOVE.W D0,INTENA(A6)       ; Re-enable interrupts

    MOVE.W OldDMA,D0
    OR.W #$8000,D0
    MOVE.W D0,DMACON(A6)       ; Re-enable DMA

    MOVE.L 4.W,A6
    JSR -138(A6)               ; Permit()
```

**Rule:** Save/restore all hardware state when taking over system.

---

### 17. Kickstart Version Differences

**Problem:** Different Kickstart versions have different OS functions.

**Versions:**
- **1.2/1.3** - Early Amigas (A500, A1000)
- **2.0+** - Later Amigas (A600, A1200)

**Safe Approach:**
```asm
; Check Kickstart version
    MOVE.L 4.W,A6
    MOVE.W LIB_VERSION(A6),D0
    CMP.W #36,D0            ; V36 = Kickstart 2.0
    BLT OldKickstart

; Use newer functions
    ; ...
```

**Rule:** Target lowest common Kickstart or check version.

---

## Summary of Critical Rules

1. ✅ **Graphics/audio data in Chip RAM** - SECTION with _C suffix
2. ✅ **Word/long on even addresses** - use EVEN/CNOP
3. ✅ **Privileged instructions need supervisor mode** - take over system
4. ✅ **A7 is stack pointer** - don't use as general register
5. ✅ **LEA for addresses** - faster than MOVE.L #
6. ✅ **MOVEM for multiple registers** - more efficient
7. ✅ **DBcc for counted loops** - efficient looping
8. ✅ **Wait for blitter before use** - check busy bit
9. ✅ **Copper lists in Chip RAM** - DMA requirement
10. ✅ **BSR for local, JSR for far** - range limitations
11. ✅ **Bits numbered 0-31** - not 1-32
12. ✅ **MOVEQ #0 faster than CLR** - for data registers
13. ✅ **Save/restore system state** - when taking over
14. ✅ **Check Kickstart version** - if using newer features

---

**This document should be consulted before writing any 68000 assembly lesson code.**
