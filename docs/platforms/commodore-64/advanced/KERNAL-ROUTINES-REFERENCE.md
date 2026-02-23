# C64 KERNAL Routines Reference

**Reference Source:** C64 Programmer's Reference Guide, Chapter 5: BASIC to Machine Language
**Purpose:** Quick reference for lesson creation and code verification
**Audience:** Curriculum designers and lesson verifiers

---

## Quick Lookup

All KERNAL routines use the jump table at **$FF81-$FFF3**. This design allows Commodore to update internal implementations without breaking compatibility.

### By Category

- **[File Operations](#file-operations)** - Open, close, set file parameters
- **[Character I/O](#character-io)** - Read/write characters
- **[Serial Bus](#serial-bus)** - Disk and device communication
- **[Screen & Keyboard](#screen--keyboard)** - Display and input
- **[Memory Operations](#memory-operations)** - Load, save, verify
- **[Time & Vectors](#time--vectors)** - Jiffy clock, interrupt vectors
- **[Utility](#utility)** - Miscellaneous functions

---

## File Operations

| Routine | Address | Purpose | Call Preparation |
|---------|---------|---------|------------------|
| **SETLFS** | $FFBA | Set logical file parameters | A=file#, X=device#, Y=secondary |
| **SETNAM** | $FFBD | Set filename | A=length, X/Y=address (lo/hi) |
| **OPEN** | $FFC0 | Open logical file | SETLFS, SETNAM first |
| **CLOSE** | $FFC3 | Close logical file | A=file number |
| **CHKIN** | $FFC6 | Set input channel | X=file number (must be open) |
| **CHKOUT** | $FFC9 | Set output channel | X=file number (must be open) |
| **CLRCHN** | $FFCC | Restore default I/O channels | None |

### Standard File I/O Pattern

```assembly
; 1. Set up logical file
LDA #1          ; Logical file number
LDX #8          ; Device number (8 = disk)
LDY #2          ; Secondary address
JSR SETLFS      ; $FFBA

; 2. Set filename
LDA #FNAMELEN   ; Length of filename
LDX #<FNAME     ; Address low byte
LDY #>FNAME     ; Address high byte
JSR SETNAM      ; $FFBD

; 3. Open file
JSR OPEN        ; $FFC0
BCS ERROR       ; Branch if error

; 4. Set as input
LDX #1          ; Logical file number
JSR CHKIN       ; $FFC6

; 5. Read data
JSR CHRIN       ; $FFCF
; Repeat as needed

; 6. Clean up
JSR CLRCHN      ; $FFCC
LDA #1
JSR CLOSE       ; $FFC3

ERROR:
; Handle error (code in A)
```

---

## Character I/O

| Routine | Address | Purpose | Communication |
|---------|---------|---------|---------------|
| **CHRIN** | $FFCF | Input character | Returns: A=character, Carry=error |
| **CHROUT** | $FFD2 | Output character | A=character to output |
| **GETIN** | $FFE4 | Get character from keyboard | Returns: A=character (0 if none) |

### Examples

```assembly
; Print "HELLO" to screen
LDA #'H
JSR CHROUT      ; $FFD2
LDA #'E
JSR CHROUT
LDA #'L
JSR CHROUT
LDA #'L
JSR CHROUT
LDA #'O
JSR CHROUT

; Wait for keypress
WAIT:
JSR GETIN       ; $FFE4
BEQ WAIT        ; Loop if no key pressed
; A now contains key code
```

---

## Serial Bus

Low-level routines for communicating with devices on the serial bus (disk drives, printers).

| Routine | Address | Purpose | Stack Use |
|---------|---------|---------|-----------|
| **LISTEN** | $FFB1 | Command device to listen | 7 bytes |
| **TALK** | $FFB4 | Command device to talk | 7 bytes |
| **UNLSN** | $FFAE | Send UNLISTEN | 7 bytes |
| **UNTLK** | $FFAB | Send UNTALK | 7 bytes |
| **ACPTR** | $FFA5 | Accept byte from serial bus | 11 bytes |
| **CIOUT** | $FFA8 | Output byte to serial bus | 13 bytes |
| **SECOND** | $FF93 | Send secondary address after LISTEN | 7 bytes |
| **TKSA** | $FF96 | Send secondary address after TALK | 7 bytes |

**Note:** These are low-level. Most programs should use the file I/O routines instead.

### Serial Bus Pattern (Low-Level)

```assembly
; Send byte to device 8
LDA #8
JSR LISTEN      ; $FFB1 - Command device 8 to listen
LDA #$6F        ; Secondary address $0F + $60
JSR SECOND      ; $FF93 - Send secondary address
LDA #'A         ; Data byte
JSR CIOUT       ; $FFA8 - Output byte
JSR UNLSN       ; $FFAE - Release bus
```

---

## Screen & Keyboard

| Routine | Address | Purpose | Notes |
|---------|---------|---------|-------|
| **PLOT** | $FFF0 | Read/set cursor position | Carry=0: set X/Y, Carry=1: read X/Y |
| **SCNKEY** | $FF9F | Scan keyboard | Updates keyboard buffer |
| **GETIN** | $FFE4 | Get character from keyboard | Non-blocking, returns 0 if no key |

### Cursor Control Example

```assembly
; Position cursor at row 10, column 5
CLC             ; Clear carry to SET position
LDX #10         ; Row
LDY #5          ; Column
JSR PLOT        ; $FFF0

; Read current cursor position
SEC             ; Set carry to READ position
JSR PLOT        ; $FFF0
; X now contains row, Y contains column
```

---

## Memory Operations

| Routine | Address | Purpose | Preparation |
|---------|---------|---------|-------------|
| **LOAD** | $FFD5 | Load file into memory | SETLFS, SETNAM, A=0/1 (verify/load) |
| **SAVE** | $FFD8 | Save memory to file | SETLFS, SETNAM, A=zero page pointer |

### Load Example

```assembly
; Load "SPRITES.BIN" into $C000
LDA #1          ; File number
LDX #8          ; Device (disk)
LDY #0          ; Secondary address (0=relocate)
JSR SETLFS      ; $FFBA

LDA #11         ; Filename length
LDX #<FNAME
LDY #>FNAME
JSR SETNAM      ; $FFBD

LDA #0          ; 0 = load (not verify)
LDX #$00        ; Load address low byte
LDY #$C0        ; Load address high byte
JSR LOAD        ; $FFD5
BCS ERROR       ; Branch if error

FNAME:
.BYTE "SPRITES.BIN"
```

### Save Example

```assembly
; Save $C000-$C7FF to "DATA.BIN"
LDA #1
LDX #8
LDY #0
JSR SETLFS      ; $FFBA

LDA #8
LDX #<FNAME
LDY #>FNAME
JSR SETNAM      ; $FFBD

LDA #<ZEROPAGE  ; Zero page pointer to start/end addresses
LDX #$00        ; Start address low = $C000
LDY #$C0        ; Start address high
JSR SAVE        ; $FFD8
BCS ERROR

ZEROPAGE = $FB  ; Use $FB-$FC for pointer
; At $FB: $00 $C0 (start = $C000)
; At $FD: $00 $C8 (end = $C800)

FNAME:
.BYTE "DATA.BIN"
```

---

## Time & Vectors

| Routine | Address | Purpose | Communication |
|---------|---------|---------|---------------|
| **RDTIM** | $FFDE | Read jiffy clock | Returns: A/X/Y = hi/mid/lo bytes |
| **SETTIM** | $FFDB | Set jiffy clock | A/X/Y = hi/mid/lo bytes to set |
| **UDTIM** | $FFEA | Update jiffy clock | Call every 1/60 second |
| **VECTOR** | $FF8D | Read/set I/O vectors | Carry=0: set from $0314-$0333, Carry=1: read to $0314-$0333 |

### Jiffy Clock

- **Frequency:** 60 Hz (NTSC) or 50 Hz (PAL)
- **Storage:** 3 bytes at $00A0-$00A2
- **Maximum:** 5,184,000 jiffies (~24 hours)
- **Reset:** Automatically resets to zero after maximum

```assembly
; Wait approximately 3 seconds (180 jiffies)
JSR RDTIM       ; $FFDE - Read current time
STX TARGET      ; Store middle byte
STX TARGET+1
CLC
LDA TARGET
ADC #180        ; Add 180 jiffies
STA TARGET
BCC WAITLOOP
INC TARGET+1

WAITLOOP:
JSR RDTIM       ; $FFDE
CPX TARGET+1    ; Check high byte
BNE WAITLOOP
CPX TARGET      ; Check middle byte
BNE WAITLOOP
; 3 seconds have passed
```

---

## Utility

| Routine | Address | Purpose | Notes |
|---------|---------|---------|-------|
| **IOBASE** | $FFF3 | Read I/O base address | Returns: X/Y = $DC00 (CIA #1) |
| **RAMTAS** | $FF87 | Initialize RAM | Clears $0002-$0101, $0200-$03FF |
| **RESTOR** | $FF8A | Restore I/O vectors | Resets to default values |
| **READST** | $FFB7 | Read I/O status word | Returns: A = status flags |
| **STOP** | $FFE1 | Check STOP key | Z=1 if pressed, Z=0 if not |
| **MEMTOP** | $FF99 | Read/set top of RAM | Carry=0: set from X/Y, Carry=1: read to X/Y |
| **MEMBOT** | $FF9C | Read/set bottom of RAM | Carry=0: set from X/Y, Carry=1: read to X/Y |

### Status Word (READST)

```assembly
JSR READST      ; $FFB7
AND #$40        ; Bit 6 = EOF
BNE EOF_FOUND
```

**Status Bits:**
- Bit 7: Device not present
- Bit 6: **EOF (End of File)**
- Bit 5: (Reserved)
- Bit 4: Verify error
- Bit 3: Read error
- Bit 2: Timeout write
- Bit 1: Timeout read
- Bit 0: (See specific device)

**Common Usage:** Check for EOF when reading files.

---

## Device Numbers

| Device | Number | Description |
|--------|--------|-------------|
| Keyboard | 0 | Default input device |
| Datasette | 1 | Cassette tape drive |
| RS-232 | 2 | Serial communications |
| Screen | 3 | Default output device |
| Printer | 4 | Serial bus printer (IEC) |
| Secondary Printer | 5 | Second printer (rare) |
| Disk Drive | 8 | **Most common** - First disk drive |
| Second Disk | 9 | Second disk drive (if present) |
| Network | 10-30 | Serial bus devices |
| Command | 31 | Command/data channel |

**Note:** Device 8 is the standard disk drive device number.

---

## Error Codes

KERNAL routines return errors via the **carry flag** and **accumulator**.

**Pattern:**
```assembly
JSR ROUTINE
BCS ERROR       ; Branch if carry set
; Success - carry clear
; ...
ERROR:
; A contains error code
```

### Standard Error Codes

| Code | Meaning |
|------|---------|
| 0 | Routine terminated by STOP key |
| 1 | Too many open files (max 10) |
| 2 | File already open |
| 3 | File not open |
| 4 | File not found |
| 5 | Device not present |
| 6 | Not an input file |
| 7 | Not an output file |
| 8 | Missing file name |
| 9 | Illegal device number |

---

## Register Usage Conventions

**Preserved by KERNAL:**
- Generally, KERNAL routines do NOT preserve A, X, Y
- **If you need register values after a call, save them first**

**Common Pattern:**
```assembly
; Save registers before KERNAL call
PHA             ; Save A
TXA
PHA             ; Save X
TYA
PHA             ; Save Y

JSR KERNAL_ROUTINE

; Restore registers
PLA
TAY             ; Restore Y
PLA
TAX             ; Restore X
PLA             ; Restore A
```

---

## Stack Requirements

Each KERNAL routine documents its stack usage. Ensure adequate stack space before calling.

**Example:** LISTEN uses 7 bytes of stack. If you call LISTEN from within a subroutine (JSR = 2 bytes) that's already nested 3 deep (6 bytes), total stack usage = 2 + 7 + 6 = 15 bytes.

**Stack Range:** $0100-$01FF (256 bytes total)

**Rule of Thumb:** Keep at least 32 bytes free for safe nesting.

---

## Common Pitfalls

1. **Forgetting SETLFS/SETNAM** - Always call before OPEN
2. **Not checking carry flag** - Errors silently ignored
3. **Leaving channels open** - Use CLRCHN after CHKIN/CHKOUT
4. **Not closing files** - Max 10 files open at once
5. **Ignoring READST** - EOF detection requires checking status
6. **Wrong device number** - Device 8 for disk, not 1 (cassette)
7. **Not preserving registers** - Save X/Y if needed after call

---

## For Lesson Creation

### Beginner-Friendly Routines

Start lessons with these (simple, immediate results):

1. **CHROUT** - Print single character
2. **GETIN** - Read keyboard (non-blocking)
3. **PLOT** - Position cursor
4. **RDTIM** - Read time for delays

### Intermediate Routines

Introduce after file concepts are understood:

1. **SETLFS/SETNAM/OPEN** - File setup
2. **CHKIN/CHKOUT** - Channel selection
3. **CHRIN** - Read from file
4. **CLOSE/CLRCHN** - Cleanup

### Advanced Topics

For later lessons:

1. **LOAD/SAVE** - Direct memory operations
2. **Serial bus** - LISTEN/TALK/ACPTR/CIOUT
3. **VECTOR** - Interrupt vector management
4. **Custom IRQ handlers** - Timing and effects

---

## Quick Reference: Complete KERNAL Jump Table

```
$FF81  CINT    Initialize screen editor
$FF84  IOINIT  Initialize I/O devices
$FF87  RAMTAS  Initialize RAM
$FF8A  RESTOR  Restore I/O vectors
$FF8D  VECTOR  Read/set I/O vectors
$FF90  SETMSG  Control KERNAL messages
$FF93  SECOND  Send secondary address after LISTEN
$FF96  TKSA    Send secondary address after TALK
$FF99  MEMTOP  Read/set top of RAM
$FF9C  MEMBOT  Read/set bottom of RAM
$FF9F  SCNKEY  Scan keyboard
$FFA2  SETTMO  Set timeout (IEEE bus)
$FFA5  ACPTR   Accept byte from serial bus
$FFA8  CIOUT   Output byte to serial bus
$FFAB  UNTLK   Send UNTALK
$FFAE  UNLSN   Send UNLISTEN
$FFB1  LISTEN  Command device to listen
$FFB4  TALK    Command device to talk
$FFB7  READST  Read I/O status word
$FFBA  SETLFS  Set logical file parameters
$FFBD  SETNAM  Set filename
$FFC0  OPEN    Open logical file
$FFC3  CLOSE   Close logical file
$FFC6  CHKIN   Set input channel
$FFC9  CHKOUT  Set output channel
$FFCC  CLRCHN  Restore default I/O
$FFCF  CHRIN   Input character
$FFD2  CHROUT  Output character
$FFD5  LOAD    Load file
$FFD8  SAVE    Save file
$FFDB  SETTIM  Set jiffy clock
$FFDE  RDTIM   Read jiffy clock
$FFEA  UDTIM   Update jiffy clock
$FFE1  STOP    Check STOP key
$FFE4  GETIN   Get character from keyboard
$FFE7  CLALL   Close all files
$FFED  SCREEN  Return screen size
$FFF0  PLOT    Read/set cursor position
$FFF3  IOBASE  Read I/O base address
```

---

## See Also

- **C64-MACHINE-LANGUAGE-OVERVIEW.md** - ML basics and 6510 architecture
- **C64-MEMORY-MAP.md** - Complete memory layout
- **BASIC-TO-ML-INTEGRATION.md** - Calling ML from BASIC

---

**Document Version:** 1.0
**Source Material:** Commodore 64 Programmer's Reference Guide (1982)
**Synthesized:** 2025 for Code Like It's 198x curriculum
