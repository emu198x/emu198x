# NES Common Errors and Pitfalls

**Purpose:** Document common mistakes in NES assembly programming and how to avoid them
**Audience:** NES curriculum designers and assembly programmers
**Last Updated:** 2025-10-30

---

## Critical Hardware Quirks

### 1. NMI Timing Race Condition

**Problem:** Enabling NMI while VBlank flag is already set immediately triggers NMI.

**Symptoms:**
- NMI handler runs twice per frame
- Game logic executes too fast
- Sprite flickering or glitches

**Wrong:**
```asm
; Wait for VBlank
:   BIT $2002
    BPL :-

; Enable NMI - RACE CONDITION!
LDA #%10000000
STA $2000       ; If VBlank still set, NMI triggers immediately
```

**Correct:**
```asm
; Wait for VBlank to START
:   BIT $2002
    BPL :-

; Wait for VBlank to END
:   BIT $2002
    BMI :-

; NOW enable NMI safely
LDA #%10000000
STA $2000
```

**Explanation:** Reading $2002 clears the VBlank flag, but it may be set again immediately if you're still in VBlank. Always wait for VBlank to end before enabling NMI.

---

### 2. PPUADDR/PPUSCROLL Write Latch

**Problem:** PPUADDR and PPUSCROLL use a shared write latch that toggles between first/second write. If not reset, writes go to wrong byte.

**Symptoms:**
- Background draws at wrong address
- Scroll position incorrect
- Nametable corruption

**Wrong:**
```asm
LDA #$20
STA $2006       ; First write (high byte)
LDA #$00
STA $2006       ; Second write (low byte)

; ... later code ...

LDA #$21        ; Trying to set new address
STA $2006       ; ERROR: This is treated as second write!
```

**Correct:**
```asm
LDA $2002       ; ALWAYS read PPUSTATUS to reset latch
LDA #$20
STA $2006       ; First write (high byte)
LDA #$00
STA $2006       ; Second write (low byte)

; ... later code ...

LDA $2002       ; Reset latch again
LDA #$21
STA $2006       ; Now correctly first write
LDA #$00
STA $2006       ; Second write
```

**Rule:** ALWAYS read $2002 before any PPUADDR or PPUSCROLL writes.

---

### 3. PPUDATA Read Buffer

**Problem:** Reading from PPUDATA ($2007) returns buffered data from the PREVIOUS read, not the current address.

**Symptoms:**
- First byte read is garbage
- Data appears offset by one byte

**Wrong:**
```asm
LDA $2002       ; Reset latch
LDA #$20
STA $2006       ; Point to $2000
LDA #$00
STA $2006

LDA $2007       ; Read byte - WRONG: Returns buffered garbage!
STA $0200       ; Stores wrong data
```

**Correct:**
```asm
LDA $2002       ; Reset latch
LDA #$20
STA $2006       ; Point to $2000
LDA #$00
STA $2006

LDA $2007       ; Dummy read (fills buffer)
LDA $2007       ; THIS read returns correct data from $2000
STA $0200       ; Stores correct data
```

**Exception:** Palette RAM ($3F00-$3FFF) does NOT use the read buffer - first read is correct.

---

### 4. Sprite 0 Hit at X=255

**Problem:** Hardware bug in 2C02 PPU - sprite 0 hit detection fails when sprite X position is 255.

**Symptoms:**
- Sprite 0 hit never triggers
- Status bar effects don't work
- Raster split fails

**Wrong:**
```asm
LDA #255
STA $0203       ; Sprite 0 X position - HIT WON'T TRIGGER!
```

**Correct:**
```asm
LDA #254        ; Use 254 or less
STA $0203       ; Sprite 0 X position - hit works correctly
```

**Also Required for Sprite 0 Hit:**
- Rendering must be enabled (PPUMASK bits 3 and 4)
- Sprite 0 must overlap non-transparent background pixel
- Y position must be within visible scanlines (0-239)

---

### 5. OAMDATA vs OAMDMA

