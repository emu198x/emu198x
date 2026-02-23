# NES Controller Input Quick Reference

**Purpose:** Fast lookup for reading NES controllers
**Audience:** NES assembly programmers and curriculum designers
**For comprehensive details:** See NESDev Wiki Controller Reading documentation

---

## Controller Hardware

**Standard NES Controller:**
- 8 buttons: A, B, Select, Start, Up, Down, Left, Right
- Serial shift register (reads 1 bit at a time)
- Connected to $4016 (controller 1) or $4017 (controller 2)

**Other Supported Devices:**
- Zapper (light gun)
- Power Pad
- Four Score (4-player adapter)
- Arkanoid controller
(Advanced - not covered in Tier 1)

---

## Register Summary

| Address | Name | Access | Purpose |
|---------|------|--------|---------|
| $4016 | JOY1 | R/W | Controller 1 data and strobe |
| $4017 | JOY2 | Read | Controller 2 data |

**Strobe:** Write $01 then $00 to $4016 to latch current button states
**Read:** Each read of $4016/$4017 returns next button bit in bit 0

---

## Button Reading Sequence

### 1. Strobe Controllers

**Purpose:** Latch current button states into shift register

```asm
LDA #$01
STA $4016       ; Strobe on (prepare to latch)
LDA #$00
STA $4016       ; Strobe off (latch button states)
```

**Critical:** Must strobe before reading buttons. This captures the instantaneous state of all 8 buttons.

### 2. Read 8 Buttons (Serial)

**Order:** A, B, Select, Start, Up, Down, Left, Right

Each read returns:
- Bit 0: Button state (1=pressed, 0=not pressed)
- Bits 1-4: Open bus (undefined, ignore)
- Bits 5-7: Various (ignore for standard controller)

**Pattern:**
```asm
; Read controller 1
LDA $4016       ; Read A button (bit 0)
LDA $4016       ; Read B button (bit 0)
LDA $4016       ; Read Select button
LDA $4016       ; Read Start button
LDA $4016       ; Read Up button
LDA $4016       ; Read Down button
LDA $4016       ; Read Left button
LDA $4016       ; Read Right button
```

**Controller 2:** Same sequence, but read from $4017

---

## Complete Reading Pattern

### Method 1: Build Byte with All Button States

**Most Common - Stores all buttons in single byte**

```asm
.segment "ZEROPAGE"
buttons: .res 1         ; Controller 1 buttons
                        ; Bit 7=A, 6=B, 5=Sel, 4=Start
                        ; Bit 3=Up, 2=Down, 1=Left, 0=Right

ReadController:
    ; Strobe controller
    LDA #$01
    STA $4016
    LDA #$00
    STA $4016

    ; Read 8 buttons into buttons variable
    LDX #$08            ; 8 buttons to read
:   LDA $4016           ; Read one button
    LSR                 ; Shift bit 0 into carry flag
    ROL buttons         ; Rotate carry into buttons
    DEX
    BNE :-              ; Loop until X=0

    RTS
```

**Result:** All 8 buttons packed into one byte
- Bit 7: A button
- Bit 6: B button
- Bit 5: Select
- Bit 4: Start
- Bit 3: Up
- Bit 2: Down
- Bit 1: Left
- Bit 0: Right

**Testing Buttons:**
```asm
; Test A button (bit 7)
LDA buttons
AND #%10000000
BEQ a_not_pressed
; A button is pressed
a_not_pressed:

; Test Up button (bit 3)
LDA buttons
AND #%00001000
BEQ up_not_pressed
; Up is pressed
up_not_pressed:
```

### Method 2: Test Buttons Individually

**Simple but Less Efficient**

```asm
ReadAndProcessController:
    ; Strobe
    LDA #$01
    STA $4016
    LDA #$00
    STA $4016

    ; Read and test A button
    LDA $4016
    AND #$01            ; Isolate bit 0
    BEQ :+              ; Skip if not pressed
    ; A button pressed - do something
:

    ; Read and test B button
    LDA $4016
    AND #$01
    BEQ :+
    ; B button pressed
:

    ; Skip Select and Start
    LDA $4016           ; Select
    LDA $4016           ; Start

    ; Read and test Up
    LDA $4016
    AND #$01
    BEQ :+
    ; Up pressed - move paddle up
    DEC paddle_y
:

    ; Read and test Down
    LDA $4016
    AND #$01
    BEQ :+
    ; Down pressed - move paddle down
    INC paddle_y
:

    ; Read Left and Right
    LDA $4016           ; Left (ignore for Pong)
    LDA $4016           ; Right (ignore for Pong)

    RTS
```

---

## Two-Player Input

### Reading Both Controllers

