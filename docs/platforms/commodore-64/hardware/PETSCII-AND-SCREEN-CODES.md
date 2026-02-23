# PETSCII and Screen Codes - Authoritative Reference

**Purpose:** Complete character code reference for C64 programming
**Replaces:** PETSCII-SCREEN-CODES.md, SCREEN-CODES-REFERENCE.md, PETSCII-REFERENCE.md
**Audience:** Anyone programming C64 in BASIC or assembly

---

## The Critical Difference

The C64 uses **two different character encoding systems**:

### PETSCII (PET Standard Code of Information Interchange)
**Used with:**
- BASIC: `PRINT "A"`, `CHR$(65)`, `ASC("A")`, `GET A$`, `INPUT`
- Assembly: KERNAL CHROUT ($FFD2), CHRIN ($FFCF), GETIN ($FFE4)

### Screen Codes
**Used with:**
- BASIC: `POKE 1024,1` (direct screen memory writes)
- Assembly: `STA $0400` (writing to screen RAM)

**They are NOT the same!**

| Character | PETSCII | Screen Code |
|-----------|---------|-------------|
| A | 65 | 1 |
| Space | 32 | 32 |
| @ | 64 | 0 |
| Heart (♥) | 83 | 83 |

---

## Memory Locations

| Memory | Address Range | Decimal | Purpose |
|--------|---------------|---------|---------|
| **Screen RAM** | $0400-$07E7 | 1024-2023 | Character positions (40×25 = 1000 bytes) |
| **Color RAM** | $D800-$DBE7 | 55296-56295 | Color for each position (low 4 bits) |

**Formula:**
- Position = 1024 + (row × 40) + column
- Color = 55296 + (row × 40) + column

---

## Quick Conversion Rules

### PETSCII → Screen Code

**Letters A-Z (uppercase):**
```
Screen Code = PETSCII - 64
Example: 'A' = 65 (PETSCII) → 1 (Screen)
```

**Letters a-z (lowercase/graphics - shifted mode):**
```
Screen Code = PETSCII - 96
Example: 'a' = 97 (PETSCII) → 1 (Screen)
```

**Numbers and most symbols (32-63):**
```
Screen Code = PETSCII (same)
Example: '0' = 48 (both)
```

**Graphics characters (128-255):**
```
Varies - see tables below
```

### Screen Code → PETSCII

**To display a character:**
```
PETSCII = Screen Code + 64 (for codes 0-31)
PETSCII = Screen Code (for codes 32-63)
PETSCII = Screen Code + 32 (for codes 64-95)
PETSCII = Screen Code + 64 (for codes 96-127)
```

**Better:** Use the lookup tables below rather than formulas.

---

## Common Characters (Uppercase Mode)