**Problem:** Writing sprites via OAMDATA ($2004) is extremely slow - 1 byte per write requires 256 writes × 4 cycles = 1,024 cycles minimum.

**Symptoms:**
- Game runs slowly
- Not enough time in VBlank for other updates
- Visible slowdown

**Wrong (but works):**
```asm
LDA #$00
STA $2003       ; Set OAM address

LDX #0
:   LDA sprite_data,X
    STA $2004       ; Write one byte - SLOW!
    INX
    BNE :-          ; 256 iterations = 1000+ cycles wasted
```

**Correct:**
```asm
LDA #$00
STA $2003       ; Set OAM address
LDA #>sprite_data  ; High byte of sprite buffer ($0200)
STA $4014       ; OAMDMA - copies 256 bytes in 513 cycles!
```

**Rule:** ALWAYS use OAMDMA ($4014) for sprite updates. OAMDATA is only for testing or single-byte updates.

---

### 6. DPCM Audio Corrupts Controller Reads

**Problem:** On NTSC NES (not PAL), DMC (delta-modulated audio) DMA can corrupt controller reads by stealing CPU cycles.

**Symptoms:**
- Random button presses detected
- Controls feel unresponsive
- Intermittent controller issues

**Mitigation 1 - Multiple Reads:**
```asm
; Read controller twice and compare
JSR read_controller
STA temp1
JSR read_controller
STA temp2

LDA temp1
CMP temp2
BNE read_again      ; If different, read again
```

**Mitigation 2 - OAM DMA Sync:**
```asm
; Read controller immediately after OAM DMA
; DMA locks out DPCM temporarily

LDA #$00
STA $2003
LDA #>oam_buffer
STA $4014           ; OAMDMA

; Read controller NOW while DPCM is blocked
JSR read_controller
```

**Best Practice:** Read controllers at consistent frame position, preferably after OAM DMA.

---

## Timing and VBlank Issues

### 7. PPU Writes During Rendering

**Problem:** Writing to PPU registers while rendering is enabled causes visual glitches.

**Symptoms:**
- Screen corruption
- Flickering sprites
- Wrong colors displayed

**Wrong:**
```asm
; Rendering is ON (PPUMASK = %00011110)

LDA #$3F
STA $2006       ; Writing during rendering - GLITCH!
```

**Correct Option 1 - Use VBlank:**
```asm
NMI_Handler:
    ; We're in VBlank - safe to write
    LDA #$3F
    STA $2006
    ; ... more writes ...
    RTI
```

**Correct Option 2 - Disable Rendering:**
```asm
; Turn off rendering
LDA #%00000000
STA $2001

; Now safe to write
LDA #$3F
STA $2006
; ... more writes ...

; Re-enable rendering
LDA #%00011110
STA $2001
```

**Rule:** PPU memory writes must happen during VBlank (in NMI handler) or with rendering disabled.

---

### 8. Not Enough VBlank Time

**Problem:** VBlank lasts ~2,273 CPU cycles (NTSC). Complex updates can exceed this.

**Symptoms:**
- Graphics corruption
- Incomplete nametable updates
- Mid-frame visual glitches

**Wrong:**
```asm
NMI_Handler:
    ; Trying to update entire nametable in VBlank
    LDX #0
:   LDA nametable_data,X
    STA $2007           ; 4 cycles per write
    INX
    CPX #240            ; 960 bytes = 3,840+ cycles - TOO SLOW!
    BNE :-
    RTI
```

**Correct:**
```asm
NMI_Handler:
    ; Update only what changed (20-30 bytes typically)
    LDX #0
:   LDA changed_tiles,X
    STA $2007
    INX
    CPX changed_count   ; Only update changed tiles
    BNE :-
    RTI
```

**VBlank Budget (NTSC):**
- Available: ~2,273 CPU cycles
- OAMDMA: 513 cycles
- Remaining: ~1,760 cycles for other updates
- PPUDATA write: ~4 cycles each
- Max writes: ~440 bytes (but leave margin for logic)

