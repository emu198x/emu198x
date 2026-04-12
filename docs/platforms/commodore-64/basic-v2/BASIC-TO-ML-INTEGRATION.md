# BASIC to Machine Language Integration

**Reference Source:** C64 Programmer's Reference Guide, Chapter 5: BASIC to Machine Language
**Purpose:** Methods for calling ML from BASIC programs
**Audience:** Curriculum designers teaching C64 assembly integration

---

## Overview

The C64 provides five methods to integrate machine language with BASIC:

1. **SYS** - Simplest: call ML subroutine and return
2. **USR** - Call ML function that returns a value
3. **I/O Vectors** - Intercept BASIC's input/output
4. **IRQ Vector** - Background tasks during program execution
5. **CHRGET Wedge** - Intercept BASIC interpreter itself

**For Lessons:** Start with SYS (lessons 1-10), introduce USR (lessons 11-15), advanced topics (IRQ, wedges) in lessons 16+.

---

## Method 1: SYS - Call Machine Language Subroutine

**What it does:** Calls ML routine at specified address, then returns to BASIC.

**Syntax:**
```basic
SYS <address>
```

**Example:**
```basic
10 REM CHANGE BORDER COLOR
20 FOR I=0 TO 14
30 READ A: POKE 49152+I,A
40 NEXT I
50 SYS 49152
60 DATA 169,1,141,32,208,96

Assembly equivalent:
LDA #1          ; A9 01
STA $D020       ; 8D 20 D0
RTS             ; 60
```

### How SYS Works Internally

1. BASIC saves all registers (A, X, Y, Status)
2. Calls ML routine via JSR
3. ML routine executes
4. **RTS returns to BASIC**
5. BASIC restores all registers

**Critical:** ML routine MUST end with **RTS**. Forgetting RTS = crash.

### Passing Parameters to SYS

**Problem:** SYS doesn't accept parameters directly.

**Solutions:**

#### A. PEEK Memory Locations

```basic
10 POKE 251,5           ; Parameter 1
20 POKE 252,10          ; Parameter 2
30 SYS 49152

Assembly:
LDA $FB         ; Get parameter 1 (5)
LDX $FC         ; Get parameter 2 (10)
; Do something with A and X
RTS
```

**Good locations:** $FB-$FE (safe for user programs)

#### B. Use BASIC Variables (Advanced)

Access BASIC variable storage directly (complex, not recommended for early lessons).

### Returning Values from SYS

**Problem:** SYS doesn't return values to BASIC directly.

**Solutions:**

#### A. POKE Result to Memory

```basic
10 SYS 49152
20 RESULT = PEEK(251)
30 PRINT "RESULT:"; RESULT

Assembly:
; Calculate result in A
STA $FB         ; Store to $FB
RTS
```

#### B. Use USR Function (See Method 2)

---

## Method 2: USR - Call Function That Returns Value

**What it does:** Calls ML function and returns 16-bit integer result to BASIC.

**Syntax:**
```basic
X = USR(<address>)
```

**Setup Required:**
```basic
10 POKE 785,<LO: POKE 786,>HI  ; Set USR vector
20 X = USR(0)                   ; Call function
```

### USR Example: Simple Calculation

```basic
10 REM SET USR VECTOR TO 49152
20 POKE 785,0: POKE 786,192
30 REM CALL USR FUNCTION
40 X = USR(0)
50 PRINT "RESULT:"; X

Assembly at $C000 (49152):
LDA #42         ; Calculate result (42)
TAY             ; Low byte in Y
LDA #0          ; High byte in A
RTS             ; Return to BASIC
```

**Return Convention:**
- **A** = high byte of result (MSB)
- **Y** = low byte of result (LSB)
- Result range: 0-65535 (16-bit unsigned)

### USR with Parameters (Floating Point)

**Advanced:** USR can accept floating-point parameter.

```basic
X = USR(3.14159)
```

**Parameter Location:** Floating-point accumulator (FAC1) at $0061-$0066

**Complex:** Requires understanding BASIC's floating-point format. Reserve for advanced lessons.

### USR vs SYS

| Feature | SYS | USR |
|---------|-----|-----|
| Returns value | No | Yes (16-bit) |
| Setup required | No | Yes (vector) |
| Typical use | Execute routine | Calculate value |
| Complexity | Simple | Moderate |

**For Lessons:** Use SYS for most examples. Introduce USR when functions/return values are conceptually needed.

---

