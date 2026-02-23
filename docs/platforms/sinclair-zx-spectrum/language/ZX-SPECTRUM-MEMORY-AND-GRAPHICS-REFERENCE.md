# ZX Spectrum Memory & Graphics Quick Reference

**Purpose:** Fast lookup for ZX Spectrum hardware, memory layout, and graphics programming
**Audience:** ZX Spectrum programmers and curriculum designers
**For comprehensive details:** See ZX Spectrum Technical Manual

---

## Memory Map

### ZX Spectrum 48K Memory Layout

```
$0000-$3FFF (0-16383):     ROM (16KB)
$4000-$57FF (16384-22527): Screen bitmap (6144 bytes)
$5800-$5AFF (22528-23295): Screen attributes (768 bytes)
$5B00-$FFFF (23296-65535): User RAM (~42KB)
```

### ROM ($0000-$3FFF)

**Contents:**
- BASIC interpreter
- System routines
- Character set (96 characters × 8 bytes)

**Useful ROM Routines:**
- `$0D6B` - PRINT_A (print character in A register)
- `$0F2C` - CLS (clear screen)
- `$0ECD` - PRINT_AT (set print position)
- `$1601` - BEEP (generate tone)
- `$15EF` - BREAK_KEY (check for BREAK)

### Screen Memory ($4000-$5AFF)

**Screen Bitmap ($4000-$57FF):**
- 256×192 pixels (6144 bytes)
- 1 bit per pixel (set = INK colour, clear = PAPER colour)
- Non-linear layout (see Screen Layout section)

**Screen Attributes ($5800-$5AFF):**
- 32×24 character cells (768 bytes)
- 1 byte per 8×8 pixel cell
- Format: `FBPPPIII`
  - `F` = FLASH (bit 7)
  - `B` = BRIGHT (bit 6)
  - `PPP` = PAPER colour (bits 5-3)
  - `III` = INK colour (bits 2-0)

### User RAM ($5B00-$FFFF)

**System Variables ($5B00-$5C00):**
- `$5C00` (23552) - KSTATE (keyboard state)
- `$5C08` (23560) - LAST_K (last key pressed)
- `$5C3B` (23611) - BORDCR (border colour)
- `$5C6A` (23658) - FRAMES (frame counter, 50Hz)

**BASIC Area:**
- Program text
- Variables
- Arrays
- String space

**Screen Buffer Alternative:**
- Can use $C000-$D7FF as second screen (BANK switching on 128K)

---

## Screen Layout

### Bitmap Address Calculation

**Problem:** Screen memory is NOT linear. Consecutive rows are NOT consecutive in memory.

**Memory Organisation (thirds):**
```
Top third:    $4000-$47FF (rows 0-63)
Middle third: $4800-$4FFF (rows 64-127)
Bottom third: $5000-$57FF (rows 128-191)
```

**Within each third (8 scan line groups):**
```
Scan 0: $4000, $4100, $4200... (every 256 bytes)
Scan 1: $4020, $4120, $4220... (every 256 bytes)
Scan 2: $4040, $4140, $4240... (every 256 bytes)
...
Scan 7: $40E0, $41E0, $42E0... (every 256 bytes)
```

**Formula for pixel at (x, y):**
```
Address = $4000 + ((y AND 7) * 32) + ((y AND 56) * 4) + ((y AND 192) * 32) + (x / 8)
Bit = 7 - (x MOD 8)
```

**BASIC Implementation:**
```basic
10 REM Calculate screen address for pixel (x, y)
20 LET y3=(y AND 7)*32+(y AND 56)*4+(y AND 192)*32
30 LET addr=16384+y3+INT (x/8)
40 LET bit=7-(x-INT (x/8)*8)
50 REM POKE addr,PEEK addr OR (2^bit)  : REM Set pixel
```