**Best Practice:** Update 20-50 bytes per frame maximum. Queue larger updates across multiple frames.

---

## Memory and Addressing

### 9. Decimal Mode Not Supported

**Problem:** NES 6502 has decimal mode disabled in hardware. SED instruction does nothing.

**Symptoms:**
- BCD arithmetic doesn't work
- ADC/SBC produce binary results always

**Wrong:**
```asm
SED             ; Enable decimal mode - NO EFFECT ON NES!
LDA #$09
ADC #$01        ; Expecting $10 (BCD), get $0A (binary)
```

**Correct:**
```asm
; Use binary arithmetic only on NES
CLC
LDA #9
ADC #1          ; Result: 10 (binary $0A)
```

**Rule:** Never use SED on NES. Implement BCD in software if needed.

---

### 10. Zero Page vs Absolute Addressing

**Problem:** Using absolute addressing when zero page would work wastes bytes and cycles.

**Performance:**
```asm
LDA $0080       ; Zero page: 2 bytes, 3 cycles
LDA $0180       ; Absolute:  3 bytes, 4 cycles
```

**Best Practice:**
```asm
; Put frequently accessed variables in zero page ($00-$FF)
player_x = $10
player_y = $11
buttons  = $12

; Fast access
LDA player_x    ; 2 bytes, 3 cycles
```

**Rule:** Reserve zero page ($00-$FF) for hot variables. Use absolute addressing ($0100+) for cold data.

---

## Interrupt Vectors

### 11. Missing or Wrong Interrupt Vectors

**Problem:** NES reads interrupt vectors at $FFFA-$FFFF. If missing or wrong, NES won't boot or will crash.

**Required Vectors:**
```
$FFFA-$FFFB: NMI vector
$FFFC-$FFFD: RESET vector (REQUIRED)
$FFFE-$FFFF: IRQ vector
```

**Wrong:**
```asm
; No vectors defined - NES won't boot!
```

**Correct (ca65 syntax):**
```asm
.segment "VECTORS"
.addr NMI_Handler
.addr RESET_Handler     ; MUST point to valid code
.addr IRQ_Handler
```

**Correct (raw assembly):**
```asm
.org $FFFA
.word NMI_Handler
.word RESET_Handler
.word IRQ_Handler
```

**Rule:** RESET vector is mandatory. NMI should be defined if using rendering. IRQ can point to RTI if unused.

---

## Sprite and Graphics Issues

### 12. Sprite Overflow Flag Unreliable

**Problem:** Sprite overflow flag (PPUSTATUS bit 5) has hardware bugs and doesn't reliably detect >8 sprites per scanline.

**Symptoms:**
- Flag doesn't set when expected
- Inconsistent behavior

**Wrong:**
```asm
LDA $2002
AND #%00100000      ; Check sprite overflow
BNE too_many_sprites ; DON'T RELY ON THIS!
```

**Correct:**
```asm
; Count sprites in software before uploading to OAM
LDX #0              ; Sprite counter
LDY #0              ; Scanline counter

:   LDA sprite_y,X
    CMP current_scanline
    BNE :+
    INY             ; Count sprite on this scanline
    CPY #9
    BCS too_many    ; Software check: >8 sprites
:   INX
    CPX sprite_count
    BNE :--
```

**Rule:** Don't trust sprite overflow flag. Count sprites in software if limiting is required.

---

### 13. Palette Address Mirroring

**Problem:** Palette RAM ($3F00-$3F1F) mirrors certain addresses, causing confusion.

**Mirrored Addresses:**
- $3F00 = $3F10 (universal background color)
- $3F04 = $3F14
- $3F08 = $3F18
- $3F0C = $3F1C

**Wrong Assumption:**
```asm
; Thinking each palette has 4 unique colors
LDA #$0F
STA $3F04       ; Background palette 1, color 0
; This ALSO sets sprite palette 1, color 0 ($3F14)!
```