## Method 3: I/O Vectors - Intercept Input/Output

**What it does:** Redirects BASIC's GET, INPUT, PRINT commands through custom ML handlers.

**Vectors:**
- **IGET** ($0324-$0325): Input from keyboard
- **ICHRIN** ($032A-$032B): Input from current device
- **ICHROUT** ($0326-$0327): Output to current device

**Use Cases:**
- Custom input handlers (e.g., joystick as keyboard)
- Screen effects during PRINT
- Data filtering/transformation

### Example: Custom PRINT Handler

```basic
10 REM INSTALL CUSTOM CHROUT
20 POKE 806,0: POKE 807,192
30 PRINT "HELLO"

Assembly at $C000:
; Custom character output
PHA             ; Save character
; Do custom effect (e.g., delay, color change)
PLA             ; Restore character
JMP $F1CA       ; Jump to normal KERNAL CHROUT
```

**Pattern:**
1. Save character (if needed)
2. Perform custom action
3. Jump to original KERNAL routine OR implement replacement

### Restoring Default Vectors

```basic
10 POKE 806,202: POKE 807,241  ; Restore CHROUT to $F1CA
```

**Or use KERNAL:**
```assembly
JSR RESTOR      ; $FF8A - Restore all vectors to defaults
```

**Caution:** If custom handler crashes, system hangs. **Always test carefully.**

---

## Method 4: IRQ Vector - Background Processing

**What it does:** Executes ML code 60 times per second (NTSC) during BASIC program execution.

**IRQ = Interrupt Request:** Hardware signal from VIC-II chip triggering routine.

**Default IRQ:** KERNAL routine that updates jiffy clock, scans keyboard, blinks cursor.

**Custom IRQ Use Cases:**
- Music players running during BASIC
- Sprite animation
- Raster effects
- Background timers

### IRQ Vector Setup

**Vector Location:** $0314-$0315 (low/high)

```basic
10 REM INSTALL CUSTOM IRQ
20 POKE 788,0: POKE 789,192    ; Point to $C000
30 REM ... BASIC CONTINUES RUNNING ...
40 REM CUSTOM IRQ RUNS 60 TIMES/SEC IN BACKGROUND
```

**Assembly at $C000:**
```assembly
; Custom IRQ handler
INC $D020       ; Flash border (example)

; CRITICAL: Jump to default IRQ or call RTI
JMP $EA31       ; Continue with normal KERNAL IRQ
; OR
; JMP $EA81     ; Exit IRQ without keyboard scan
; OR implement full replacement and use RTI
```

### Important IRQ Rules

1. **Preserve registers** - Save A, X, Y if you modify them
2. **Keep it fast** - IRQ runs 60 times/second; slow code = sluggish system
3. **Exit properly** - Jump to KERNAL IRQ ($EA31) OR use RTI
4. **Acknowledge interrupts** - If replacing fully, clear $D019

### Full IRQ Replacement Example

```assembly
IRQ_HANDLER:
    PHA             ; Save A
    TXA
    PHA             ; Save X
    TYA
    PHA             ; Save Y

    ; Your custom code here
    INC $D020       ; Example: increment border color

    ; Acknowledge IRQ
    LDA #$FF
    STA $D019       ; Clear VIC-II IRQ flag

    ; Restore registers
    PLA
    TAY
    PLA
    TAX
    PLA

    RTI             ; Return from interrupt
```

### Restoring Default IRQ

```basic
POKE 788,49: POKE 789,234    ; Restore to $EA31
```

**Or:**
```assembly
JSR RESTOR      ; $FF8A
```

**Testing Tip:** Install IRQ from BASIC, then type commands. If keyboard stops working, IRQ handler broke something.

---

## Method 5: CHRGET Wedge - Intercept BASIC Interpreter

**What it does:** Inserts custom ML code into BASIC's command interpretation loop.

**Use Cases:**
- Add new BASIC commands
- Intercept specific keywords
- Create BASIC extensions (e.g., "@LOAD" shortcut)

**Complexity:** HIGH - Requires deep understanding of BASIC interpreter.

**Recommendation:** Reserve for advanced lessons (lesson 20+) or omit entirely.

### How CHRGET Works

CHRGET is BASIC's "get next character" routine at $0073-$008A in zero page.

