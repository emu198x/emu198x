# ZX Spectrum BASIC Quick Reference

**Purpose:** Fast lookup for ZX Spectrum BASIC V keywords and syntax
**Audience:** ZX Spectrum BASIC programmers and curriculum designers
**For comprehensive details:** See ZX Spectrum BASIC Programming Manual

---

## Essential Commands

### Program Control

| Command | Syntax | Purpose |
|---------|--------|---------|
| RUN | `RUN` | Execute program from start |
| STOP | `STOP` | Halt program execution |
| CONTINUE | `CONTINUE` or `CONT` | Resume after STOP |
| GOTO | `GOTO line` | Jump to line number |
| GO SUB | `GO SUB line` | Call subroutine |
| RETURN | `RETURN` | Return from subroutine |
| PAUSE | `PAUSE frames` | Wait (50 frames = 1 second) |

### Variables and Input

| Command | Syntax | Purpose |
|---------|--------|---------|
| LET | `LET variable=expression` | Assign value (LET is optional) |
| INPUT | `INPUT variable` | Get user input |
| INPUT prompt | `INPUT "prompt"; variable` | Prompted input |
| DIM | `DIM array(size)` | Declare array |
| CLEAR | `CLEAR` | Reset variables |

**Variable Types:**
- Numeric: `score`, `x`, `y`
- String: `name$`, `text$`
- Array: `scores(10)`, `names$(5)`

### Conditionals and Loops

| Command | Syntax | Purpose |
|---------|--------|---------|
| IF...THEN | `IF condition THEN statement` | Conditional execution |
| IF...THEN line | `IF condition THEN line` | Conditional jump |
| FOR...NEXT | `FOR var=start TO end` | Counting loop |
| FOR...STEP | `FOR var=start TO end STEP inc` | Loop with increment |
| NEXT | `NEXT var` | End loop |

**Examples:**
```basic
10 IF score>100 THEN PRINT "WIN!"
20 IF lives=0 THEN GOTO 500

30 FOR i=1 TO 10
40   PRINT i
50 NEXT i

60 FOR x=0 TO 255 STEP 16
70   PLOT x,100
80 NEXT x
```

### Output

| Command | Syntax | Purpose |
|---------|--------|---------|
| PRINT | `PRINT expression` | Display text/numbers |
| PRINT AT | `PRINT AT row,col;"text"` | Print at position |
| PRINT separators | `PRINT a; b, c` | `;` = no space, `,` = tab |
| CLS | `CLS` | Clear screen |

**Screen Positions:**
- Rows: 0-21 (22 rows)
- Columns: 0-31 (32 columns)

---

## Graphics Commands

### Drawing Primitives

| Command | Syntax | Purpose |
|---------|--------|---------|
| PLOT | `PLOT x,y` | Draw pixel |
| DRAW | `DRAW dx,dy` | Draw line from last point |
| DRAW angles | `DRAW x,y,angle` | Draw with direction |
| CIRCLE | `CIRCLE x,y,radius` | Draw circle |
| POINT | `POINT (x,y)` | Test pixel (1=set, 0=clear) |

**Coordinate System:**
- X: 0-255 (256 pixels wide)
- Y: 0-175 (176 pixels high)
- Origin: Bottom-left corner

**Examples:**
```basic
10 PLOT 128,88        : REM Centre pixel
20 DRAW 50,0          : REM Horizontal line
30 DRAW 0,50          : REM Vertical line
40 CIRCLE 128,88,40   : REM Circle at centre
```

### Colour and Attributes

| Command | Syntax | Purpose |
|---------|--------|---------|
| INK | `INK colour` | Set foreground colour |
| PAPER | `PAPER colour` | Set background colour |
| BORDER | `BORDER colour` | Set border colour |
| BRIGHT | `BRIGHT 0/1` | Normal/bright colours |
| FLASH | `FLASH 0/1` | Disable/enable flashing |
| INVERSE | `INVERSE 0/1` | Normal/inverted colours |
| OVER | `OVER 0/1` | Normal/XOR drawing |