```asm
.segment "ZEROPAGE"
buttons1: .res 1        ; Controller 1
buttons2: .res 1        ; Controller 2

ReadControllers:
    ; Strobe both controllers
    LDA #$01
    STA $4016           ; Strobe on
    LDA #$00
    STA $4016           ; Strobe off (latches both)

    ; Read controller 1 (8 buttons)
    LDX #$08
:   LDA $4016
    LSR
    ROL buttons1
    DEX
    BNE :-

    ; Read controller 2 (8 buttons)
    LDX #$08
:   LDA $4017           ; Read from $4017 for controller 2
    LSR
    ROL buttons2
    DEX
    BNE :-

    RTS
```

**Usage in Game Loop:**
```asm
MainLoop:
    JSR ReadControllers

    ; Player 1 input
    LDA buttons1
    AND #%00001000      ; Up button
    BEQ :+
    DEC paddle1_y
:   LDA buttons1
    AND #%00000100      ; Down button
    BEQ :+
    INC paddle1_y
:

    ; Player 2 input
    LDA buttons2
    AND #%00001000      ; Up button
    BEQ :+
    DEC paddle2_y
:   LDA buttons2
    AND #%00000100      ; Down button
    BEQ :+
    INC paddle2_y
:

    ; Continue game logic
    JMP MainLoop
```

---

## Button Bit Masks

### Quick Reference Table

| Button | Bit Position | Bit Mask | Hex Mask |
|--------|-------------|----------|----------|
| A | 7 | %10000000 | $80 |
| B | 6 | %01000000 | $40 |
| Select | 5 | %00100000 | $20 |
| Start | 4 | %00010000 | $10 |
| Up | 3 | %00001000 | $08 |
| Down | 2 | %00000100 | $04 |
| Left | 1 | %00000010 | $02 |
| Right | 0 | %00000001 | $01 |

### Testing Multiple Buttons

```asm
; Test A OR B (either pressed)
LDA buttons
AND #%11000000      ; Mask A and B
BEQ no_action       ; Neither pressed
; At least one pressed

; Test Up AND A (both pressed simultaneously)
LDA buttons
AND #%10001000      ; Mask A and Up
CMP #%10001000      ; Check if both set
BNE not_both
; Both A and Up pressed

; Test any D-pad direction
LDA buttons
AND #%00001111      ; Mask all D-pad buttons
BEQ no_movement     ; No direction pressed
; At least one direction pressed
```

---

## Advanced Techniques

### Detecting Button Press (Edge Detection)

**Problem:** AND test detects "held" state. Often want to detect "just pressed" (transition from not pressed → pressed).

**Solution:** Store previous frame's button state

```asm
.segment "ZEROPAGE"
buttons:      .res 1   ; Current frame buttons
buttons_prev: .res 1   ; Previous frame buttons
buttons_new:  .res 1   ; Newly pressed this frame

ReadControllerEdge:
    ; Save previous state
    LDA buttons
    STA buttons_prev

    ; Read current state
    JSR ReadController

    ; Calculate newly pressed buttons
    ; New = Current AND NOT(Previous)
    LDA buttons_prev
    EOR #$FF            ; Invert previous
    AND buttons         ; AND with current
    STA buttons_new     ; Buttons pressed THIS frame
    
    RTS

; Usage:
; Test if A button was JUST pressed (not held)
LDA buttons_new
AND #%10000000
BEQ a_not_newly_pressed
; A was just pressed this frame
```

### Debouncing (If Needed)

**Usually not needed** - NES controller hardware is reliable. But for completeness:

```asm
ReadControllerDebounced:
    JSR ReadController
    STA temp1

    ; Read again
    JSR ReadController
    AND temp1           ; Both reads must agree
    STA buttons
    RTS
```

---

## Common Patterns

### Pong Paddle Control (Tier 1)

```asm
MainLoop:
    JSR ReadControllers

    ; Player 1 paddle (Up/Down)
    LDA buttons1
    AND #%00001000      ; Up
    BEQ :+
    LDA paddle1_y
    SEC
    SBC #3              ; Move up 3 pixels
    STA paddle1_y
:   LDA buttons1
    AND #%00000100      ; Down
    BEQ :+
    LDA paddle1_y
    CLC
    ADC #3              ; Move down 3 pixels
    STA paddle1_y
:

    ; Player 2 paddle
    LDA buttons2
    AND #%00001000
    BEQ :+
    LDA paddle2_y
    SEC
    SBC #3
    STA paddle2_y
:   LDA buttons2
    AND #%00000100
    BEQ :+
    LDA paddle2_y
    CLC
    ADC #3
    STA paddle2_y
:

    ; Apply boundaries, update sprites, etc.
    JMP MainLoop
```

### Menu Navigation