**Typical wedge pattern:**
```assembly
; Wedge installed at $0073
JMP MY_HANDLER  ; 3 bytes
; Original CHRGET continues at $0076

MY_HANDLER:
    ; Check for custom command
    CMP #'@        ; Is it '@'?
    BEQ CUSTOM_CMD
    ; Not custom - continue with normal CHRGET
    JMP $0076

CUSTOM_CMD:
    ; Handle custom command
    ; ...
    RTS
```

**Installation (simplified):**
```basic
POKE 115,76: POKE 116,0: POKE 117,192  ; JMP $C000
```

**Warning:** Corrupting CHRGET = BASIC stops working. **Save your work first!**

---

## Lesson Design Recommendations

### Beginner Lessons (1-10): SYS Only

**Focus:**
- Loading ML with DATA/POKE
- Calling with SYS
- Ending with RTS
- Simple effects (border, screen)

**Example Lesson:**
```basic
10 REM LESSON 3: YOUR FIRST ML PROGRAM
20 FOR I=0 TO 8
30 READ A: POKE 49152+I,A
40 NEXT I
50 PRINT "PRESS ANY KEY"
60 GET A$: IF A$="" THEN 60
70 SYS 49152
80 DATA 169,0,141,32,208,169,1,141,33,208,96
```

### Intermediate Lessons (11-15): USR and Parameters

**Add:**
- USR function calls
- Passing parameters via PEEK/POKE
- Returning values
- Simple calculations

**Example Lesson:**
```basic
10 REM LESSON 12: MULTIPLY TWO NUMBERS IN ML
20 POKE 785,0: POKE 786,192
30 POKE 251,7: POKE 252,6
40 X = USR(0)
50 PRINT "7 * 6 ="; X
```

### Advanced Lessons (16-20): IRQ and Vectors

**Add:**
- Background music with IRQ
- Custom I/O handlers
- Sprite multiplexing via IRQ
- Raster effects

**Example Lesson:**
```basic
10 REM LESSON 18: MUSIC PLAYER WITH IRQ
20 REM ... INSTALL IRQ HANDLER ...
30 REM BASIC CONTINUES, MUSIC PLAYS IN BACKGROUND
```

### Expert Topics (21+): CHRGET Wedges

**Only if appropriate:**
- BASIC extensions
- Custom commands
- Interpreter modifications

**Alternative:** Skip entirely - few real-world applications for beginners.

---

## Common Integration Patterns

### Pattern 1: One-Shot Effect

**Use:** SYS

```basic
10 SYS 49152    ; Execute effect
20 REM CONTINUE WITH BASIC
```

### Pattern 2: Repeated Calculation

**Use:** USR in loop

```basic
10 FOR I=1 TO 100
20 POKE 251,I
30 X=USR(0)     ; Calculate something
40 PRINT X
50 NEXT I
```

### Pattern 3: Background Task

**Use:** IRQ

```basic
10 REM INSTALL IRQ MUSIC PLAYER
20 POKE 788,0: POKE 789,192
30 REM GAME RUNS IN BASIC
40 REM MUSIC PLAYS VIA IRQ
```

### Pattern 4: Enhanced Input

**Use:** I/O vector (IGET)

```basic
10 REM INSTALL JOYSTICK-AS-KEYBOARD
20 POKE 804,0: POKE 805,192
30 INPUT "ENTER NAME (USE JOYSTICK)"; N$
```

---

## Loading ML Programs

### Method A: DATA Statements (Short Programs)

```basic
10 FOR I=0 TO 20
20 READ A: POKE 49152+I,A
30 NEXT I
40 DATA 169,1,141,32,208,96
```

**Pros:** Self-contained, easy to share
**Cons:** Slow, tedious for long programs

### Method B: LOAD from Disk (Long Programs)

```basic
10 LOAD "MLPROG.BIN",8,1
```

**Syntax:** LOAD "filename", device, secondary
- Device 8 = disk
- Secondary 1 = load to address stored in file (not $0801)

**Create Binary:**
Use assembler (ACME, ca65) or monitor to save ML program.

### Method C: Built-in Monitor (Immediate Testing)

Not a BASIC method, but useful for development:

**Enter monitor:** `SYS 1024` (unofficial) or use cartridge

**Assemble directly:** (machine code monitor)

---

## Error Handling

### Common Problems

| Symptom | Likely Cause | Fix |
|---------|--------------|-----|
| **System locks up** | No RTS, infinite loop | Reset, add RTS |
| **"ILLEGAL QUANTITY"** | Address out of range | Check SYS address |
| **Garbage on screen** | Overwrote screen memory | Use $C000, not $0400 |
| **Cursor stops blinking** | IRQ broke keyboard scan | Fix IRQ handler |
| **BASIC stops working** | CHRGET wedge corrupted | Don't use wedges (yet) |