**Assembly (Z80) Implementation:**
```asm
; Input: B = Y coordinate (0-191)
;        C = X coordinate (0-255)
; Output: HL = screen address
;         A = pixel mask
ScreenAddr:
    LD A,B              ; A = Y
    AND $07             ; A = Y AND 7 (scan line in group)
    OR $40              ; Set bit 6 (screen base $4000)
    LD H,A              ; H = high byte
    
    LD A,B              ; A = Y
    AND $18             ; Isolate bits 3-4 (third)
    RRCA                ; Shift right 3 times
    RRCA
    RRCA
    LD L,A              ; L = (Y AND $18) * 32
    
    LD A,B              ; A = Y
    AND $C0             ; Isolate bits 6-7 (group in third)
    RRCA                ; Shift right 3 times
    RRCA
    RRCA
    ADD A,L             ; Add to L
    LD L,A              ; L = offset within third
    
    LD A,C              ; A = X
    SRL A               ; Divide by 8
    SRL A
    SRL A
    ADD A,L             ; Add column
    LD L,A              ; HL = screen address
    
    LD A,C              ; A = X
    AND $07             ; A = bit position (0-7)
    LD B,A              ; Save bit position
    LD A,$80            ; Start with leftmost bit
    JR Z,DoneBit        ; If bit 0, done
BitLoop:
    SRL A               ; Shift right
    DJNZ BitLoop        ; Repeat B times
DoneBit:
    RET                 ; A = pixel mask, HL = address
```

### Attribute Address Calculation

**Much simpler - attributes ARE linear:**

```
Address = $5800 + (row * 32) + column
Where: row = 0-23, column = 0-31
```

**BASIC:**
```basic
10 LET addr=23568+(row*32)+col
20 POKE addr,value
```

**Assembly:**
```asm
; Input: B = row (0-23), C = column (0-31)
; Output: HL = attribute address
AttrAddr:
    LD H,$58            ; Attribute base
    LD A,B              ; A = row
    ADD A,A             ; Multiply by 32
    ADD A,A
    ADD A,A
    ADD A,A
    ADD A,A
    LD L,A              ; L = row * 32
    LD A,C              ; A = column
    ADD A,L             ; Add column
    LD L,A              ; HL = attribute address
    RET
```

---

## Graphics Hardware

### Display Timing

**PAL Timing:**
- 50Hz refresh (50 frames per second)
- 312 scan lines per frame
- 192 visible scan lines
- 120 lines in vertical blanking

**Frame Counter:**
```basic
10 REM FRAMES is at address 23672 (3 bytes, low-endian)
20 LET frames=PEEK 23672+256*PEEK 23673+65536*PEEK 23674
30 REM Increments 50 times per second
```

**Timing Code with FRAMES:**
```basic
10 REM Wait 1 second (50 frames)
20 LET start=PEEK 23672
30 IF PEEK 23672-start<50 THEN GO TO 30

40 REM Time-based animation
50 LET x=0
60 LET t=PEEK 23672
70 LET x=(PEEK 23672-t)*4: REM 4 pixels per frame
80 PLOT x,88
90 IF x<255 THEN GO TO 60
```

### Colour System

**8 Standard Colours (0-7):**
```
0: BLACK      (RGB: 0,0,0)
1: BLUE       (RGB: 0,0,255)
2: RED        (RGB: 255,0,0)
3: MAGENTA    (RGB: 255,0,255)
4: GREEN      (RGB: 0,255,0)
5: CYAN       (RGB: 0,255,255)
6: YELLOW     (RGB: 255,255,0)
7: WHITE      (RGB: 255,255,255)
```

**BRIGHT Modifier:**
- `BRIGHT 0` = Normal intensity
- `BRIGHT 1` = Increased intensity (lighter colours)
- Total: 16 displayable colours (8 normal + 8 bright)

**Attribute Byte Format:**
```
Bit 7: FLASH (0=steady, 1=flashing INK/PAPER swap every 16 frames)
Bit 6: BRIGHT (0=normal, 1=bright)
Bits 5-3: PAPER colour (0-7)
Bits 2-0: INK colour (0-7)
```

**Creating Attribute Byte:**
```basic
10 REM Calculate attribute byte
20 LET ink=2: LET paper=6: LET bright=1: LET flash=0
30 LET attr=(flash*128)+(bright*64)+(paper*8)+ink
40 PRINT attr           : REM 114 (binary: 01110010)

50 REM Set attribute at row 10, col 15
60 POKE 23552+(10*32)+15,attr
```

### Colour Clash

**Problem:** Attributes are 8×8 pixel cells. Cannot have 2 different INK colours in same cell.