**Correct Understanding:**
```asm
; Palette structure:
; $3F00: Universal background (mirrored to $3F10)
; $3F01-$3F03: BG palette 0, colors 1-3
; $3F05-$3F07: BG palette 1, colors 1-3
; $3F09-$3F0B: BG palette 2, colors 1-3
; $3F0D-$3F0F: BG palette 3, colors 1-3
; $3F11-$3F13: Sprite palette 0, colors 1-3
; $3F15-$3F17: Sprite palette 1, colors 1-3
; $3F19-$3F1B: Sprite palette 2, colors 1-3
; $3F1D-$3F1F: Sprite palette 3, colors 1-3
```

**Rule:** Color 0 of each palette shares the universal background color. Only colors 1-3 are unique per palette.

---

## Build and Toolchain Issues

### 14. ca65 Syntax vs Raw 6502

**Problem:** ca65 assembler has specific syntax requirements different from other assemblers.

**Common ca65-isms:**
```asm
; Immediate values
LDA #$20        ; Correct
LDA $20         ; Zero page, not immediate!

; Comments
LDA #$00        ; ca65 uses semicolon

; Hexadecimal
LDA #$FF        ; Must use $ prefix

; Labels
Label:          ; Must end with colon
    LDA #$00

; Segments
.segment "CODE"
.segment "RODATA"
.segment "VECTORS"
```

**Rule:** Check ca65 documentation for syntax. Use $ for hex, # for immediate, ; for comments.

---

### 15. Missing nes.cfg Linker Configuration

**Problem:** ca65 requires a linker configuration file to generate correct .nes ROM.

**Symptoms:**
- Linker errors
- ROM doesn't boot
- Wrong memory layout

**Required:**
```bash
# Assemble
ca65 -o program.o program.asm

# Link with nes.cfg
ld65 -C nes.cfg -o program.nes program.o
```

**Rule:** Always use `-C nes.cfg` with ld65. The config file defines iNES header, memory layout, and segments.

---

## Testing and Debugging

### 16. Emulator vs Hardware Differences

**Problem:** Some emulators are inaccurate and allow code that fails on real hardware.

**Common Discrepancies:**
- Timing differences
- PPU behavior during rendering
- DPCM DMA conflicts (often not emulated)

**Best Practice:**
- Test on accurate emulator (Mesen, FCEUX)
- Test on real hardware if possible
- Don't rely on emulator-specific behaviors

---

### 17. Off-by-One Errors in Loops

**Problem:** 6502 has no direct "loop N times" instruction, leading to fencepost errors.

**Wrong:**
```asm
LDX #8          ; Want to loop 8 times
:   ; ... loop body ...
    DEX
    BNE :-      ; Loops 8 times - CORRECT

LDX #8          ; Want to loop 8 times
:   ; ... loop body ...
    DEX
    BPL :-      ; Loops 9 times - OFF BY ONE!
```

**Correct:**
```asm
; Loop 8 times: X goes 7→0
LDX #8
:   DEX
    ; ... loop body ...
    BPL :-

; OR: X goes 8→1
LDX #8
:   ; ... loop body ...
    DEX
    BNE :-
```

**Rule:** Carefully choose loop bounds and branch condition. Test edge cases (0, 1, max iterations).

---

## Summary of Critical Rules

1. ✅ **Always read $2002 before PPUADDR/PPUSCROLL writes** (reset latch)
2. ✅ **Use OAMDMA ($4014), never OAMDATA ($2004)** for sprite updates
3. ✅ **Dummy read required for PPUDATA** (except palette RAM)
4. ✅ **Sprite 0 hit fails at X=255** - use 254 or less
5. ✅ **No decimal mode on NES** - SED does nothing
6. ✅ **PPU writes only during VBlank or rendering disabled**
7. ✅ **VBlank budget: ~2,273 cycles** - update 20-50 bytes max
8. ✅ **RESET vector mandatory** at $FFFC-$FFFD
9. ✅ **Read controllers after OAM DMA** to avoid DPCM corruption
10. ✅ **Palette color 0 mirrors** - $3F00 = $3F10

---

**This document should be consulted before writing any NES lesson code to avoid these common pitfalls.**
