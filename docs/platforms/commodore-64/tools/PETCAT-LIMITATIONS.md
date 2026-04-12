# petcat Limitations and Semantic Verification Checklist

## Overview

`petcat` is the Commodore BASIC tokenizer/detokenizer from the VICE emulator suite. It validates **syntax** but NOT **semantics**—meaning it can accept code that compiles but doesn't work correctly on real C64 hardware.

## What petcat DOES Validate

✓ **Syntax**: Keywords, line numbers, statement structure
✓ **Tokenization**: Converts BASIC keywords to tokens
✓ **Line format**: Proper BASIC V2 line structure

## What petcat DOES NOT Validate

❌ **Register addresses**: Can't verify if POKE/PEEK uses valid hardware registers
❌ **Value ranges**: Can't check if POKE values exceed 0-255
❌ **C64 BASIC V2 capabilities**: Accepts features from later BASIC versions
❌ **Hardware behavior**: Can't verify collision registers, MSB logic, etc.
❌ **Boundary values**: Can't check sprite coordinates, screen limits
❌ **Bit mask logic**: Can't verify CIA bit patterns, sprite enable bits

## Critical Example: RESTORE with Line Numbers

```basic
420 RESTORE 430    REM ✓ petcat accepts this (valid syntax)
                   REM ❌ But doesn't work in C64 BASIC V2!
```

**Why this is dangerous:**
- `RESTORE <line>` is valid syntax in BASIC 4.0, BASIC 7.0, C128
- petcat accepts it as syntactically valid
- C64 BASIC V2 ignores the line number argument
- Code runs without error but behaves incorrectly
- Students learn wrong techniques

## Semantic Verification Checklist

When writing or reviewing C64 BASIC lessons, manually verify:

### 1. Register Addresses (POKE/PEEK)

**VIC-II Registers:**
- [ ] 53248-53294: Valid VIC-II range
- [ ] 53248-53263: Sprite X/Y positions (0-7)
- [ ] 53264: Sprite X MSB register
- [ ] 53269: Sprite enable register
- [ ] 53278-53279: Collision registers
- [ ] 53280-53294: Border, background, sprite colors

**SID Registers:**
- [ ] 54272-54300: Valid SID range
- [ ] 54272-54278: Voice 1 registers
- [ ] 54279-54285: Voice 2 registers
- [ ] 54286-54292: Voice 3 registers
- [ ] 54296: Volume register

**CIA Registers:**
- [ ] 56320: Port A (Joystick Port 2)
- [ ] 56321: Port B (Joystick Port 1)
- [ ] 56320-56335: Valid CIA #1 range

**Memory:**
- [ ] 1024-2023: Screen RAM (valid for default configuration)
- [ ] 55296-56295: Color RAM
- [ ] 2040-2047: Sprite pointers
- [ ] 832-16383: Sprite data (anywhere in current VIC bank)

### 2. C64 BASIC V2 Language Features