**Standard Colours (0-7):**
```
0: BLACK     4: GREEN
1: BLUE      5: CYAN
2: RED       6: YELLOW
3: MAGENTA   7: WHITE
```

**Attribute System:**
- 8×8 pixel character cells
- Each cell: 1 INK + 1 PAPER + BRIGHT + FLASH
- Cannot have 2 INK colours in same cell

**Examples:**
```basic
10 PAPER 0: INK 7: CLS        : REM White on black
20 BORDER 2                    : REM Red border
30 BRIGHT 1: INK 4             : REM Bright green text
40 PRINT AT 10,10; "HELLO"

50 INK 2: PAPER 6              : REM Red on yellow
60 FLASH 1                     : REM Enable flashing
70 PRINT "DANGER"
```

---

## Sound

### BEEP Command

| Command | Syntax | Purpose |
|---------|--------|---------|
| BEEP | `BEEP duration,pitch` | Generate tone |

**Parameters:**
- `duration`: Seconds (e.g., 0.5, 1, 2)
- `pitch`: Semitones from middle C (0 = middle C, 12 = octave higher, -12 = octave lower)

**Examples:**
```basic
10 BEEP 0.5,0         : REM Middle C for half second
20 BEEP 0.1,12        : REM High C, short beep
30 BEEP 0.2,-12       : REM Low C
40 BEEP 1,0           : REM Long tone

50 REM Simple melody
60 BEEP 0.2,0: BEEP 0.2,2: BEEP 0.2,4: BEEP 0.4,5
```

**Musical Scale (from middle C):**
```
-12: Low C    0: C      12: High C
-10: Low D    2: D      14: High D
-8:  Low E    4: E      16: High E
-7:  Low F    5: F      17: High F
-5:  Low G    7: G      19: High G
-3:  Low A    9: A      21: High A
-1:  Low B    11: B     23: High B
```

---

## Input and Control

### Keyboard Input

| Command | Syntax | Purpose |
|---------|--------|---------|
| INKEY$ | `LET key$=INKEY$` | Read key (no wait) |
| INPUT | `INPUT variable` | Wait for input + ENTER |
| PAUSE | `PAUSE 0` | Wait for keypress |

**INKEY$ Returns:**
- Empty string `""` if no key pressed
- Single character if key pressed
- Works in loops for game controls

**Examples:**
```basic
10 REM Game loop with INKEY$
20 LET key$=INKEY$
30 IF key$="q" THEN PRINT "UP"
40 IF key$="a" THEN PRINT "DOWN"
50 IF key$="" THEN REM No key pressed
60 GO TO 20

100 REM Wait for specific key
110 IF INKEY$<>"" THEN GO TO 110  : REM Clear buffer
120 IF INKEY$="" THEN GO TO 120   : REM Wait for press
```

---

## String Operations

| Function | Syntax | Purpose |
|----------|--------|---------|
| LEN | `LEN string$` | String length |
| VAL | `VAL string$` | Convert string to number |
| STR$ | `STR$ number` | Convert number to string |
| CHR$ | `CHR$ code` | Character from ASCII code |
| CODE | `CODE string$` | ASCII code of first char |
| String slice | `string$(start TO end)` | Extract substring |

**Examples:**
```basic
10 LET name$="SPECTRUM"
20 PRINT LEN name$             : REM 8
30 PRINT name$(1 TO 4)         : REM "SPEC"
40 PRINT name$(5 TO )          : REM "TRUM"

50 LET score=1500
60 LET text$="SCORE: "+STR$ score
70 PRINT text$                 : REM "SCORE: 1500"

80 LET input$="42"
90 LET num=VAL input$          : REM 42
```

---

## Mathematical Functions

