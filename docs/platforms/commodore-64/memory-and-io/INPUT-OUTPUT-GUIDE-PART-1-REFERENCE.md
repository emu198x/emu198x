# Input/Output Guide - Part 1 Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 6 (Part 1)

---

## Overview

The Commodore 64 supports multiple input/output devices for communication beyond the screen. This reference covers output formatting, device I/O, game controllers, and RS-232 serial communication.

**Key I/O capabilities:**
- TV/Screen output (PRINT)
- Printer output (PRINT#)
- Modem/RS-232 communication
- Cassette tape data storage
- Floppy disk storage
- Joystick and paddle input
- Light pen input

---

## Output to TV (Screen)

### PRINT Statement Formatting

**Basic usage:**
```basic
PRINT "HELLO"        : REM Simple text
PRINT A$, B$, C$     : REM Multiple items
PRINT "X=";X         : REM Mixed text and variables
```

**Formatting functions:**
- **TAB(n)** - Position cursor at column n from left edge
- **SPC(n)** - Move cursor right n spaces from current position

**Punctuation effects:**

| Symbol | Effect |
|--------|--------|
| `;` (semicolon) | No space between items; suppresses RETURN at end of line |
| `,` (comma) | Tab to next column (4 columns of 10 characters each) |
| `""` (quotes) | Enclose literal text |

### Control Characters

**Special character codes (CHR$):**
- CHR$(13) - RETURN (moves to next logical line)
- CHR$(17) - Cursor down / Switch to upper/lower case
- CHR$(145) - Cursor up / Switch to upper case/graphics
- See Appendix C for complete table

### Logical Lines

**Important concept:**
- Logical line can be 1 or 2 physical screen lines
- Lines are "linked" when typing past end of line
- RETURN goes to next logical line, not always next physical line
- Screen scrolls 1 or 2 lines depending on logical line at top

---

## OPEN Statement for Devices

### OPEN Syntax

```basic
OPEN file#, device#, number, "string"
```

### Device Numbers and Parameters

| Device | Device# | Number | String |
|--------|---------|--------|--------|
| **Cassette** | 1 | 0=Input, 1=Output, 2=Output+EOT | Filename |
| **Modem/RS-232** | 2 | 0 | Control registers |
| **Screen** | 3 | 0 or 1 | Text to PRINT |
| **Printer** | 4 or 5 | 0=Upper/Graphics, 7=Upper/Lower | Text to PRINT |
| **Disk** | 8-11 | 2-14=Data, 15=Command | Drive#, Filename, Type, R/W |

### Examples

```basic
REM Printer
100 OPEN 4,4 : PRINT#4,"HELLO"

REM Disk
110 OPEN 3,8,3,"0:DATA,S,W" : PRINT#3,"DISK DATA"

REM Tape
120 OPEN 1,1,1,"DATAFILE" : PRINT#1,"TAPE DATA"

REM Modem
130 OPEN 2,2,0,CHR$(10) : PRINT#2,"MODEM DATA"
```

---

## Printer Output

### Opening Printer Channel

```basic
REM Upper case with graphics
OPEN 1,4

REM Upper and lower case
OPEN 1,4,7
```

### Printer Control Codes

| CHR$ Code | Function |
|-----------|----------|
| 10 | Line feed |
| 13 | RETURN (automatic line feed on Commodore printers) |
| 14 | Begin double-width character mode |
| 15 | End double-width character mode |
| 18 | Begin reverse character mode |
| 146 | End reverse character mode |
| 17 | Switch to upper/lower case character set |
| 145 | Switch to upper case/graphics character set |
| 16 | Tab to position in next 2 characters |
| 27 | Move to specified dot position |
| 8 | Begin dot-programmable graphic mode |
| 26 | Repeat graphics data |

### Character Set Switching

**Within one character set:**
```basic
REM In upper/graphics mode, switch one line to upper/lower
10 OPEN 1,4
20 PRINT#1,"UPPER CASE"
30 PRINT#1,CHR$(17);"Mixed Case";CHR$(145)
40 PRINT#1,"UPPER CASE AGAIN"
```

**Important notes:**
- TAB() doesn't work correctly on printer (uses screen cursor position)
- Use SPC() for spacing instead
- Character set applies to entire OPEN session unless switched per line

---

## Modem / RS-232 Output

### ASCII Character Translation

**Important:** Commodore uses PETSCII, not ASCII
- Most computers use ASCII (American Standard Code for Information Interchange)
- Must translate Commodore codes to ASCII when communicating
- See Appendix C for ASCII code table

### OPEN Statement for Modem

```basic
OPEN 1,2,0,CHR$(6)                    : REM 300 baud
OPEN 2,2,0,CHR$(163)+CHR$(112)        : REM 110 baud with parity
```

**Parameters:**
- First character: Baud rate, data bits, stop bits
- Second character (optional): Parity and duplex

**Communication requirements:**
- Know receiving device's protocol
- Count characters and RETURNs carefully
- Automated login requires precise character timing

---

## Cassette Tape I/O

### Tape Characteristics

**Advantages:**
- Almost unlimited capacity (longer tape = more data)
- Inexpensive storage medium

**Limitations:**
- Sequential access only (must read through data)
- Time-consuming for large files
- ~50 bytes/second transfer rate

### Programming Strategy

**Common approach:**
1. Read entire cassette file into RAM
2. Process data in memory
3. Write all data back to tape

**Limitation:** File size restricted to available RAM

**When to use disk instead:**
- Data file larger than available RAM
- Need random access to records
- Business applications (ledgers, mailing lists)
- Frequent updates to data

### PRINT# Formatting for Tape

**Problem with comma separator:**
```basic
REM BAD - Wastes space
PRINT#1, A$, B$, C$
REM Produces: DOG       CAT       TREE[RETURN]
REM (Variable spacing wastes tape)
```

**INPUT# will fail:**
```basic
REM A$ gets all three values: "DOG       CAT       TREE"
INPUT#1, A$, B$, C$
```

**Solution - Use proper separators:**
```basic
REM GOOD - Use comma or RETURN separators
R$ = ","
PRINT#1, A$ R$ B$ R$ C$
REM Produces: DOG,CAT,TREE[RETURN]

REM Or use individual PRINT# statements
PRINT#1, A$
PRINT#1, B$
PRINT#1, C$
```

### GET# Special Handling

**Empty string issue:**
```basic
REM WRONG - Will error on CHR$(0)
GET#1, A$ : A = ASC(A$)

REM RIGHT - Protect against empty string
GET#1, A$ : A = ASC(A$ + CHR$(0))
```

**Why:** CHR$(0) is received as empty string "", not as one-character string with code 0

---

## Floppy Disk I/O

### Three File Types

1. **Sequential files** - Similar to tape, but multiple files can be open simultaneously
2. **Relative files** - Organized into records; read/replace individual records
3. **Random files** - Access any 256-byte block directly

### Data Formatting

**Same limitations as tape:**
- Need RETURN or comma separators
- CHR$(0) reads as empty string with GET#
- PRINT# formatting affects INPUT# behavior

### Channels

**Data and command channels:**
- Data channel: Temporary buffer in disk drive RAM
- Command channel: Tells drive where to write buffered data
- Allows efficient block or record operations

**Best for large data:**
- Use relative files for databases
- Provides speed and flexibility
- See disk drive manual for complete programming guide

---

## Game Ports (Joysticks and Paddles)

### Port Locations

**Two 9-pin game ports:**
- **Port 1** (Control Port A)
- **Port 2** (Control Port B)

**Each port accepts:**
- One joystick, OR
- One pair of paddles (2 paddles), OR
- Light pen (Port 1 only)

### Joystick Hardware

**Digital joystick - 5 switches:**
- Up (Switch 0)
- Down (Switch 1)
- Left (Switch 2)
- Right (Switch 3)
- Fire button (Switch 4)

**Wiring:**
```
         (Top)
         FIRE
      (Switch 4)

    UP
(Switch 0)

LEFT ----+---- RIGHT
(Switch 2)     (Switch 3)

      DOWN
   (Switch 1)
```

### Reading Joysticks from BASIC

**Memory locations:**
- Port 1: PEEK(56320) - CIA #1 Data Port A ($DC00)
- Port 2: PEEK(56321) - CIA #1 Data Port B ($DC01)

**Bit mapping:**
- Bit 0 = Up
- Bit 1 = Down
- Bit 2 = Left
- Bit 3 = Right
- Bit 4 = Fire button

**Logic:** 0 = pressed, 1 = not pressed (inverted)

### Joystick Reading Routine

```basic
10 FOR K=0 TO 10 : REM Set up direction string
20 READ DR$(K) : NEXT
30 DATA "","N","S","","W","NW"
40 DATA "SW","","E","NE","SE"
50 PRINT "GOING...";
60 GOSUB 100 : REM Read the joystick
65 IF DR$(JV)="" THEN 80 : REM Check if direction chosen
70 PRINT DR$(JV);" "; : REM Output direction
80 IF FR=16 THEN 60 : REM Check if fire button pushed
90 PRINT "-----F-----I-----R-----E-----!!!" : GOTO 60
100 JV=PEEK(56320) : REM Get joystick value (Port 1)
110 FR=JV AND 16 : REM Form fire button status
120 JV=15-(JV AND 15) : REM Form direction value
130 RETURN
```

**For Port 2:** Change line 100 to `JV=PEEK(56321)`

### Direction Value Table

| JV Value | Direction |
|----------|-----------|
| 0 | NONE |
| 1 | UP |
| 2 | DOWN |
| 3 | - (invalid) |
| 4 | LEFT |
| 5 | UP & LEFT |
| 6 | DOWN & LEFT |
| 7 | - (invalid) |
| 8 | RIGHT |
| 9 | UP & RIGHT |
| 10 | DOWN & RIGHT |

---

## Paddles

### Hardware Connection

**Paddle inputs:**
- Connected to CIA #1 and SID chip
- Analog input through SID registers
- **Port 1:** X-axis=54297 ($D419), Y-axis=54298 ($D41A)
- **Port 2:** X-axis=54299 ($D41B), Y-axis=54300 ($D41C)

### Critical Warning

**⚠️ PADDLES ARE NOT RELIABLE WHEN READ FROM BASIC ALONE!**

Must use machine language routine to read paddles accurately.

### Using Paddle Reading Routine

**From BASIC:**
```basic
10 C=12*4096 : REM Set paddle routine start
11 REM Poke in the paddle reading routine (see below)
15 FOR I=0 TO 63 : READ A : POKE C+I,A : NEXT
20 SYS C : REM Call paddle routine
30 P1=PEEK(C+257) : REM Paddle one value
40 P2=PEEK(C+258) : REM Paddle two value
50 P3=PEEK(C+259) : REM Paddle three value
60 P4=PEEK(C+260) : REM Paddle four value
61 REM Read fire button status
62 S1=PEEK(C+261) : S2=PEEK(C+262)
70 PRINT P1,P2,P3,P4 : REM Print paddle values
75 PRINT : PRINT "FIRE A ";S1,"FIRE B ";S2
80 FOR W=1 TO 50 : NEXT : REM Wait
90 PRINT "{CLEAR}" : PRINT : GOTO 20
95 REM Data for machine code routine
100 DATA 162,1,120,173,2,220,141,0,193,169,192,141,2,220,169
110 DATA 128,141,0,220,160,128,234,136,16,252,173,25,212,157
120 DATA 1,193,173,26,212,157,3,193,173,0,220,9,128,141,5,193
130 DATA 169,64,202,16,222,173,0,193,141,2,220,173,1,220,141
140 DATA 6,193,88,96
```

**Paddle values:** 0-255 (full range of potentiometer)

---

## Light Pen

### Hardware

**Connection:** Port 1 only (Pin 6)

**Registers:**
- LPX (register 19 / $13): X position (8 MSB of 9-bit counter)
- LPY (register 20 / $14): Y position (8 bits, full raster resolution)

### Operation

**Triggering:**
- Latches position on low-going edge
- Can only trigger once per frame
- Subsequent triggers in same frame have no effect

**Resolution:**
- X: 2 horizontal dots (9-bit counter provides 512 states)
- Y: Single raster line (8-bit value)

**Best practice:**
- Take multiple samples (3+) and average
- Required due to single-trigger-per-frame limitation
- Depends on light pen characteristics

---

## RS-232 Interface

### Overview

**Built-in RS-232 interface:**
- Connects to modems, printers, other RS-232 devices
- Standard RS-232 format
- **Voltage levels:** TTL (0-5V) instead of standard ±12V
- Requires cable with voltage conversion (Commodore RS-232 cartridge handles this)

### Access Methods

**From BASIC:**
- OPEN, CLOSE, CMD, INPUT#, GET#, PRINT#
- ST variable for status

**From Machine Language:**
- KERNAL routines
- Direct register access

### Interrupts

**NMI-based processing:**
- Uses CIA #2 timers and interrupts
- Generates NMI (Non-Maskable Interrupt) requests
- Allows background RS-232 operation during BASIC/ML programs

**Hold-offs:**
- Built into KERNAL, cassette, serial bus routines
- During cassette/serial bus activity: RS-232 cannot receive
- Prevents data corruption
- No interference if programming is careful

### Buffers

**Two 256-byte FIFO buffers:**
- Transmit buffer
- Receive buffer
- **Total:** 512 bytes allocated at top of memory

**⚠️ CRITICAL WARNING:**
- OPEN automatically allocates 512 bytes
- If insufficient free space exists: NO ERROR MESSAGE
- **End of BASIC program will be destroyed!**
- ALWAYS check available memory before OPEN

**Buffer management:**
- Automatically allocated on OPEN
- Automatically removed on CLOSE

---

## Opening RS-232 Channel

### BASIC Syntax

```basic
OPEN lfn, 2, 0, "<control><command><opt_low><opt_high>"
```

**Parameters:**
- **lfn** - Logical file number (1-255)
  - If lfn > 127: Line feed follows all carriage returns
- **2** - Device number (always 2 for RS-232)
- **0** - Secondary address (always 0)
- **Filename string:**
  - First character: Control register (baud rate, word length, stop bits)
  - Second character: Command register (parity, duplex, handshake) - optional
  - Third/Fourth characters: Reserved for future use

### Control Register (Required)

**Bits 0-3: Baud Rate**

| Bits 3-0 | Baud Rate |
|----------|-----------|
| 0000 | User rate (uses opt bytes) |
| 0001 | 50 baud |
| 0010 | 75 baud |
| 0011 | 110 baud |
| 0100 | 134.5 baud |
| 0101 | 150 baud |
| 0110 | 300 baud |
| 0111 | 600 baud |
| 1000 | 1200 baud |
| 1001 | 1800 / 2400 baud |
| 1010 | 2400 baud |
| 1011 | 3600 baud (NTSC only) |
| 1100 | 4800 baud (NTSC only) |
| 1101 | 7200 baud (NTSC only) |
| 1110 | 9600 baud (NTSC only) |
| 1111 | 19200 baud (NTSC only) |

**Bits 5-6: Data Word Length**

| Bits 6-5 | Word Length |
|----------|-------------|
| 00 | 8 bits |
| 01 | 7 bits |
| 10 | 6 bits |
| 11 | 5 bits |

**Bit 7: Stop Bits**
- 0 = 1 stop bit
- 1 = 2 stop bits

**Bit 4:** Unused

### Command Register (Optional)

**Bits 5-7: Parity Options**

| Bits 7-6-5 | Parity |
|------------|--------|
| --0 | Parity disabled, none generated/received |
| 001 | Odd parity receiver/transmitter |
| 011 | Even parity receiver/transmitter |
| 101 | Mark transmitted, parity check disabled |
| 111 | Space transmitted, parity check disabled |

**Bit 4: Duplex**
- 0 = Full duplex
- 1 = Half duplex

**Bits 0-1: Handshake**
- 00 = 3-line (no DSR, no DCD)
- 01 = X-line (Xmodem standard)

**Bits 2-3:** Unused

### User-Defined Baud Rate

**When bits 0-3 of control register = 0000:**

Use optional baud bytes to define custom rate:

```basic
opt_baud_low = (system_frequency / rate / 2 - 100) - (opt_baud_high * 256)
opt_baud_high = INT((system_frequency / rate / 2 - 100) / 256)
```

**System frequencies:**
- NTSC (North America): 1.02273E6
- PAL (Europe/UK): 0.98525E6

### Examples

```basic
REM 300 baud, 8 data bits, 1 stop bit
OPEN 1,2,0,CHR$(6)

REM 110 baud with parity and other options
OPEN 2,2,0,CHR$(163)+CHR$(112)

REM Custom baud rate
OPEN 3,2,0,CHR$(0)+CHR$(0)+CHR$(low)+CHR$(high)
```

### Important Notes

**Only one RS-232 channel at a time:**
- Second OPEN resets buffer pointers
- All buffered data lost

**No error checking:**
- Non-implemented baud rate → very slow output (< 50 baud)
- Illegal control word silently fails

**⚠️ OPEN before variables/arrays:**
- RS-232 OPEN performs automatic CLR
- Creates variables/arrays after OPEN to prevent loss

---

## Getting Data from RS-232

### Buffer Capacity

**Receive buffer:** 255 characters before overflow

**Overflow indication:**
- ST variable in BASIC
- RSSTAT in machine language

**On overflow:**
- All characters received during full buffer are lost
- Clear buffer regularly to prevent data loss

### Speed Considerations

**BASIC limitations:**
- Cannot handle high-speed bursts
- Garbage collection can cause overflow
- Use machine language for high-speed reception

### BASIC Syntax

```basic
REM Recommended
GET#lfn, string_variable$

REM NOT recommended
INPUT#lfn, variable_list
```

### KERNAL Entries

- **CHKIN ($FFC6)** - Set input channel
- **GETIN ($FFE4)** - Get character (non-blocking)
- **CHRIN ($FFCF)** - Get character (blocking)

### Important Notes

**Word length < 8 bits:**
- Unused bits set to zero

**GET# with no data:**
- Returns "" (null/empty string)

**INPUT# behavior (why NOT recommended):**
- Hangs waiting for non-null character + carriage return
- If CTS or DSR lines disappear: System hangs (RESTORE-only recovery)
- Cannot abort gracefully

**Handshaking:**
- CHKIN handles x-line handshake per EIA RS-232-C standard (August 1979)
- RTS, CTS, DCD lines implemented
- Commodore 64 defined as Data Terminal device

---

## Sending Data to RS-232

### Buffer Capacity

**Transmit buffer:** 255 characters before hold-off

**On full buffer:**
- System waits in CHROUT routine
- Waits until transmission allowed
- Recovery: RUN/STOP + RESTORE (WARM START)

### BASIC Syntax

```basic
CMD lfn                    : REM Redirect output
PRINT#lfn, variable_list   : REM Print to RS-232
```

### KERNAL Entries

- **CHKOUT ($FFC9)** - Set output channel
- **CHROUT ($FFD2)** - Output character

---

## Quick Reference Tables

### Device Numbers

| Device | Number | Purpose |
|--------|--------|---------|
| Cassette | 1 | Tape storage |
| RS-232 | 2 | Serial communication |
| Screen | 3 | Display output |
| Printer | 4-5 | Hardcopy output |
| Disk | 8-11 | Floppy disk storage |

### Common Memory Locations

| Address | Decimal | Purpose |
|---------|---------|---------|
| $DC00 | 56320 | CIA #1 Port A (Joystick 1) |
| $DC01 | 56321 | CIA #1 Port B (Joystick 2) |
| $DC02 | 56322 | CIA #1 Data Direction A |
| $DC03 | 56323 | CIA #1 Data Direction B |
| $D419 | 54297 | SID Paddle X (Port 1) |
| $D41A | 54298 | SID Paddle Y (Port 1) |
| $D41B | 54299 | SID Paddle X (Port 2) |
| $D41C | 54300 | SID Paddle Y (Port 2) |

### File Type Summary

| Type | Device | Access | Best For |
|------|--------|--------|----------|
| Sequential | Tape/Disk | Linear | Simple data files |
| Relative | Disk | Record-based | Databases, random access |
| Random | Disk | Block-based | Low-level disk access |

---

## Best Practices

### General I/O

1. **Check available memory before RS-232 OPEN**
2. **Use GET# instead of INPUT# for RS-232**
3. **Always use proper separators in file output (comma or RETURN)**
4. **Protect against empty strings: `A=ASC(A$+CHR$(0))`**
5. **Close files when done to free buffers**

### Tape I/O

1. **Read entire file into RAM, process, write back**
2. **Don't use comma separator in PRINT# (wastes space)**
3. **Use RETURN or comma as explicit separators**
4. **Consider disk for files larger than RAM**

### RS-232

1. **OPEN before creating variables (CLR is automatic)**
2. **Monitor buffer status to prevent overflow**
3. **Use machine language for high-speed communication**
4. **Translate PETSCII to ASCII when needed**

### Game Controllers

1. **Use machine language routine for paddles (BASIC unreliable)**
2. **Take multiple samples for light pen (3+ averaged)**
3. **Check for diagonal joystick movement (two bits set)**
4. **Remember: 0 = pressed, 1 = not pressed (inverted logic)**

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 6
- **Related References:**
  - Appendix C: Character codes (PETSCII and ASCII)
  - Appendix I: Hardware pinouts
  - Memory map: KERNAL routines and chip registers
  - Disk drive manual: Advanced disk programming

---

**Document Version:** 1.0 (Part 1 of 2)
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications

**Note:** This covers Part 1 of Chapter 6. Part 2 will cover User Port, Serial Bus, Expansion Port, and Z-80 Cartridge.
