# CIA Quick Reference - Programming Essentials

**Purpose:** Quick lookup for lesson creation - joystick, keyboard, timers
**For comprehensive hardware details:** See 6526-CIA-REFERENCE.md
**Audience:** Curriculum designers creating C64 assembly lessons

---

## What You Need to Know

The C64 has **two CIA chips** (Complex Interface Adapters):

- **CIA #1 ($DC00)**: Keyboard, joysticks, timers, IRQ interrupts
- **CIA #2 ($DD00)**: Serial bus, VIC bank selection, RS-232, NMI interrupts

**Most Common Uses in Lessons:**
1. Reading joysticks (CIA #1)
2. Scanning keyboard (CIA #1)
3. Simple timing delays (Timer A/B)
4. VIC bank switching (CIA #2)

---

## Register Map Quick Lookup

### CIA #1 Registers ($DC00-$DC0F)

| Address | Dec | Name | Use in Lessons |
|---------|-----|------|----------------|
| **$DC00** | 56320 | Port A Data | Keyboard rows, **Joystick 2** |
| **$DC01** | 56321 | Port B Data | Keyboard columns, **Joystick 1** |
| **$DC02** | 56322 | Direction A | Configure Port A (input/output) |
| **$DC03** | 56323 | Direction B | Configure Port B (input/output) |
| **$DC04** | 56324 | Timer A Low | Low byte of Timer A |
| **$DC05** | 56325 | Timer A High | High byte of Timer A |
| **$DC06** | 56326 | Timer B Low | Low byte of Timer B |
| **$DC07** | 56327 | Timer B High | High byte of Timer B |
| **$DC0D** | 56333 | Interrupt Control | Enable/read timer interrupts |
| **$DC0E** | 56334 | Control A | Timer A control |
| **$DC0F** | 56335 | Control B | Timer B control |

**Note:** TOD (Time of Day) registers $DC08-$DC0B exist but rarely used in lessons.

### CIA #2 Registers ($DD00-$DD0F)

| Address | Dec | Name | Use in Lessons |
|---------|-----|------|----------------|
| **$DD00** | 56576 | Port A Data | **VIC bank selection**, serial bus |
| **$DD01** | 56577 | Port B Data | User port, RS-232 (rarely used) |
| **$DD02** | 56578 | Direction A | Configure Port A |
| **$DD0D** | 56589 | Interrupt Control | NMI interrupts (RESTORE key) |

---

## Joystick Reading (Most Common)

### Joystick Port Mapping

| Port | CIA | Register | Typical Use |
|------|-----|----------|-------------|
| Port 1 | CIA #1 | $DC01 | **Most games use Port 2** |
| Port 2 | CIA #1 | $DC00 | **Standard gaming port** |

### Joystick Bit Layout

```
Bit:  7   6   5   4   3   2   1   0
      -   -   -  FIRE RIGHT LEFT DOWN UP

0 = Pressed
1 = Not pressed (default with pull-ups)
```

### Standard Joystick Reading Pattern

```assembly
; Read Joystick Port 2 (CIA #1 Port A)
LDA $DC00       ; Read joystick state
AND #$1F        ; Mask bits 0-4 (ignore keyboard bits)

; Check fire button
AND #$10        ; Bit 4 = fire
BEQ FIRE_PRESSED

; Check directions
LDA $DC00
AND #$01        ; Up
BEQ UP_PRESSED

LDA $DC00
AND #$02        ; Down
BEQ DOWN_PRESSED

LDA $DC00
AND #$04        ; Left
BEQ LEFT_PRESSED

LDA $DC00
AND #$08        ; Right
BEQ RIGHT_PRESSED
```

### Multi-Direction Check

```assembly
; Read Joystick 2 into A
LDA $DC00
EOR #$FF        ; Invert (now 1 = pressed)
AND #$1F        ; Mask bits 0-4

; Now you can check multiple directions:
; Bit 0 = Up
; Bit 1 = Down
; Bit 2 = Left
; Bit 3 = Right
; Bit 4 = Fire

; Example: Check if UP+FIRE pressed
AND #%00010001  ; UP (bit 0) + FIRE (bit 4)
CMP #%00010001
BEQ UP_AND_FIRE
```

---

## Keyboard Scanning

The C64 keyboard is an 8×8 matrix:
- **Rows** scanned via CIA #1 Port A ($DC00)
- **Columns** read via CIA #1 Port B ($DC01)

### Basic Keyboard Scan Pattern

```assembly
; Set Port A as outputs (drive rows)
LDA #$FF
STA $DC02       ; DDRA = all outputs

; Set Port B as inputs (read columns)
LDA #$00
STA $DC03       ; DDRB = all inputs

; Scan Row 0 (contains SPACE, RETURN, etc.)
LDA #$FE        ; Activate row 0 (bit 0 = 0, others = 1)
STA $DC00       ; Drive row 0 low

; Read columns
LDA $DC01       ; Read which keys are pressed
EOR #$FF        ; Invert (now 1 = pressed)
; Check specific column bits...
```

### Common Keys and Their Matrix Positions

**Row 7 ($7F in $DC00), Column bits in $DC01:**
```
Bit 0: STOP
Bit 1: Q
Bit 2: C=
Bit 3: SPACE
Bit 4: 2
Bit 5: CTRL
Bit 6: ←
Bit 7: 1
```

**Row 0 ($FE in $DC00):**
```
Bit 0: DEL
Bit 1: RETURN
Bit 2: →
Bit 3: F7
Bit 4: F1
Bit 5: F3
Bit 6: F5
Bit 7: ↓
```

### Simpler: Check Specific Key (SPACE Example)

```assembly
; Check if SPACE bar pressed
LDA #$7F        ; Select row 7
STA $DC00
LDA $DC01       ; Read columns
AND #$10        ; Bit 4 = SPACE
BEQ SPACE_PRESSED
```

### Using KERNAL Instead

**For beginner lessons, use KERNAL:**
```assembly
; Non-blocking key check
JSR $FFE4       ; GETIN
BEQ NO_KEY      ; A=0 means no key pressed
; A contains PETSCII code of key
```

**Recommendation:** Use KERNAL GETIN for early lessons, introduce matrix scanning in intermediate lessons.

---

## Timers - Simple Delays

Each CIA has two 16-bit countdown timers.

### Timer A Simple Delay Pattern

```assembly
; Delay approximately 1/60 second (one frame)
; System clock: 1.022727 MHz (NTSC)
; 1/60 sec = ~17045 cycles

LDA #<17045     ; Low byte of delay
STA $DC04       ; Timer A low
LDA #>17045     ; High byte
STA $DC05       ; Timer A high

LDA #%00010001  ; Start timer, one-shot mode
STA $DC0E       ; Control Register A

WAIT:
LDA $DC0E       ; Read control register
AND #%00000001  ; Check if timer running (bit 0)
BNE WAIT        ; Loop while running
; Timer finished
```

### Timer Control Register Bits (CRA = $DC0E, CRB = $DC0F)

```
Bit 0: START (1 = start timer, 0 = stop)
Bit 1: Timer output on PB6/PB7 (advanced)
Bit 2: Toggle/pulse mode (advanced)
Bit 3: One-shot (1) or continuous (0)
Bit 4: Force load (1 = load latch into counter)
Bit 5-6: Timer input mode
Bit 7: TOD clock selection (ignore for simple delays)
```

**Simple pattern:**
- `#%00010001` = One-shot, start immediately
- `#%00010011` = Continuous, start immediately

### Calculating Delay Values

**Formula:** Delay (cycles) = Frequency × Time

**NTSC C64:** 1.022727 MHz ≈ 1022727 Hz
**PAL C64:** 0.985248 MHz ≈ 985248 Hz

**Common delays:**
```
1/60 sec (1 frame NTSC): 17045 cycles
1/50 sec (1 frame PAL):  19705 cycles
1 millisecond:           ~1023 cycles (NTSC)
1 second:                1022727 cycles (won't fit in 16-bit: use loops)
```

**For lessons:** Use frame-based delays (17045) for simplicity.

---

## VIC Bank Switching (CIA #2)

The VIC-II chip can only see 16K at a time. CIA #2 Port A bits 0-1 select which 16K bank.

### VIC Bank Selection

**Bits 0-1 of $DD00 are INVERTED:**

| $DD00 bits 1-0 | VIC Bank | Address Range |
|----------------|----------|---------------|
| **%11** (3) | Bank 0 | $0000-$3FFF (default) |
| **%10** (2) | Bank 1 | $4000-$7FFF |
| **%01** (1) | Bank 2 | $8000-$BFFF |
| **%00** (0) | Bank 3 | $C000-$FFFF |

### Standard VIC Bank Switch Pattern

```assembly
; Switch to VIC Bank 2 ($8000-$BFFF)

; Step 1: Set bits 0-1 as outputs
LDA $DD02       ; Read current data direction
ORA #%00000011  ; Set bits 0-1 as outputs
STA $DD02       ; DDRA

; Step 2: Set bank (bits inverted!)
LDA $DD00       ; Read current port state
AND #%11111100  ; Clear bits 0-1
ORA #%00000001  ; Set to 01 = Bank 2
STA $DD00       ; Switch to bank 2
```

**Common mistake:** Forgetting bits are inverted (11 = bank 0, not bank 3).

---

## Timer Interrupts (Advanced)

Timers can generate IRQ interrupts when they underflow.

### Basic Timer Interrupt Setup

```assembly
; Generate IRQ every 1/60 second

SEI             ; Disable interrupts while setting up

; Set Timer A for ~17045 cycles
LDA #<17045
STA $DC04
LDA #>17045
STA $DC05

; Enable Timer A interrupt
LDA #%10000001  ; Bit 7 = write mode, bit 0 = Timer A
STA $DC0D       ; ICR - enable interrupt

; Start timer (continuous mode)
LDA #%00000001  ; Continuous, start timer
STA $DC0E       ; CRA

; Install custom IRQ handler
LDA #<IRQ_HANDLER
STA $0314
LDA #>IRQ_HANDLER
STA $0315

CLI             ; Re-enable interrupts

; IRQ handler:
IRQ_HANDLER:
    LDA $DC0D       ; Read ICR (clears interrupt)
    AND #%00000001  ; Timer A?
    BEQ NOT_TIMER_A

    ; Your code here (keep it fast!)
    INC $D020       ; Example: flash border

NOT_TIMER_A:
    JMP $EA31       ; Jump to KERNAL IRQ handler
```

**Important:** Always read $DC0D in IRQ handler to acknowledge interrupt!

---

## Common Patterns for Lessons

### Pattern 1: Single Joystick Game Control

```assembly
GAME_LOOP:
    ; Read joystick
    LDA $DC00
    AND #$1F        ; Mask joystick bits

    EOR #$1F        ; Invert (1 = pressed)
    BEQ NO_INPUT    ; No joystick movement

    ; Check directions
    LSR             ; Shift bit 0 (UP) into carry
    BCC CHECK_DOWN
    JSR MOVE_UP
CHECK_DOWN:
    LSR             ; Shift bit 1 (DOWN) into carry
    BCC CHECK_LEFT
    JSR MOVE_DOWN
CHECK_LEFT:
    LSR             ; Bit 2 (LEFT)
    BCC CHECK_RIGHT
    JSR MOVE_LEFT
CHECK_RIGHT:
    LSR             ; Bit 3 (RIGHT)
    BCC CHECK_FIRE
    JSR MOVE_RIGHT
CHECK_FIRE:
    LSR             ; Bit 4 (FIRE)
    BCC NO_INPUT
    JSR FIRE_ACTION

NO_INPUT:
    JMP GAME_LOOP
```

### Pattern 2: Wait for Fire Button

```assembly
WAIT_FIRE:
    LDA $DC00       ; Joystick 2
    AND #$10        ; Fire button
    BNE WAIT_FIRE   ; Loop while NOT pressed (bit = 1)
    ; Fire pressed - continue
```

### Pattern 3: Debounced Key Check

```assembly
; Wait for key press and release
WAIT_KEY:
    JSR $FFE4       ; GETIN
    BEQ WAIT_KEY    ; Loop until key pressed
    PHA             ; Save key code
WAIT_RELEASE:
    JSR $FFE4
    BNE WAIT_RELEASE ; Wait until released
    PLA             ; Restore key code
    ; A contains the key that was pressed
```

---

## Lesson Design Recommendations

### Beginner Lessons (1-10)

**Focus on:**
- Joystick reading (single direction checks)
- Simple key detection via KERNAL GETIN
- Avoid timers and interrupts

**Example:**
```assembly
; Lesson 5: Move sprite with joystick
LDA $DC00       ; Read joystick 2
AND #$01        ; Check UP
BEQ MOVE_UP     ; If pressed, move sprite
```

### Intermediate Lessons (11-15)

**Add:**
- Multi-direction joystick handling
- Keyboard matrix scanning (specific keys)
- Simple timer delays

**Example:**
```assembly
; Lesson 12: Check SPACE bar directly
LDA #$7F
STA $DC00
LDA $DC01
AND #$10
BEQ SPACE_PRESSED
```

### Advanced Lessons (16-20)

**Add:**
- Timer interrupts
- VIC bank switching
- Full keyboard scanning

**Example:**
```assembly
; Lesson 18: Music player with timer interrupt
; Set up Timer A for 1/60 sec interrupt
; IRQ handler plays next note
```

---

## Common Mistakes and Fixes

| Problem | Cause | Fix |
|---------|-------|-----|
| Joystick always reads as pressed | Forgot to mask unused bits | `AND #$1F` before checking |
| Wrong VIC bank selected | Forgot bits are inverted | 11=$0000, 10=$4000, 01=$8000, 00=$C000 |
| Timer doesn't stop | One-shot not set | Use bit 3 = 1 in control register |
| Keyboard reads wrong | Port directions not set | $DC02=$FF, $DC03=$00 |
| Timer interrupt floods | Didn't read $DC0D | Always `LDA $DC0D` in IRQ handler |
| Joystick and keyboard conflict | Both use CIA #1 | Disable keyboard scan when reading joystick |

---

## Quick Reference: Essential Registers

### Joystick
```assembly
$DC00           ; Joystick 2 (Port A)
$DC01           ; Joystick 1 (Port B)
```

### Timer A (most common)
```assembly
$DC04           ; Timer A Low Byte
$DC05           ; Timer A High Byte
$DC0E           ; Control Register A
$DC0D           ; Interrupt Control/Status
```

### VIC Bank
```assembly
$DD00           ; Port A (bits 0-1 select bank, INVERTED)
$DD02           ; Direction A (set bits 0-1 = output)
```

### Keyboard
```assembly
$DC00           ; Select row (output)
$DC01           ; Read column (input)
$DC02           ; Set as output ($FF)
$DC03           ; Set as input ($00)
```

---

## See Also

- **6526-CIA-REFERENCE.md** - Complete hardware specifications
- **C64-MEMORY-MAP.md** - Memory layout including CIA registers
- **KERNAL-ROUTINES-REFERENCE.md** - GETIN and other keyboard routines
- **VIC-II-QUICK-REFERENCE.md** - Screen and sprite programming

---

**Document Version:** 1.0
**Synthesized:** 2025 for Code Like It's 198x curriculum
**Focus:** Programming essentials only - no hardware/electrical details