### Safe Testing Procedure

1. **Save your BASIC program first** (`SAVE "TEST",8`)
2. Test ML routine with SYS
3. If crash, power cycle and reload
4. Debug using smaller test cases

---

## Quick Reference: Integration Methods

| Method | Vector | Complexity | When to Use | Lesson Level |
|--------|--------|------------|-------------|--------------|
| **SYS** | None | Low | Execute ML routine | Beginner (1-10) |
| **USR** | $0311-$0312 | Medium | Return calculated value | Intermediate (11-15) |
| **I/O Vectors** | $0324-$032F | Medium | Custom input/output | Advanced (16+) |
| **IRQ** | $0314-$0315 | High | Background tasks | Advanced (16+) |
| **CHRGET** | $0073-$0075 | Very High | BASIC extensions | Expert (21+) or omit |

---

## Assembly Routine Checklist

For ML routines called from BASIC:

- [ ] **Ends with RTS** (critical!)
- [ ] Uses safe memory locations ($C000-$CFFF)
- [ ] Doesn't overwrite stack ($0100-$01FF)
- [ ] Doesn't corrupt zero page (except $FB-$FE)
- [ ] If IRQ: preserves registers, acknowledges interrupt
- [ ] If I/O vector: chains to original or implements fully
- [ ] Tested with simple case before integration

---

## Example: Complete Mini-Program

### BASIC Loader + ML Routine

```basic
1000 REM ================================
1010 REM  BORDER FLASH PROGRAM
1020 REM  ML ROUTINE AT 49152 ($C000)
1030 REM ================================
1040 REM
1050 REM LOAD ML ROUTINE
1060 FOR I=0 TO 25
1070 READ A: POKE 49152+I,A
1080 NEXT I
1090 REM
1100 REM MAIN LOOP
1110 FOR C=0 TO 15
1120 POKE 251,C: REM PASS COLOR TO ML
1130 SYS 49152
1140 FOR D=1 TO 100: NEXT D: REM DELAY
1150 NEXT C
1160 END
1170 REM
1180 REM ML ROUTINE DATA
1190 DATA 169,0,141,32,208: REM LDA #0 / STA $D020
1200 DATA 165,251: REM LDA $FB (get parameter)
1210 DATA 141,32,208: REM STA $D020 (set border)
1220 DATA 96: REM RTS
```

**Assembly Source:**
```assembly
; Border color routine
; Parameter in $FB (color 0-15)

        *=$C000

START:
        LDA #0          ; Clear border first
        STA $D020

        LDA $FB         ; Get color parameter
        STA $D020       ; Set border color

        RTS             ; Return to BASIC
```

---

## For Lesson Creation

### Progression Framework

**Tier 1: Discovery (Lessons 1-10)**
- SYS only
- Simple visual effects
- Border/background colors
- Screen pokes

**Tier 2: Capability (Lessons 11-15)**
- USR functions
- Parameter passing
- Return values
- Simple calculations

**Tier 3: Mastery (Lessons 16-20)**
- IRQ handlers
- I/O vectors
- Sprites via IRQ
- Music players

**Tier 4: Artistry (Lessons 21+)**
- Complex IRQ effects
- Raster splits
- CHRGEN wedges (optional)
- Full ML programs (no BASIC)

### Teaching Tips

1. **Always show assembly source AND hex DATA** - students learn both
2. **Explain RTS importance early** - #1 mistake is forgetting it
3. **Use memory locations consistently** - $FB-$FE for parameters
4. **Start with visible effects** - border color, screen characters
5. **Delay advanced topics** - IRQ after 15+ lessons
6. **Test everything** - crashes are discouraging for beginners

---

## See Also

- **C64-MACHINE-LANGUAGE-OVERVIEW.md** - ML basics and 6510 instructions
- **KERNAL-ROUTINES-REFERENCE.md** - KERNAL call reference
- **C64-MEMORY-MAP.md** - Memory layout and safe locations
- **PETSCII-SCREEN-CODES.md** - Character codes for screen output

---

**Document Version:** 1.0
**Source Material:** Commodore 64 Programmer's Reference Guide (1982)
**Synthesized:** 2025 for Code Like It's 198x curriculum