| Function | Syntax | Purpose |
|----------|--------|---------|
| ABS | `ABS x` | Absolute value |
| INT | `INT x` | Integer part (floor) |
| SQR | `SQR x` | Square root |
| SIN | `SIN x` | Sine (radians) |
| COS | `COS x` | Cosine (radians) |
| TAN | `TAN x` | Tangent (radians) |
| ATN | `ATN x` | Arctangent |
| EXP | `EXP x` | e^x |
| LN | `LN x` | Natural log |
| PI | `PI` | π constant (3.14159...) |
| RND | `RND` | Random 0-0.999... |

**Random Numbers:**
```basic
10 REM Random integer 1-6 (dice)
20 LET dice=INT (RND*6)+1

30 REM Random integer min-max
40 LET num=INT (RND*(max-min+1))+min

50 REM Random X coordinate
60 LET x=INT (RND*256)
```

---

## Program Management

### Listing and Editing

| Command | Syntax | Purpose |
|---------|--------|---------|
| LIST | `LIST` | Show all lines |
| LIST range | `LIST start TO end` | Show line range |
| LIST line | `LIST line` | Show single line |
| EDIT | `EDIT line` | Edit line (Spectrum 128K+) |

### Storage

| Command | Syntax | Purpose |
|---------|--------|---------|
| SAVE | `SAVE "filename"` | Save program to tape |
| LOAD | `LOAD "filename"` | Load program from tape |
| VERIFY | `VERIFY "filename"` | Check program on tape |
| NEW | `NEW` | Delete program |

---

## Operators

### Arithmetic

| Operator | Purpose | Example |
|----------|---------|---------|
| `+` | Addition | `x+5` |
| `-` | Subtraction | `y-2` |
| `*` | Multiplication | `a*3` |
| `/` | Division | `score/10` |
| `^` | Power | `2^8` (256) |

### Comparison

| Operator | Purpose | Example |
|----------|---------|---------|
| `=` | Equal | `IF x=10` |
| `<>` | Not equal | `IF x<>0` |
| `<` | Less than | `IF x<100` |
| `>` | Greater than | `IF x>0` |
| `<=` | Less or equal | `IF x<=10` |
| `>=` | Greater or equal | `IF x>=5` |

### Logical

| Operator | Purpose | Example |
|----------|---------|---------|
| `AND` | Logical AND | `IF x>0 AND x<10` |
| `OR` | Logical OR | `IF key$="q" OR key$="a"` |
| `NOT` | Logical NOT | `IF NOT flag` |

---

## Common Patterns

### Game Loop Structure

```basic
10 REM Initialize
20 LET score=0
30 LET lives=3
40 CLS

50 REM Main loop
60 LET key$=INKEY$
70 REM Handle input
80 REM Update game state
90 REM Draw screen
100 GO TO 60
```

### Simple Animation

```basic
10 REM Bouncing ball
20 LET x=128: LET y=88
30 LET dx=2: LET dy=2

40 REM Animation loop
50 PLOT x,y
60 PAUSE 2
70 PLOT x,y              : REM Erase (OVER 1 for XOR)

80 LET x=x+dx: LET y=y+dy

90 IF x<0 OR x>255 THEN LET dx=-dx
100 IF y<0 OR y>175 THEN LET dy=-dy

110 GO TO 50
```

### Collision Detection

```basic
10 REM Check if point touches pixel
20 IF POINT (x,y)=1 THEN PRINT "HIT!"

30 REM Check rectangle overlap
40 IF x1<x2+w2 AND x1+w1>x2 THEN
50   IF y1<y2+h2 AND y1+h1>y2 THEN
60     PRINT "COLLISION!"
70   END IF
80 END IF
```

### Score Display

```basic
10 REM Score in top-right
20 PRINT AT 0,25;"SC:";score

30 REM Formatted score (6 digits)
40 LET s$=STR$ score
50 LET s$="000000"(1 TO 6-LEN s$)+s$
60 PRINT AT 0,20;"SCORE ";s$
```