**Example of Clash:**
```
+---------+---------+
| Cell 1  | Cell 2  |
| INK 2   | INK 4   | ← OK: Different cells
| PAPER 0 | PAPER 0 |
+---------+---------+
    ↑
If sprite overlaps cell boundary,
both parts must share same INK/PAPER
```

**Mitigation Strategies:**

1. **Monochrome Sprites:**
   - Use same colour for all sprite pixels
   - Accept background colour in sprite area

2. **Aligned Movement:**
   - Move sprites in 8-pixel steps
   - Keep sprites within character cell boundaries

3. **Strategic Backgrounds:**
   - Design levels around 8×8 grid
   - Use single background colour per area

4. **Accept Limitation:**
   - Embrace authentic ZX aesthetic
   - Part of the platform's character

---

## Pixel Operations

### Direct Pixel Manipulation

**Set Pixel (OR operation):**
```basic
10 REM Turn pixel ON at (x, y)
20 LET addr=16384+((y AND 7)*32)+((y AND 56)*4)+((y AND 192)*32)+(x/8)
30 LET bit=7-(x-INT (x/8)*8)
40 POKE addr,PEEK addr OR (2^bit)
```

**Clear Pixel (AND NOT operation):**
```basic
10 REM Turn pixel OFF at (x, y)
20 LET addr=16384+((y AND 7)*32)+((y AND 56)*4)+((y AND 192)*32)+(x/8)
30 LET bit=7-(x-INT (x/8)*8)
40 POKE addr,PEEK addr AND (255-(2^bit))
```

**Test Pixel:**
```basic
10 IF POINT (x,y)=1 THEN PRINT "PIXEL IS SET"
```

### Fast Clear Screen

**BASIC CLS is slow. Faster methods:**

```basic
10 REM Fast clear (16.7ms vs CLS 200ms)
20 FOR addr=16384 TO 22527
30   POKE addr,0
40 NEXT addr

50 REM Even faster with LDIR (assembly)
60 CLEAR 32767           : REM Set RAMTOP
70 POKE 32768,195        : REM JP instruction
80 POKE 32769,0: POKE 32770,128  : REM Address $8000
90 REM ... machine code at $8000 using LDIR ...
```

**Assembly LDIR Clear:**
```asm
; Ultra-fast clear (2.5ms)
ClearScreen:
    LD HL,$4000         ; Screen start
    LD DE,$4001         ; Destination
    LD BC,6143          ; Count (6144-1)
    LD (HL),0           ; Clear first byte
    LDIR                ; Repeat for entire screen
    RET
```

---

## User-Defined Graphics (UDG)

### UDG System

**Location:** 21 characters (A-U) redefinable
**Address:** `USR "a"` to `USR "u"`
**Default:** 23296-23463 (168 bytes = 21 × 8)

**Define UDG:**
```basic
10 REM Define smiley face as character "a"
20 FOR i=0 TO 7
30   READ byte
40   POKE USR "a"+i,byte
50 NEXT i
60 PRINT "a"           : REM Display UDG

100 DATA 60,66,165,129,165,153,66,60
```

**Byte Format (8 bits = 8 pixels):**
```
Binary:  00111100  (60 decimal)
Display: ..####..

Complete smiley:
00111100  ..####..
01000010  .#....#.
10100101  #.#..#.#
10000001  #......#
10100101  #.#..#.#
10011001  #..##..#
01000010  .#....#.
00111100  ..####..
```

**Using UDGs in Games:**
```basic
10 REM Define player sprite
20 RESTORE 1000
30 FOR i=0 TO 7
40   READ byte: POKE USR "a"+i,byte
50 NEXT i

60 REM Define enemy sprite
70 RESTORE 2000
80 FOR i=0 TO 7
90   READ byte: POKE USR "b"+i,byte
100 NEXT i

110 REM Draw sprites
120 PRINT AT 10,10;"a"  : REM Player
130 PRINT AT 15,20;"b"  : REM Enemy

1000 DATA 24,36,102,255,219,24,36,102
2000 DATA 60,126,219,255,126,60,36,66
```

---

## Advanced Graphics Techniques

### XOR Drawing (OVER 1)