```asm
MenuLoop:
    JSR ReadController

    ; Up - previous menu item
    LDA buttons
    AND #%00001000
    BEQ :+
    DEC menu_cursor
    ; Clamp to 0
    BPL :+
    LDA #0
    STA menu_cursor
:

    ; Down - next menu item
    LDA buttons
    AND #%00000100
    BEQ :+
    INC menu_cursor
    ; Clamp to max
    LDA menu_cursor
    CMP #MENU_MAX
    BCC :+
    LDA #MENU_MAX-1
    STA menu_cursor
:

    ; A button - select
    LDA buttons
    AND #%10000000
    BEQ :+
    JMP ExecuteMenuItem
:

    JMP MenuLoop
```

### 8-Way Movement

```asm
UpdatePlayer:
    JSR ReadController

    ; Horizontal movement
    LDA buttons
    AND #%00000010      ; Left
    BEQ :+
    DEC player_x
:   LDA buttons
    AND #%00000001      ; Right
    BEQ :+
    INC player_x
:

    ; Vertical movement
    LDA buttons
    AND #%00001000      ; Up
    BEQ :+
    DEC player_y
:   LDA buttons
    AND #%00000100      ; Down
    BEQ :+
    INC player_y
:

    RTS
```

---

## Timing Considerations

### Reading Frequency

**Standard:** Read once per frame (60 Hz)
- In main loop, before game logic
- Consistent timing for all players

**Don't Read in NMI:**
- NMI is for graphics updates
- Keep controller reading in main loop
- Avoids race conditions

**Example:**
```asm
MainLoop:
    ; Wait for NMI
:   LDA nmi_ready
    BEQ :-
    LDA #$00
    STA nmi_ready

    ; Read input (game logic phase)
    JSR ReadControllers

    ; Update game state
    JSR UpdatePlayer
    JSR UpdateBall
    JSR CheckCollisions

    JMP MainLoop

NMI:
    ; Graphics updates only
    PHA
    LDA #$02
    STA $4014           ; OAM DMA
    LDA #$01
    STA nmi_ready
    PLA
    RTI
```

### Strobe Duration

**Critical:** Must write $01 then $00 to $4016
- Don't skip the $00 write
- Don't strobe mid-read

**Wrong:**
```asm
; BAD - missing $00 write
LDA #$01
STA $4016
; Missing LDA #$00 / STA $4016
LDA $4016           ; Unreliable!
```

**Correct:**
```asm
LDA #$01
STA $4016
LDA #$00            ; Must write $00
STA $4016
LDA $4016           ; Now reliable
```

---

## Troubleshooting

### Buttons Read Incorrectly

**Symptoms:**
- Wrong button triggers action
- Random button presses
- No input detected

**Checks:**
1. Did you strobe ($01 then $00 to $4016)?
2. Are you reading 8 times (all buttons)?
3. Are you using correct bit masks?
4. Is controller physically connected?

### Diagonal Movement Issues

**Problem:** Player moves diagonally when pressing Up+Right

**Solution:** Decide on priority or allow diagonals

```asm
; Option 1: Horizontal priority (disable diagonal)
LDA buttons
AND #%00000011      ; Left or Right?
BNE horizontal      ; If yes, skip vertical
; Process Up/Down only if no horizontal input

; Option 2: Allow diagonals (process both)
LDA buttons
AND #%00000010      ; Left
BEQ :+
DEC player_x
:   LDA buttons
AND #%00000001      ; Right
BEQ :+
INC player_x
:   LDA buttons
AND #%00001000      ; Up
BEQ :+
DEC player_y
:
; Both horizontal and vertical processed
```

### Controller 2 Not Working

**Check:**
- Are you reading from $4017 (not $4016)?
- Did you strobe $4016 (strobes both controllers)?
- Is controller 2 physically connected?

---

## Quick Reference

### Complete Two-Player Reading Function

```asm
.segment "ZEROPAGE"
buttons1: .res 1
buttons2: .res 1

ReadBothControllers:
    ; Strobe both controllers
    LDA #$01
    STA $4016
    LDA #$00
    STA $4016

    ; Read controller 1
    LDX #$08
:   LDA $4016
    LSR
    ROL buttons1
    DEX
    BNE :-

    ; Read controller 2
    LDX #$08
:   LDA $4017
    LSR
    ROL buttons2
    DEX
    BNE :-

    RTS
```

### Button Mask Constants (ca65)

```asm
BUTTON_A      = %10000000
BUTTON_B      = %01000000
BUTTON_SELECT = %00100000
BUTTON_START  = %00010000
BUTTON_UP     = %00001000
BUTTON_DOWN   = %00000100
BUTTON_LEFT   = %00000010
BUTTON_RIGHT  = %00000001

; Usage:
LDA buttons
AND #BUTTON_A
BEQ not_pressed
```

---

**Version:** 1.0
**Created:** 2025-10-24
**For:** NES Phase 1 Assembly Programming

**See Also:**
- 6502-QUICK-REFERENCE.md (CPU instructions)
- PPU-PROGRAMMING-QUICK-REFERENCE.md (display programming)
- NES-MEMORY-MAP.md (memory organization)

**Complete:** NES reference documentation set finished!