---

## Memory and System

### Memory Addresses

| Address | Purpose |
|---------|---------|
| 23296 | RAMTOP - top of usable RAM |
| 23552 | Screen bitmap start |
| 23672 | Printer buffer |
| 23730 | UDG (user-defined graphics) |

**Accessing Memory:**
```basic
10 POKE address,value    : REM Write byte
20 LET value=PEEK address: REM Read byte
```

### User-Defined Graphics

```basic
10 REM Define UDG character
20 FOR i=0 TO 7
30   READ byte
40   POKE USR "a"+i,byte
50 NEXT i
60 PRINT "a"            : REM Print UDG

100 DATA 60,66,165,129,165,153,66,60  : REM Smiley face
```

---

## Error Handling

**Common Errors:**
- `0 OK` - Statement completed
- `2 Variable not found` - Undeclared variable
- `3 Subscript wrong` - Array index out of bounds
- `4 Out of memory` - Program too large
- `9 STOP statement` - Program stopped
- `A Invalid argument` - Invalid function parameter
- `B Integer out of range` - Number too large

**Error Recovery:**
```basic
10 REM Check before division
20 IF divisor<>0 THEN LET result=value/divisor

30 REM Clamp array access
40 IF index<1 OR index>10 THEN LET index=1
50 LET value=array(index)
```

---

## Quick Tips

### Performance

- **Use integers when possible** - `INT (x)` faster than floats
- **Minimise PRINT** - Screen updates are slow
- **Cache calculations** - Store `SIN`, `COS` in arrays
- **Avoid string concatenation in loops** - Build strings once

### Memory

- **Variable names** - First 2 chars matter: `score` = `sc123`
- **String arrays** - Preallocate with `DIM name$(10)`
- **CLEAR statement** - Resets all variables, arrays

### Debugging

- **STOP statement** - Pause execution
- **CONTINUE** - Resume after STOP
- **PRINT for debugging** - Display variable values
- **LIST specific lines** - `LIST 100 TO 120`

---

## Colour Clash Management

**Problem:** Attribute system = 8×8 pixel cells, each cell = 1 INK + 1 PAPER

**Solutions:**

1. **Monochrome sprites** - Same colour as background
2. **Design around 8×8 grid** - Align sprites to character cells
3. **Strategic colour choices** - Use paper colour for sprite outline
4. **Accept the limitation** - Part of authentic ZX aesthetic

**Example:**
```basic
10 REM White sprite on black background
20 PAPER 0: INK 7: CLS

30 REM Sprite moves in 8-pixel steps
40 LET x=0
50 FOR x=0 TO 240 STEP 8
60   PRINT AT 10,x/8; CHR$ 144  : REM UDG sprite
70   PAUSE 5
80   PRINT AT 10,x/8; " "       : REM Erase
90 NEXT x
```

---

## Complete Example: Simple Pong Bat

```basic
10 REM Simple Pong Bat
20 PAPER 0: INK 7: CLS
30 BORDER 0

40 REM Initialize bat
50 LET baty=80

60 REM Main loop
70 LET key$=INKEY$
80 IF key$="q" AND baty<160 THEN LET baty=baty+4
90 IF key$="a" AND baty>8 THEN LET baty=baty-4

100 REM Draw bat (vertical line)
110 PLOT 10,baty
120 DRAW 0,16

130 PAUSE 2

140 REM Erase bat
150 PLOT 10,baty
160 DRAW 0,16

170 GO TO 70
```

---

**Version:** 1.0
**Created:** 2025-10-24
**For:** ZX Spectrum Phase 0 BASIC Programming

**See Also:**
- ZX-SPECTRUM-MEMORY-MAP.md (hardware details)
- ZX-SPECTRUM-GRAPHICS-REFERENCE.md (advanced graphics)

**Next:** ZX Spectrum Memory & Graphics Quick Reference
