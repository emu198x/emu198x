# C64 Memory Map Reference

**Reference Source:** C64 Programmer's Reference Guide, Chapter 5: BASIC to Machine Language
**Purpose:** Memory layout reference for lesson planning and code placement
**Audience:** Curriculum designers creating C64 assembly language lessons

---

## Quick Reference

### Safe Locations for Machine Language Programs

| Address Range | Decimal | Size | Best For |
|---------------|---------|------|----------|
| **$C000-$CFFF** | 49152-53247 | 4K | **Recommended** - ML programs called from BASIC |
| **$CC00-$CCFF** | 52224-52479 | 256 bytes | Cassette buffer (safe if not using tape) |
| **$0334-$033B** | 820-827 | 8 bytes | Tiny routines, custom vectors |
| **$FB-$FE** | 251-254 | 4 bytes | Zero page pointers (user-safe) |

### Dangerous Locations to AVOID

| Address Range | Why to Avoid |
|---------------|--------------|
| **$00-$FA** | KERNAL and BASIC zero page (critical system variables) |
| **$0100-$01FF** | Stack (crashes if overwritten) |
| **$0200-$02FF** | BASIC input buffer, system variables |
| **$0800-$9FFF** | BASIC program and variable space |
| **$A000-$BFFF** | BASIC ROM (unless banked out) |
| **$D000-$DFFF** | I/O registers and character ROM (bank-switched) |
| **$E000-$FFFF** | KERNAL ROM |

---

## Complete Memory Map

### $0000-$00FF: Zero Page (256 bytes)

**CRITICAL:** Most zero page is used by BASIC and KERNAL. Corruption causes crashes.

| Address | Use | Notes |
|---------|-----|-------|
| $00-$01 | **Processor I/O port** | Memory banking control |
| $02 | Unused | (Reserved) |
| $03-$8F | BASIC and KERNAL work area | **DO NOT USE** |
| $90 | Keyboard buffer size | |
| $91 | STOP key flag | |
| $93 | Load/verify flag | |
| $94 | Serial output: deferred character | |
| $97 | X coordinate of last plot | |
| $98 | Y coordinate of last plot | |
| $99 | Default input device (0) | |
| $9A | Default output device (3) | |
| $A0-$A2 | **Jiffy clock** (3 bytes) | System timer (60Hz) |
| $BA | Current device number | |
| $C5 | Matrix coordinate of last key pressed | |
| $D6 | Cursor blink phase | |
| $FB-$FE | **User pointers** | **SAFE for ML programs** |
| $FF | Unused | |

**Key Insight:** $FB-$FE (4 bytes) are the ONLY safe zero page locations for user programs. Use these for pointer storage.

---

### $0100-$01FF: Stack (256 bytes)

**Purpose:** Return addresses, saved registers, subroutine nesting

**Structure:** Descending stack
- Stack Pointer ($0100 + SP register value)
- Starts at $01FF and grows downward
- **JSR** pushes 2 bytes (return address)
- **PHA** pushes 1 byte

**NEVER write code or data here.** System will crash.

---

### $0200-$02FF: BASIC Input Buffer and System Variables

| Address | Use | Size |
|---------|-----|------|
| $0200-$0258 | BASIC input buffer | 89 bytes |
| $0259-$0262 | Logical file table | 10 bytes |
| $0263-$026C | Device number table | 10 bytes |
| $026D-$0276 | Secondary address table | 10 bytes |
| $0277-$0280 | Keyboard buffer | 10 bytes |
| $0281-$0282 | Start of BASIC program pointer | 2 bytes |
| $0283-$0284 | Start of variables pointer | 2 bytes |
| $0285-$0286 | Start of arrays pointer | 2 bytes |
| $0287-$0288 | End of arrays pointer | 2 bytes |
| $0289-$028A | String storage pointer | 2 bytes |
| $028B-$028C | Current variable pointer | 2 bytes |
| $028D-$028E | Current line pointer | 2 bytes |

**Avoid this area.** Contains critical BASIC state.

---

### $0300-$03FF: System and Cassette Buffer