**Purpose:** Draw/erase without affecting background

```basic
10 REM Setup
20 OVER 1               : REM Enable XOR mode

30 REM Draw sprite
40 PLOT x,y: DRAW 8,0: DRAW 0,8: DRAW -8,0: DRAW 0,-8

50 PAUSE 5

60 REM Erase (draw again with OVER 1)
70 PLOT x,y: DRAW 8,0: DRAW 0,8: DRAW -8,0: DRAW 0,-8

80 REM Move and repeat
90 LET x=x+4
100 GO TO 40
```

**Assembly XOR:**
```asm
; XOR byte at (HL) with pattern in A
XorByte:
    XOR (HL)            ; XOR with screen memory
    LD (HL),A           ; Write back
    RET
```

### Masking

**Purpose:** Preserve background around irregular sprites

```basic
10 REM Masked sprite (player with transparent background)
20 REM Step 1: AND with mask (clear sprite area)
30 LET byte=PEEK addr
40 LET masked=byte AND mask_byte
50 POKE addr,masked

60 REM Step 2: OR with sprite (draw sprite)
70 LET byte=PEEK addr
80 LET final=byte OR sprite_byte
90 POKE addr,final
```

### Double Buffering

**Purpose:** Eliminate flicker

**Spectrum 128K has 2 screens:**
- Screen 0: $4000-$5AFF (normal)
- Screen 1: $C000-$D7FF (shadow)

```basic
10 REM Draw to shadow screen
20 OUT 32765,7          : REM Display normal, write to shadow

30 REM ... draw frame to shadow screen ...

40 REM Flip screens
50 OUT 32765,5          : REM Display shadow, write to normal

60 REM ... draw next frame to normal screen ...

70 REM Repeat
80 OUT 32765,7
90 GO TO 30
```

---

## Performance Optimization

### Fast Integer Math

**Multiplication by powers of 2 (bit shifts):**
```basic
10 REM Multiply by 2, 4, 8, 16...
20 LET x2=x*2           : REM Slow
30 LET x2=x+x           : REM Faster

40 REM Multiply by 4
50 LET x4=(x+x)+(x+x)   : REM Fast

60 REM Multiply by 8
70 LET x8=x*8           : REM Slow
80 LET x8=(x+x)+(x+x)+(x+x)+(x+x)  : REM Fast
```

**Division by powers of 2:**
```basic
10 LET x_div_2=INT (x/2)    : REM Slow
20 LET x_div_2=INT (x*0.5)  : REM Slightly faster

30 REM Assembly is much faster (SRL, SRA)
```

### Lookup Tables

**For expensive calculations (SIN, COS, SQRT):**
```basic
10 REM Pre-calculate sine table
20 DIM sintab(360)
30 FOR angle=0 TO 360
40   LET sintab(angle)=SIN (angle*PI/180)
50 NEXT angle

60 REM Use in game loop (instant lookup)
70 LET y=100+sintab(angle)*50
```

### Minimize PRINT

**PRINT is extremely slow (~1-5ms per character)**

```basic
10 REM Slow (prints every frame)
20 PRINT AT 0,0;"SCORE: ";score
30 GO TO 20

40 REM Faster (only update when changed)
50 LET old_score=score
60 IF score<>old_score THEN PRINT AT 0,0;"SCORE: ";score
70 LET old_score=score
80 GO TO 60
```

---

## Assembly Language Integration

### Calling Machine Code from BASIC

**Method 1: USR Function**
```basic
10 REM Load machine code at address 32768
20 CLEAR 32767          : REM Set RAMTOP below code
30 FOR i=0 TO length-1
40   READ byte
50   POKE 32768+i,byte
60 NEXT i

70 REM Call machine code
80 LET result=USR 32768

100 DATA 62,10,201      : REM LD A,10 / RET (returns 10)
```

**Method 2: RANDOMIZE USR**
```basic
10 RANDOMIZE USR 32768  : REM Call code, ignore return value
```

**Return Value:**
```
BC register = return value
A register = also available via PEEK after call
```

### Common Assembly Routines

**Fast Plot:**
```asm
; Input: B = Y (0-191), C = X (0-255)
; Modifies: AF, HL
FastPlot:
    CALL ScreenAddr     ; HL = screen address, A = bit mask
    OR (HL)             ; Set bit
    LD (HL),A
    RET
```