**DOES NOT EXIST in C64 BASIC V2:**
- [ ] RESTORE with line number argument (`RESTORE 100` doesn't work)
- [ ] DO...LOOP structures
- [ ] WHILE...WEND
- [ ] Named procedures (PROC/ENDPROC)
- [ ] String functions beyond LEFT$, RIGHT$, MID$, LEN
- [ ] Advanced string manipulation

**DOES EXIST in C64 BASIC V2:**
- [ ] OR and AND operators (contrary to some docs!)
- [ ] RESTORE (no argument—resets to first DATA statement)
- [ ] ON...GOSUB with calculated index
- [ ] FOR...NEXT loops
- [ ] IF...THEN...ELSE (ELSE was added in BASIC V2)

### 3. POKE Value Ranges

- [ ] All POKE values are 0-255 (BASIC validates this)
- [ ] Variables exceeding 255 will cause ILLEGAL QUANTITY ERROR
- [ ] MSB handling required for sprite X coordinates >255

Example of correct MSB handling:
```basic
IF X>255 THEN POKE 53264,1:POKE 53248,X-256 ELSE POKE 53264,0:POKE 53248,X
```

### 4. Sprite Boundaries

**Screen coordinates:**
- [ ] X: 24 (left edge) to 320 (right edge, safe)
  - Absolute max: 344, but requires MSB management
  - Sprite width: 24 pixels
  - Right edge calculation: X + 24
- [ ] Y: 50 (top edge) to 229 (bottom edge)
  - Sprite height: 21 pixels
  - Bottom edge calculation: Y + 21

**MSB register (53264):**
- [ ] Sprite 0 uses bit 0 (value 1)
- [ ] Sprite 1 uses bit 1 (value 2)
- [ ] Sprite 2 uses bit 2 (value 4)
- [ ] Combined: sum the values

### 5. Bit Mask Logic

**Joystick (CIA):**
- [ ] AND 31 isolates joystick bits (0-4)
- [ ] Bit 0 (UP) = mask 1
- [ ] Bit 1 (DOWN) = mask 2
- [ ] Bit 2 (LEFT) = mask 4
- [ ] Bit 3 (RIGHT) = mask 8
- [ ] Bit 4 (FIRE) = mask 16
- [ ] Active-low: 0=pressed, 1=released

**Sprite enable (53269):**
- [ ] Bit flags, not sequential values
- [ ] Sprite 0 = bit 0 (value 1)
- [ ] Sprite 1 = bit 1 (value 2)
- [ ] Sprites 0+1 = 1+2 = 3, NOT 2
- [ ] All 8 sprites = 255 (binary 11111111)

### 6. Hardware Behavior

**Collision registers (53278, 53279):**
- [ ] Reading the register CLEARS it
- [ ] Store value before checking: `C=PEEK(53278)`
- [ ] Don't PEEK twice in same frame
- [ ] Background collision requires foreground color (not color 0)

**SID chip:**
- [ ] ADSR registers MUST be initialized for sound
- [ ] Voice 1: 54277 (Attack/Decay), 54278 (Sustain/Release)
- [ ] Voice 2: 54284, 54285
- [ ] Voice 3: 54291, 54292
- [ ] Control register bit 0 is gate (1=on, 0=release)

**Sprite MSB:**
- [ ] Required when X coordinate >255
- [ ] Must check BEFORE POKE to avoid ILLEGAL QUANTITY ERROR
- [ ] Low byte = X - 256 when X>255

### 7. DATA Statement Management

**C64 BASIC V2 limitations:**
- [ ] RESTORE takes NO arguments (unlike BASIC 4.0+)
- [ ] Always resets to first DATA statement
- [ ] No way to skip to specific DATA line
- [ ] Must READ sequentially through unwanted data
- [ ] Must organize DATA sequentially by usage order

## Verification Workflow

1. **Run petcat** to validate syntax:
   ```bash
   petcat -w2 -o program.prg -- program.bas
   ```

2. **Manual semantic check** against this checklist

3. **Test on real hardware** or VICE emulator:
   ```bash
   x64sc -autostart program.prg
   ```

4. **Watch for runtime errors:**
   - ILLEGAL QUANTITY ERROR → POKE value >255
   - OUT OF DATA ERROR → READ exceeds DATA statements
   - Unexpected behavior → Semantic error

## Common Semantic Errors That petcat Misses

1. **RESTORE with line numbers** (our lesson 031 error)
   - petcat accepts: `RESTORE 430`
   - C64 ignores line number, always resets to first DATA

2. **Sprite X > 255 without MSB handling**
   - petcat accepts: `POKE 53248,X` where X=300
   - C64 crashes with ILLEGAL QUANTITY ERROR

3. **Wrong collision register behavior**
   - petcat accepts: `IF PEEK(53278)>0 THEN POKE 53280,2`
   - C64 never detects collision (first PEEK clears register)

4. **Wrong bit mask values**
   - petcat accepts: `IF (J AND 2)=0 THEN PRINT "LEFT"`
   - C64 prints "LEFT" when DOWN pressed (bit 1, not LEFT)

5. **Missing ADSR initialization**
   - petcat accepts SID code without ADSR setup
   - C64 produces no sound (zero sustain level)

## Documentation References

When in doubt, verify against:
- **C64 Programmer's Reference Guide** (official manual)
- **Mapping the Commodore 64** (memory map)
- **C64 Wiki** (community documentation)
- **VICE documentation** (emulator-specific details)

## Related Documents

- `/docs/BASIC-V2-REFERENCE.md` - Complete C64 BASIC V2 command reference
- `/docs/PHASE-0-CURRICULUM.md` - Curriculum specifications
- `/docs/LESSON-PREFLIGHT-CHECKLIST.md` - Pre-publication verification

---

**Last verified:** 2025-01-26
**Context:** After fixing lesson 031 RESTORE error and verifying all modified lessons