| Address | Use | Notes |
|---------|-----|-------|
| $0300-$0313 | System error message vectors | |
| $0314-$0315 | IRQ vector (low/high) | **Can customize for effects** |
| $0316-$0317 | BRK vector | |
| $0318-$0319 | NMI vector | |
| $031A-$031F | I/O vectors | KERNAL device handlers |
| $0320-$0332 | More I/O vectors | |
| $0334-$033B | **User space** | **8 bytes - Safe for ML** |
| $033C-$03FF | Cassette buffer | **192 bytes - Safe if no tape** |

**Cassette Buffer ($033C-$03FF):** If you're not using cassette tape, this 192-byte area is available for ML code or data. Common for very short routines.

---

### $0400-$07FF: Screen Memory (1024 bytes)

**Purpose:** Default text screen (40 columns × 25 rows = 1000 bytes)

**Structure:**
- Each byte = screen code for one character
- Row 0: $0400-$0427 (40 bytes)
- Row 1: $0428-$044F (40 bytes)
- Row 24: $07C0-$07E7 (40 bytes)

**Formula:** Address = $0400 + (row × 40) + column

**Not PETSCII:** Screen codes differ from PETSCII codes. See PETSCII-SCREEN-CODES.md.

```assembly
; Write 'A' (screen code 1) to top-left corner
LDA #1
STA $0400
```

---

### $0800-$9FFF: BASIC Program Area (38K)

**Default:** $0801-$9FFF available for BASIC programs

**Reducing for ML:**
```basic
POKE 51,LO: POKE 52,HI: POKE 55,LO: POKE 56,HI: CLR
```

**Example:** Reserve $9000-$9FFF for ML (4K):
```basic
10 POKE 51,0: POKE 52,144: POKE 55,0: POKE 56,144: CLR
```

**Note:** CLR is required - resets BASIC variable pointers.

---

### $A000-$BFFF: BASIC ROM (8K)

**Default:** Contains Commodore BASIC V2 interpreter

**Bank Switching:** Can be switched out to access 8K of RAM underneath
- Controlled via $0001 (processor port)
- Allows 8K more program space if BASIC not needed
- **Not recommended for lessons** - adds complexity

---

### $C000-$CFFF: RAM (4K) - **BEST FOR ML PROGRAMS**

**Why this location:**
- Not used by BASIC
- Doesn't conflict with screen or I/O
- Easy to remember: $C000 decimal = 49152
- Standard starting point for `SYS 49152`

**Standard Usage:**
```basic
10 REM LOAD ML PROGRAM
20 FOR I=0 TO 255
30 READ A: POKE 49152+I,A
40 NEXT I
50 SYS 49152
60 DATA 169,1,141,32,208,96
```

---

### $D000-$DFFF: I/O and Character ROM (4K)

**Bank-Switched:** Can show I/O registers OR character ROM depending on $0001 settings.

**Default (I/O visible):**

#### VIC-II Registers ($D000-$D02E)

| Address | Register | Purpose |
|---------|----------|---------|
| **$D000-$D00F** | Sprite X/Y positions | 8 sprites × 2 bytes each |
| **$D010** | Sprite X MSB | High bit for X coordinates > 255 |
| **$D015** | Sprite enable | Bit N = sprite N on/off |
| **$D016** | Screen control | Multicolor mode, X scroll |
| **$D017** | Sprite Y expansion | Double height |
| **$D018** | **Memory control** | Screen/character memory pointers |
| **$D019** | IRQ flags | Raster, sprite collision |
| **$D01A** | IRQ mask | Enable/disable IRQ sources |
| **$D01B** | Sprite priority | Foreground/background |
| **$D01C** | Sprite multicolor mode | |
| **$D01D** | Sprite X expansion | Double width |
| **$D020** | **Border color** | 0-15 |
| **$D021** | **Background color** | 0-15 |
| **$D022-$D023** | Background colors 1-2 | Multicolor mode |
| **$D024** | Background color 3 | Multicolor mode |
| **$D025-$D026** | Sprite multicolor | Shared colors |
| **$D027-$D02E** | Sprite individual colors | 8 bytes |

**Most Common for Lessons:**
```assembly
LDA #0
STA $D020       ; Black border
LDA #6
STA $D021       ; Blue background
```

#### SID Registers ($D400-$D7FF)