**Fast Sprite (8×8):**
```asm
; Input: B = Y, C = X, IX = sprite data (8 bytes)
; Modifies: AF, BC, DE, HL
DrawSprite:
    CALL ScreenAddr     ; HL = screen address
    LD B,8              ; 8 rows
DrawLoop:
    LD A,(IX+0)         ; Get sprite byte
    LD (HL),A           ; Write to screen
    INC IX              ; Next sprite byte
    INC H               ; Next scan line (same column)
    DJNZ DrawLoop
    RET
```

---

## Memory Management

### Available RAM

**48K Spectrum:**
- ROM: 16KB (unusable for programs)
- Screen: 6.75KB (23296 bytes)
- System: ~1.5KB
- **Available: ~41KB** for BASIC program, variables, arrays

**Calculating Free Memory:**
```basic
10 PRINT "Free RAM: ";(PEEK 23730+256*PEEK 23731)-(PEEK 23653+256*PEEK 23654);" bytes"
```

### Memory Conservation

**Tips:**
1. **Use short variable names** - `sc` not `score` (saves 0 bytes, but faster lookup)
2. **Reuse variables** - Temporary variables can be reused
3. **Arrays vs individual variables** - Arrays are more memory-efficient
4. **Delete unused lines** - Renumber program to reclaim space
5. **String arrays** - Pre-allocate with DIM to avoid fragmentation

---

## Hardware Registers

### ULA (Uncommitted Logic Array)

**Port $FE (254) - Keyboard and Border:**

**Read:**
- Bits 0-4: Keyboard input (active low)
- Bit 6: Tape EAR input

**Write:**
- Bits 0-2: Border colour
- Bit 3: MIC output
- Bit 4: EAR output

**Reading Keyboard:**
```basic
10 REM Read keys (CAPS to V)
20 LET keys=IN 65278    : REM Port $FEFE
30 IF keys AND 1=0 THEN PRINT "CAPS SHIFT"
40 IF keys AND 2=0 THEN PRINT "Z"
50 IF keys AND 4=0 THEN PRINT "X"
60 IF keys AND 8=0 THEN PRINT "C"
70 IF keys AND 16=0 THEN PRINT "V"
```

**Setting Border:**
```basic
10 OUT 254,colour       : REM 0-7
```

---

## Quick Reference Tables

### System Variables

| Address | Name | Purpose |
|---------|------|---------|
| 23296 | RAMTOP | Top of RAM |
| 23552 | KSTATE | Keyboard state |
| 23560 | LAST_K | Last key pressed |
| 23611 | BORDCR | Border colour |
| 23658 | PROG | BASIC program start |
| 23672 | FRAMES | Frame counter (3 bytes) |
| 23730 | UDG | UDG character start |

### Screen Regions

| Address Range | Purpose | Size |
|--------------|---------|------|
| $4000-$47FF | Top third bitmap | 2KB |
| $4800-$4FFF | Middle third bitmap | 2KB |
| $5000-$57FF | Bottom third bitmap | 2KB |
| $5800-$5AFF | Attributes | 768B |

### Keyboard Half-Rows

| Port | Bits | Keys |
|------|------|------|
| $FEFE (65278) | 0-4 | CAPS V C X Z |
| $FDFE (65022) | 0-4 | A S D F G |
| $FBFE (64510) | 0-4 | Q W E R T |
| $F7FE (63486) | 0-4 | 1 2 3 4 5 |
| $EFFE (61438) | 0-4 | 0 9 8 7 6 |
| $DFFE (57342) | 0-4 | P O I U Y |
| $BFFE (49150) | 0-4 | ENTER L K J H |
| $7FFE (32766) | 0-4 | SPACE SYM M N B |

---

**Version:** 1.0
**Created:** 2025-10-24
**For:** ZX Spectrum Phase 0 Programming

**See Also:**
- ZX-SPECTRUM-BASIC-QUICK-REFERENCE.md (language commands)
- ZX-SPECTRUM-ASSEMBLY-REFERENCE.md (Z80 instructions)

**Complete:** ZX Spectrum reference documentation set finished!