| Char | PETSCII | Screen | Notes |
|------|---------|--------|-------|
| **Space** | 32 | 32 | Same |
| **!** | 33 | 33 | Same |
| **"** | 34 | 34 | Same |
| **#** | 35 | 35 | Same |
| **$** | 36 | 36 | Same |
| **%** | 37 | 37 | Same |
| **&** | 38 | 38 | Same |
| **'** | 39 | 39 | Same |
| **(** | 40 | 40 | Same |
| **)** | 41 | 41 | Same |
| **\*** | 42 | 42 | Same |
| **+** | 43 | 43 | Same |
| **,** | 44 | 44 | Same |
| **-** | 45 | 45 | Same |
| **.** | 46 | 46 | Same |
| **/** | 47 | 47 | Same |
| **0-9** | 48-57 | 48-57 | Same |
| **:** | 58 | 58 | Same |
| **;** | 59 | 59 | Same |
| **<** | 60 | 60 | Same |
| **=** | 61 | 61 | Same |
| **>** | 62 | 62 | Same |
| **?** | 63 | 63 | Same |
| **@** | 64 | 0 | **Different!** |
| **A-Z** | 65-90 | 1-26 | **Different!** |
| **[** | 91 | 27 | Different |
| **£** | 92 | 28 | Pound sign |
| **]** | 93 | 29 | Different |
| **↑** | 94 | 30 | Up arrow |
| **←** | 95 | 31 | Left arrow |

**Pattern for 32-63:** PETSCII = Screen Code (same)
**Pattern for 64-95:** Screen Code = PETSCII - 64

---

## Graphics Characters (Uppercase Mode)

| Description | PETSCII | Screen | Visual |
|-------------|---------|--------|--------|
| Horizontal line | 67 | 3 | ─ |
| Vertical line | 66 | 2 | │ |
| Upper left corner | 85 | 21 | ╭ |
| Upper right corner | 73 | 9 | ╮ |
| Lower left corner | 74 | 10 | ╰ |
| Lower right corner | 75 | 11 | ╯ |
| Cross | 78 | 14 | ┼ |
| T-junction left | 69 | 5 | ├ |
| T-junction right | 70 | 6 | ┤ |
| T-junction up | 71 | 7 | ┴ |
| T-junction down | 72 | 8 | ┬ |
| Heart ♥ | 83 | 83 | Same |
| Diamond ♦ | 90 | 90 | Same |
| Club ♣ | 88 | 88 | Same |
| Spade ♠ | 65 | 65 | Same |
| Circle ● | 81 | 81 | Same |
| Solid block █ | 160 | 160 | Same |

---

## Control Characters (PETSCII Only)

These are **not** displayable - they control output.

| Code | CHR$() | Description | Effect |
|------|--------|-------------|--------|
| **5** | CHR$(5) | White color | Sets text to white |
| **13** | CHR$(13) | Return | Carriage return (ENTER) |
| **14** | CHR$(14) | Lowercase mode | Switch to lowercase charset |
| **17** | CHR$(17) | Cursor down | Move cursor down 1 line |
| **18** | CHR$(18) | Reverse on | Enable reverse video |
| **19** | CHR$(19) | Home | Move cursor to home position |
| **28** | CHR$(28) | Red | Text color red |
| **29** | CHR$(29) | Cursor right | Move cursor right 1 space |
| **30** | CHR$(30) | Green | Text color green |
| **31** | CHR$(31) | Blue | Text color blue |
| **129** | CHR$(129) | Orange | Text color orange |
| **144** | CHR$(144) | Black | Text color black |
| **145** | CHR$(145) | Cursor up | Move cursor up 1 line |
| **146** | CHR$(146) | Reverse off | Disable reverse video |
| **147** | CHR$(147) | **Clear screen** | Clear screen and home |
| **148** | CHR$(148) | Insert | Insert mode |
| **149** | CHR$(149) | Brown | Text color brown |
| **150** | CHR$(150) | Light red | Text color light red |
| **151** | CHR$(151) | Dark gray | Text color dark gray |
| **152** | CHR$(152) | Medium gray | Text color medium gray |
| **153** | CHR$(153) | Light green | Text color light green |
| **154** | CHR$(154) | Light blue | Text color light blue |
| **155** | CHR$(155) | Light gray | Text color light gray |
| **156** | CHR$(156) | Purple | Text color purple |
| **157** | CHR$(157) | Cursor left | Move cursor left 1 space |
| **158** | CHR$(158) | Yellow | Text color yellow |
| **159** | CHR$(159) | Cyan | Text color cyan |

### Common Control Character Uses

```basic
PRINT CHR$(147)     : REM Clear screen
PRINT CHR$(5)       : REM White text
PRINT CHR$(19)      : REM Home cursor
PRINT CHR$(13)      : REM Carriage return
PRINT CHR$(18)"HI"CHR$(146)  : REM Reverse video "HI" then normal
```

---

## Color Codes (for PETSCII and Color RAM)

### PETSCII Color Control

| Color | PETSCII | CHR$() |
|-------|---------|--------|
| Black | 144 | CHR$(144) |
| White | 5 | CHR$(5) |
| Red | 28 | CHR$(28) |
| Cyan | 159 | CHR$(159) |
| Purple | 156 | CHR$(156) |
| Green | 30 | CHR$(30) |
| Blue | 31 | CHR$(31) |
| Yellow | 158 | CHR$(158) |
| Orange | 129 | CHR$(129) |
| Brown | 149 | CHR$(149) |
| Light Red | 150 | CHR$(150) |
| Dark Gray | 151 | CHR$(151) |
| Medium Gray | 152 | CHR$(152) |
| Light Green | 153 | CHR$(153) |
| Light Blue | 154 | CHR$(154) |
| Light Gray | 155 | CHR$(155) |

### Color RAM Values (for POKEing)

| Color | Value | Hex |
|-------|-------|-----|
| Black | 0 | $00 |
| White | 1 | $01 |
| Red | 2 | $02 |
| Cyan | 3 | $03 |
| Purple | 4 | $04 |
| Green | 5 | $05 |
| Blue | 6 | $06 |
| Yellow | 7 | $07 |
| Orange | 8 | $08 |
| Brown | 9 | $09 |
| Light Red | 10 | $0A |
| Dark Gray | 11 | $0B |
| Medium Gray | 12 | $0C |
| Light Green | 13 | $0D |
| Light Blue | 14 | $0E |
| Light Gray | 15 | $0F |

---

## Complete PETSCII Table

### PETSCII 0-127 (Uppercase Mode)

| Code | Char | Notes | Code | Char | Notes |
|------|------|-------|------|------|-------|
| 0-31 | - | Control chars | 64 | @ | |
| 32 | Space | | 65 | A | |
| 33 | ! | | 66 | B | |
| 34 | " | | 67 | C | |
| 35 | # | | 68 | D | |
| 36 | $ | | 69 | E | |
| 37 | % | | 70 | F | |
| 38 | & | | 71 | G | |
| 39 | ' | | 72 | H | |
| 40 | ( | | 73 | I | |
| 41 | ) | | 74 | J | |
| 42 | * | | 75 | K | |
| 43 | + | | 76 | L | |
| 44 | , | | 77 | M | |
| 45 | - | | 78 | N | |
| 46 | . | | 79 | O | |
| 47 | / | | 80 | P | |
| 48-57 | 0-9 | Digits | 81 | Q | |
| 58 | : | | 82 | R | |
| 59 | ; | | 83 | S | |
| 60 | < | | 84 | T | |
| 61 | = | | 85 | U | |
| 62 | > | | 86 | V | |
| 63 | ? | | 87 | W | |
| | | | 88 | X | |
| | | | 89 | Y | |
| | | | 90 | Z | |
| | | | 91 | [ | |
| | | | 92 | £ | Pound |
| | | | 93 | ] | |
| | | | 94 | ↑ | Up arrow |
| | | | 95 | ← | Left arrow |

### PETSCII 128-255

Codes 128-255 include:
- More control characters (128-159)
- Shifted characters/graphics (160-191)
- Reversed characters (192-255)

**For complete tables, see original PETSCII-REFERENCE.md**

---

## Complete Screen Code Table

### Screen Codes 0-127

| Code | Char (Uppercase) | PETSCII Equiv |
|------|------------------|---------------|
| 0 | @ | 64 |
| 1-26 | A-Z | 65-90 |
| 27 | [ | 91 |
| 28 | £ | 92 |
| 29 | ] | 93 |
| 30 | ↑ | 94 |
| 31 | ← | 95 |
| 32-63 | Same as PETSCII | Space, digits, symbols |
| 64-95 | Graphics/symbols | Varies |
| 96-127 | Reversed chars | Varies |

**For complete tables with graphics, see original SCREEN-CODES-REFERENCE.md**

---

## Practical Examples

### Example 1: Write Text to Screen (BASIC)

```basic
10 REM Display "HELLO" at top-left corner
20 POKE 1024,8  : REM 'H' = screen code 8
30 POKE 1025,5  : REM 'E' = screen code 5
40 POKE 1026,12 : REM 'L' = screen code 12
50 POKE 1027,12 : REM 'L' = screen code 12
60 POKE 1028,15 : REM 'O' = screen code 15
70 REM Make it red
80 FOR I=0 TO 4: POKE 55296+I,2: NEXT
```

### Example 2: Convert PETSCII to Screen Code (Assembly)

```assembly
; Convert PETSCII character in A to screen code
; Handles codes 32-95 (space through underscore)
PETSCII_TO_SCREEN:
    CMP #32
    BCC CONTROL     ; < 32 = control character (not displayable)
    CMP #64
    BCC DONE        ; 32-63 are the same
    CMP #96
    BCC IS_UPPER    ; 64-95 need conversion
    SBC #96         ; 96+ subtract 96
    JMP DONE
IS_UPPER:
    SBC #64         ; Subtract 64 (carry already set from CMP)
DONE:
    RTS
CONTROL:
    LDA #32         ; Return space for control chars
    RTS
```

### Example 3: Clear Screen with Character (Assembly)

```assembly
; Fill screen with solid blocks
CLEAR_SCREEN:
    LDA #160        ; Screen code for solid block
    LDX #0
LOOP:
    STA $0400,X     ; Page 1
    STA $0500,X     ; Page 2
    STA $0600,X     ; Page 3
    STA $0700,X     ; Page 4 (partial - only first 232 bytes used)
    INX
    BNE LOOP
    RTS
```

### Example 4: Keyboard Input to Screen (BASIC)

```basic
10 GET A$
20 IF A$="" THEN 10
30 P=ASC(A$)        : REM Get PETSCII code
40 IF P<32 OR P>95 THEN 10 : REM Skip control chars
50 IF P>=64 THEN S=P-64 ELSE S=P : REM Convert to screen code
60 POKE 1024,S      : REM Display at top-left
70 PRINT "PETSCII:";P;" SCREEN:";S
80 GOTO 10
```

---

## Common Conversion Scenarios

### Scenario 1: Display User Input
```basic
GET A$              : REM Returns PETSCII
IF A$<>"" THEN PRINT A$  : REM PRINT handles PETSCII automatically
```

### Scenario 2: Draw Text Manually
```basic
A$="HELLO"
FOR I=1 TO LEN(A$)
    P=ASC(MID$(A$,I,1))     : REM Get PETSCII
    S=P-64 IF P>=64 ELSE P  : REM Convert to screen code
    POKE 1024+I-1,S         : REM Poke to screen
NEXT
```

### Scenario 3: Read Screen Character
```basic
S=PEEK(1024)        : REM Get screen code
P=S+64 IF S<64 ELSE S : REM Convert to approximate PETSCII
PRINT CHR$(P)       : REM Display
```

---

## Shifted Mode (Lowercase/Graphics)

Press **Shift + Commodore** to switch to lowercase mode.

In lowercase mode:
- Codes 65-90 become lowercase letters a-z
- Uppercase A-Z require shift key
- Screen codes change for shifted characters

**For lesson creation:** Usually stay in uppercase mode for clarity.

---

## Tips for Lesson Creation

### Beginner Lessons (1-10)
- Use PRINT with PETSCII only
- Avoid direct screen memory POKEs
- Stick to simple text and control codes

### Intermediate Lessons (11-15)
- Introduce screen memory POKEs
- Show PETSCII vs screen code difference
- Use graphics characters for simple effects

### Advanced Lessons (16+)
- Custom character sets (modify screen codes)
- Direct screen manipulation for speed
- Conversion routines for efficiency

---

## See Also

- **C64-MEMORY-MAP.md** - Screen and color RAM locations
- **VIC-II-QUICK-REFERENCE.md** - Screen control registers
- **BASIC-V2-REFERENCE.md** - PRINT, CHR$(), ASC() commands

---

**Document Version:** 1.0 - Consolidated Reference
**Replaces:** PETSCII-SCREEN-CODES.md, SCREEN-CODES-REFERENCE.md, PETSCII-REFERENCE.md
**Synthesized:** 2025 for Code Like It's 198x curriculum