| Address | Register | Purpose |
|---------|----------|---------|
| **$D400-$D406** | Voice 1 | Frequency, pulse, control, attack/decay, sustain/release |
| **$D407-$D40D** | Voice 2 | (Same structure) |
| **$D40E-$D414** | Voice 3 | (Same structure) |
| **$D415-$D416** | Filter cutoff | |
| **$D417** | Filter resonance/routing | |
| **$D418** | **Volume + filter mode** | Bits 0-3 = volume (0-15) |
| **$D419-$D41A** | Paddle inputs | |
| **$D41B** | Voice 3 waveform output | Random number generation |
| **$D41C** | Voice 3 envelope output | |

**Simple Sound Example:**
```assembly
LDA #15
STA $D418       ; Full volume

LDA #50
STA $D400       ; Frequency low
LDA #20
STA $D401       ; Frequency high

LDA #17
STA $D404       ; Triangle wave, gate on
```

#### CIA #1 Registers ($DC00-$DCFF)

**Purpose:** Keyboard, joystick, paddles

| Address | Register | Purpose |
|---------|----------|---------|
| **$DC00** | Data Port A | Keyboard column, joystick 2 |
| **$DC01** | Data Port B | Keyboard row, joystick 1 |
| **$DC02** | Data direction A | |
| **$DC03** | Data direction B | |
| **$DC04-$DC05** | Timer A | 16-bit countdown timer |
| **$DC06-$DC07** | Timer B | 16-bit countdown timer |
| **$DC08-$DC09** | Time of day clock | |
| **$DC0D** | Interrupt control | |

**Joystick Port 2 (most common):**
```assembly
LDA $DC00
AND #$10        ; Bit 4 = fire button
BEQ FIRE_PRESSED
```

**Joystick Bits (both ports):**
- Bit 0: Up
- Bit 1: Down
- Bit 2: Left
- Bit 3: Right
- Bit 4: Fire

#### CIA #2 Registers ($DD00-$DDFF)

**Purpose:** Serial bus, RS-232, memory banking, NMI

| Address | Register | Purpose |
|---------|----------|---------|
| **$DD00** | Data Port A | Serial bus, VIC memory bank |
| **$DD01** | Data Port B | RS-232 |
| **$DD02** | Data direction A | |
| **$DD03** | Data direction B | |
| **$DD0D** | NMI control | |

**VIC Bank Selection:** Bits 0-1 of $DD00 select which 16K bank VIC-II sees:
- %11 (3): $0000-$3FFF (Bank 0)
- %10 (2): $4000-$7FFF (Bank 1)
- %01 (1): $8000-$BFFF (Bank 2)
- %00 (0): $C000-$FFFF (Bank 3)

---

### $D800-$DBFF: Color RAM (1024 bytes)

**Purpose:** Color for each screen position (low nybble only)

**Structure:** Parallel to screen memory ($0400-$07FF)
- Each byte = color for corresponding screen position
- Only bits 0-3 used (colors 0-15)
- Location = $D800 + (row × 40) + column

**Not Affected by VIC Banking:** Always at $D800.

```assembly
; Make top-left character white (color 1)
LDA #1
STA $D800
```

**Colors:**
```
0  Black       4  Purple      8  Orange      12 Medium Gray
1  White       5  Green       9  Brown       13 Light Green
2  Red         6  Blue       10  Light Red   14 Light Blue
3  Cyan        7  Yellow     11  Dark Gray   15 Light Gray
```

---

### $E000-$FFFF: KERNAL ROM (8K)

**Purpose:** Operating system routines

**$FF81-$FFF3:** Jump table (fixed entry points)

**Do not write here.** ROM cannot be modified. Can be banked out for 8K RAM access, but **not recommended for lessons** - KERNAL calls will fail.

---

## Memory Banking ($0001)

The C64's $0000-$0001 control memory configuration:

| Bits 0-2 | Configuration |
|----------|---------------|
| %111 (7) | All RAM |
| %110 (6) | RAM with I/O ($D000-$DFFF visible) |
| %101 (5) | RAM with I/O and BASIC ROM |
| %100 (4) | All RAM |
| %011 (3) | **Default** - All ROMs, I/O visible |
| %010 (2) | KERNAL and I/O visible, BASIC banked out |
| %001 (1) | Character ROM visible at $D000, KERNAL visible |
| %000 (0) | All RAM visible |

**For Lessons:** Stay with default (%011). Explaining banking adds complexity.

---

## Safe Memory Recommendations by Lesson Level

### Beginner Lessons

**Use only:**
- $C000-$CFFF for ML code
- $FB-$FE for pointers
- Screen memory ($0400-$07FF) for output
- Color RAM ($D800-$DBFF) for colors
- Simple VIC registers ($D020, $D021)

### Intermediate Lessons

**Add:**
- $0334-$033B (8 bytes for tiny routines)
- $CC00-$CCFF (cassette buffer, 256 bytes)
- Sprite registers ($D000-$D02E)
- SID registers ($D400-$D7FF)

### Advanced Lessons

**Add:**
- Custom IRQ vectors ($0314-$0315)
- Reducing BASIC memory (POKE 51/52/55/56)
- Larger programs up to $CFFF or beyond
- Timer interrupts (CIA)

---

## Common Memory Layout for ML Programs

### Typical Structure

```
$C000-$C00F:  Main routine
$C010-$C0FF:  Subroutines
$C100-$C1FF:  Data tables
$C200-$C7FF:  Additional code/data
```

### Example: ML Game

```
$C000:        Initialization
$C020:        Game loop
$C100:        Sprite data
$C200:        Character set data
$C300:        Sound effect tables
```

---

## Quick Reference: Where to Put Things

| What | Where | Why |
|------|-------|-----|
| ML program (small) | $C000-$CFFF | Standard, BASIC-safe |
| ML program (large) | Reserve via POKE 51/52 | More space |
| Zero page pointers | $FB-$FE | Only safe user ZP |
| Tiny ML routines | $0334-$033B or $CC00-$CCFF | 8 or 256 bytes |
| Sprite data | $C000+ or VIC bank | 64 bytes per sprite |
| Character sets | $2000, $2800, $3000, $3800 | VIC-accessible |
| Screen memory | $0400 (default) or custom | VIC-accessible |

---

## For Lesson Creation

### Lesson Progression

1. **Lesson 1-5:** Only $C000-$CFFF, avoid memory details
2. **Lesson 6-10:** Introduce screen memory ($0400), color RAM ($D800)
3. **Lesson 11-15:** Zero page pointers ($FB-$FE)
4. **Lesson 16-20:** VIC/SID registers
5. **Lesson 21+:** Advanced memory management

### Example Explanations by Level

**Early:**
"We'll put our machine language program at 49152 (which is $C000 in hexadecimal). This is a safe location that doesn't interfere with BASIC."

**Intermediate:**
"The screen is located at $0400-$07FF. Each byte represents one character position. There are 25 rows of 40 characters each."

**Advanced:**
"We can reduce the memory available to BASIC and reserve $8000-$9FFF for our program. This gives us 8K instead of 4K."

---

## Memory Map Diagram

```
$FFFF ┌──────────────────┐
      │   KERNAL ROM     │  8K - Operating system
$E000 ├──────────────────┤
      │   (Free RAM)     │  4K - Can bank out KERNAL
$D000 ├──────────────────┤
      │  I/O + Color RAM │  4K - Hardware registers
$C000 ├──────────────────┤
      │   Free RAM ★     │  4K - BEST for ML programs
$B000 ├──────────────────┤
      │   (BASIC ROM)    │  8K - Can bank to RAM
$A000 ├──────────────────┤
      │  BASIC Program   │ 38K - Default BASIC area
$0800 ├──────────────────┤
      │  Screen Memory   │  1K - Text display
$0400 ├──────────────────┤
      │  Cassette Buffer │ 256 bytes (safe if no tape)
$0300 ├──────────────────┤
      │  System Buffers  │ 256 bytes (avoid)
$0200 ├──────────────────┤
      │     Stack        │ 256 bytes (NEVER overwrite)
$0100 ├──────────────────┤
      │   Zero Page      │ 256 bytes (only $FB-$FE safe)
$0000 └──────────────────┘

★ = Recommended for lessons
```

---

## See Also

- **C64-MACHINE-LANGUAGE-OVERVIEW.md** - ML basics
- **KERNAL-ROUTINES-REFERENCE.md** - KERNAL calls
- **BASIC-TO-ML-INTEGRATION.md** - Calling ML from BASIC
- **PETSCII-SCREEN-CODES.md** - Screen code reference

---

**Document Version:** 1.0
**Source Material:** Commodore 64 Programmer's Reference Guide (1982)
**Synthesized:** 2025 for Code Like It's 198x curriculum
