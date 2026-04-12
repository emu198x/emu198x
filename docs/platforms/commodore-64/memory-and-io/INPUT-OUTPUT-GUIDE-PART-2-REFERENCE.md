# Input/Output Guide - Part 2 Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 6 (Part 2)

---

## Overview

This document continues the I/O guide with detailed coverage of RS-232 closing procedures, User Port programming, Serial Bus operation, Expansion Port specifications, and Z-80 cartridge usage.

**Topics covered:**
- RS-232 channel closing and status
- User Port pinouts and programming
- Serial Bus (IEC) protocol
- Expansion Port specifications
- Z-80 CP/M cartridge operation

---

## Closing RS-232 Channel

### Effects of CLOSE

**When closing an RS-232 file:**
1. Discards all buffered data (transmit and receive)
2. Stops all RS-232 transmitting and receiving
3. Sets RTS and Sout lines high
4. Removes both 512-byte buffers from memory

### BASIC Syntax

```basic
CLOSE lfn
```

### KERNAL Entry

**CLOSE ($FFC3)**
- See Memory Map for entry/exit conditions

### Critical Warning

**⚠️ Data loss on CLOSE:**
- All data in buffers is discarded immediately
- Data may not have been transmitted yet
- No automatic flush before closing

### Ensure Data Transmitted

**Before closing, wait for transmission to complete:**

```basic
100 SS=ST : IF (SS=0 OR SS=8) THEN 100
110 CLOSE lfn
```

**Explanation:**
- ST=0 or ST=8: Transmission in progress
- Loop until other status indicates completion
- Then safe to CLOSE

---

## RS-232 Status Register

### Memory Location

**RSSTAT ($0297)** - Machine language
**ST** - BASIC variable

### Status Bit Definitions

| Bit | Name | Description |
|-----|------|-------------|
| 0 | Parity Error | Parity mismatch detected |
| 1 | Framing Error | Invalid stop bit |
| 2 | Receiver Buffer Overrun | Buffer full, data lost |
| 3 | Receiver Buffer Empty | No data available (use after GET#) |
| 4 | CTS Signal Missing | Clear To Send line inactive |
| 5 | Unused | Reserved |
| 6 | DSR Signal Missing | Data Set Ready line inactive |
| 7 | Break Detected | Break signal received |

**Bit = 0:** No error
**Bit = 1:** Error condition

### Reading Status

**From BASIC:**
```basic
SR = ST : REM Assigns ST to SR (status is cleared on read)
```

**Important notes:**
- ST is cleared after reading (in BASIC or KERNAL READST)
- If multiple checks needed, assign to variable first
- Status only updated when RS-232 was last I/O used

### User Port Pin Mapping (RS-232)

| Pin | CIA #2 | Description | EIA | Signal | I/O | Modes |
|-----|--------|-------------|-----|--------|-----|-------|
| C | PB0 | Received Data | BB | Sin | IN | 1 2 |
| D | PB1 | Request To Send | CA | RTS | OUT | 1*2 |
| E | PB2 | Data Terminal Ready | CD | DTR | OUT | 1*2 |
| F | PB3 | Ring Indicator | CE | RI | IN | 3 |
| H | PB4 | Received Line Signal | CF | DCD | IN | 2 |
| J | PB5 | Unassigned | - | XXX | IN | 3 |
| K | PB6 | Clear To Send | CB | CTS | IN | 2 |
| L | PB7 | Data Set Ready | CC | DSR | IN | 2 |
| B | FLAG2 | Received Data | BB | Sin | IN | 1 2 |
| M | PA2 | Transmitted Data | BA | Sout | OUT | 1 2 |
| A | GND | Protective Ground | AA | GND | - | 1 2 |
| N | GND | Signal Ground | AB | GND | - | 1 2 3 |

**Modes:**
- 1 = 3-line interface (Sin, Sout, GND)
- 2 = X-line interface (full handshaking)
- 3 = User available (unused in RS-232)

*Lines held high during 3-line mode

---

## RS-232 Memory Locations

### Buffer Pointers

**$00F7-$00F8 (REBUF):** Receiver buffer base address (2 bytes)
**$00F9-$00FA (ROBUF):** Transmitter buffer base address (2 bytes)

**Management:**
- Set by OPEN routine
- Deallocated by CLOSE (writes 0 to high bytes)
- Can be manually allocated/deallocated in ML

### Zero-Page Locations (Internal Use)

| Address | Name | Purpose |
|---------|------|---------|
| $00A7 | INBIT | Receiver input bit temp storage |
| $00A8 | BITCI | Receiver bit count in |
| $00A9 | RINONE | Receiver flag start bit check |
| $00AA | RIDATA | Receiver byte buffer/assembly |
| $00AB | RIPRTY | Receiver parity bit storage |
| $00B4 | BITTS | Transmitter bit count out |
| $00B5 | NXTBIT | Transmitter next bit to send |
| $00B6 | RODATA | Transmitter byte buffer/disassembly |

**Note:** These are internal and cannot be used directly

### Non-Zero-Page Locations

| Address | Name | Purpose |
|---------|------|---------|
| $0293 | M51CTR | Pseudo 6551 control register |
| $0294 | M51COR | Pseudo 6551 command register |
| $0295-$0296 | M51AJB | Baud rate calculation bytes |
| $0297 | RSSTAT | RS-232 status register |
| $0298 | BITNUM | Number of bits to send/receive |
| $0299-$029A | BAUDOF | Time of one bit cell |
| $029B | RIDBE | Receiver FIFO buffer end index |
| $029C | RIDBS | Receiver FIFO buffer start index |
| $029D | RODBS | Transmitter FIFO buffer start index |
| $029E | RODBE | Transmitter FIFO buffer end index |
| $02A1 | ENABL | Active interrupts in CIA #2 ICR |

**ENABL bit meanings:**
- Bit 4 = 1: Waiting for receiver edge
- Bit 1 = 1: Receiving data
- Bit 0 = 1: Transmitting data

---

## Sample RS-232 Programs

### Terminal Program (Silent 700)

```basic
10 REM SENDS/RECEIVES DATA TO/FROM SILENT 700 TERMINAL
11 REM MODIFIED FOR PET ASCII
20 REM SILENT 700 SETUP: 300 BAUD, 7-BIT ASCII, MARK PARITY,
21 REM FULL DUPLEX
30 REM SAME SETUP AT COMPUTER USING 3-LINE INTERFACE
100 OPEN 2,2,3,CHR$(6+32)+CHR$(32+128) : REM Open channel
110 GET#2,A$ : REM Turn on receiver (toss null)
200 REM Main loop
210 GET B$ : REM Get from keyboard
220 IF B$<>"" THEN PRINT#2,B$; : REM Send to terminal
230 GET#2,C$ : REM Get from terminal
240 PRINT B$;C$; : REM Print to screen
250 SR=ST : IF SR=0 OR SR=8 THEN 200 : REM Check status
300 REM Error reporting
310 PRINT "ERROR: ";
320 IF SR AND 1 THEN PRINT "PARITY"
330 IF SR AND 2 THEN PRINT "FRAME"
340 IF SR AND 4 THEN PRINT "RECEIVER BUFFER FULL"
350 IF SR AND 128 THEN PRINT "BREAK"
360 IF (PEEK(673) AND 1) THEN 360 : REM Wait for transmit done
370 CLOSE 2 : END
```

### ASCII Translation Program

```basic
10 REM SENDS/RECEIVES TRUE ASCII DATA
100 OPEN 5,2,3,CHR$(6)
110 DIM F%(255),T%(255)
200 FOR J=32 TO 64 : T%(J)=J : NEXT
210 T%(13)=13 : T%(20)=8 : RV=18 : CT=0
220 FOR J=65 TO 90 : K=J+32 : T%(J)=K : NEXT
230 FOR J=91 TO 95 : T%(J)=J : NEXT
240 FOR J=193 TO 218 : K=J-128 : T%(J)=K : NEXT
250 T%(146)=16 : T%(133)=16
260 FOR J=0 TO 255
270 K=T%(J)
280 IF K<>0 THEN F%(K)=J : F%(K+128)=J
290 NEXT
300 PRINT CHR$(147)
310 GET#5,A$
320 IF A$="" OR ST<>0 THEN 360
330 PRINT CHR$(157);CHR$(F%(ASC(A$)));
340 IF F%(ASC(A$))=34 THEN POKE 212,0
350 GOTO 310
360 PRINT CHR$(RV);CHR$(157);CHR$(146); : GET A$
370 IF A$<>"" THEN PRINT#5,CHR$(T%(ASC(A$)));
380 CT=CT+1
390 IF CT=8 THEN CT=0 : RV=164-RV
410 GOTO 310
```

---

## The User Port

### Purpose

**Universal I/O connection:**
- Connect to printers, modems, other computers
- Connected directly to CIA #2 (6526 chip)
- Provides 8 parallel I/O lines plus handshaking

### Physical Connector

**24-pin edge connector (2×12)**
**Location:** Rear of C64

```
Top Side:    1  2  3  4  5  6  7  8  9  10 11 12
Bottom Side: A  B  C  D  E  F  H  J  K  L  M  N
```

(Note: Pin G is missing - not used)

### Complete Pin Descriptions

#### Top Side (Numbered Pins)

| Pin | Signal | Description | Max Current |
|-----|--------|-------------|-------------|
| 1 | GND | Ground | - |
| 2 | +5V | 5V power supply | 100mA MAX |
| 3 | RESET | Cold start (active low) | - |
| 4 | CNT1 | Serial counter CIA #1 | - |
| 5 | SP1 | Serial port CIA #1 | - |
| 6 | CNT2 | Serial counter CIA #2 | - |
| 7 | SP2 | Serial port CIA #2 | - |
| 8 | PC2 | Handshaking CIA #2 | - |
| 9 | SERIAL ATN | Serial bus ATN line | - |
| 10 | 9 VAC | AC from transformer | 50mA MAX |
| 11 | 9 VAC | AC from transformer | 50mA MAX |
| 12 | GND | Ground | - |

#### Bottom Side (Lettered Pins)

| Pin | Signal | CIA Pin | Description |
|-----|--------|---------|-------------|
| A | GND | - | Ground |
| B | FLAG2 | FLAG2 | Interrupt input (handshaking) |
| C | PB0 | Port B bit 0 | I/O line 0 |
| D | PB1 | Port B bit 1 | I/O line 1 |
| E | PB2 | Port B bit 2 | I/O line 2 |
| F | PB3 | Port B bit 3 | I/O line 3 |
| H | PB4 | Port B bit 4 | I/O line 4 |
| J | PB5 | Port B bit 5 | I/O line 5 |
| K | PB6 | Port B bit 6 | I/O line 6 |
| L | PB7 | Port B bit 7 | I/O line 7 |
| M | PA2 | Port A bit 2 | Special I/O |
| N | GND | - | Ground |

### Programming the User Port

**Port B control:**
- **Port register:** 56577 ($DD01)
- **Data Direction Register (DDR):** 56579 ($DD03)

**DDR bit values:**
- 0 = Input
- 1 = Output

**Example: Set lines 3, 4, 5 as outputs, rest as inputs:**

```basic
REM DDR bits: 7 6 5 4 3 2 1 0
REM Values:   0 0 1 1 1 0 0 0
REM Decimal: 2^5 + 2^4 + 2^3 = 32 + 16 + 8 = 56

10 POKE 56579, 56 : REM Set data direction
20 POKE 56577, 32 : REM Set bit 5 high (output)
30 VALUE = PEEK(56577) : REM Read port state
```

### Handshaking Lines

**FLAG2 (Pin B):**
- Negative edge sensitive input
- Sets FLAG interrupt bit on falling edge
- Can trigger interrupt or be polled
- Used for handshaking input

**PA2 (Pin M):**
- Bit 2 of Port A
- Controlled like any I/O bit
- Port A: 56576 ($DD00)
- DDR A: 56578 ($DD02)
- Used for handshaking output

**Example: Using PA2:**
```basic
10 POKE 56578, 4 : REM Set PA2 as output (bit 2 = 1)
20 POKE 56576, 4 : REM Set PA2 high
30 POKE 56576, 0 : REM Set PA2 low
```

### Handshaking Concept

**Why needed:**
- Devices operate at different speeds
- Need coordination for data transfer
- Sender signals "data ready"
- Receiver signals "data accepted"

**Common pattern:**
1. Sender prepares data on parallel lines
2. Sender pulses handshake line
3. Receiver reads data
4. Receiver acknowledges via handshake

---

## The Serial Bus (IEC Bus)

### Overview

**Daisy-chain serial bus:**
- Up to 5 devices can connect
- Common bus shared by all devices
- C64 acts as controller
- Devices are talkers, listeners, or both

### Bus Roles

**CONTROLLER (Commodore 64):**
- Controls bus operation
- Commands devices to TALK or LISTEN
- Only C64 can be controller

**TALKER:**
- Transmits data onto bus
- Only one talker at a time
- Examples: Disk drive (reading), C64 (saving)

**LISTENER:**
- Receives data from bus
- Multiple listeners allowed simultaneously
- Examples: Printer, disk drive (writing)

### Device Addressing

**Address range:** 4-31

**Common devices:**
| Device Number | Device Type |
|---------------|-------------|
| 4 or 5 | VIC-1525 Graphics Printer |
| 8 | VIC-1541 Disk Drive |

**Secondary address:**
- Transmits setup information
- Device-specific meaning
- Example: Printer secondary address 7 = upper/lower case mode

```basic
OPEN 1,4,7 : REM Printer in upper/lower case mode
```

### Bus Physical Connections

**6-pin DIN connector**

| Pin | Signal | Direction |
|-----|--------|-----------|
| 1 | SERIAL SRQ IN | Device → C64 |
| 2 | GND | Ground |
| 3 | SERIAL ATN IN/OUT | Bidirectional |
| 4 | SERIAL CLK IN/OUT | Bidirectional |
| 5 | SERIAL DATA IN/OUT | Bidirectional |
| 6 | NO CONNECTION | - |

### Bus Signals

**SRQ (Service Request):**
- Device pulls low when needs attention
- C64 responds to service the device

**ATN (Attention):**
- C64 pulls low to start command sequence
- All devices listen for address
- Device must respond within timeout

**CLK (Clock):**
- Synchronizes data transmission
- Timing provided by active device

**DATA:**
- Serial data transmitted one bit at a time
- LSB first (bit 0), MSB last (bit 7)

### Serial Bus Timing

| Description | Symbol | Min | Typ | Max |
|-------------|--------|-----|-----|-----|
| ATN Response (required) | TAT | - | - | 1000µs |
| Listener Hold-Off | TH | 0 | - | ∞ |
| Non-EOI Response to RFD | TNE | - | 40µs | 200µs |
| Bit Set-Up Talker | TS | 20µs | 70µs | - |
| Data Valid | TV | 20µs | 20µs | - |
| Frame Handshake | TF | 0 | 20µs | 1000µs |
| Frame to Release ATN | TR | 20µs | - | - |
| Between Bytes Time | TBB | 100µs | - | - |
| EOI Response Time | TYE | 200µs | 250µs | - |
| EOI Response Hold | TEI | 60µs | - | - |
| Talker Response Limit | TRY | 0 | 30µs | 60µs |
| Byte-Acknowledge | TPR | 20µs | 30µs | - |
| Talk-Attention Release | TTK | 20µs | 30µs | 100µs |
| Talk-Attention Acknowledge | TDC | 0 | - | - |
| Talk-Attention Ack Hold | TDA | 80µs | - | - |
| EOI Acknowledge | TFR | 60µs | - | - |

**Timeout notes:**
1. Max TAT exceeded → Device not present error
2. Max TNE exceeded → EOI response required
3. Max TF exceeded → Frame error
4. TV and TPR min must be 60µs for external talker
5. TEI min must be 80µs for external listener

### Programming the Serial Bus

**BASIC commands:**
- LOAD, SAVE, VERIFY - File operations
- OPEN, CLOSE - Channel management
- PRINT#, INPUT#, GET# - Data transfer

**Example:**
```basic
10 OPEN 1,8,15,"I0" : REM Open command channel, initialize
20 OPEN 2,8,2,"DATA,S,R" : REM Open data file for reading
30 INPUT#2,A$ : REM Read data
40 CLOSE 2 : CLOSE 1
```

---

## The Expansion Port

### Physical Specifications

**Connector type:** 44-pin (22/22) female edge connector
**Location:** Rear right of C64
**Required cable:** 44-pin male edge connector

### Pin Layout

```
Top Edge (Component Side):
22 21 20 19 18 17 16 15 14 13 12 11 10 9  8  7  6  5  4  3  2  1
■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■

■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■
Z  Y  X  W  V  U  T  S  R  P  N  M  L  K  J  H  F  E  D  C  B  A

Bottom Edge (Solder Side)
```

### Complete Pinout

**Component Side (numbered 1-22):**

| Pin | Signal | Description |
|-----|--------|-------------|
| 1 | GND | System ground |
| 2 | +5VDC | +5V power supply |
| 3 | +5VDC | +5V power supply |
| 4 | IRQ | Interrupt request (active low) |
| 5 | R/W | Read/Write (write active low) |
| 6 | DOT CLOCK | 8.18 MHz video dot clock |
| 7 | I/O1 | I/O block 1 @ $DE00-$DEFF (unbuffered) |
| 8 | GAME | Active low LS TTL input |
| 9 | EXROM | Active low LS TTL input |
| 10 | I/O2 | I/O block 2 @ $DF00-$DFFF (buffered) |
| 11 | ROML | 8K ROM @ $8000 (buffered) |
| 12 | BA | Bus available from VIC-II |
| 13 | DMA | Direct memory access (active low) |
| 14-21 | D7-D0 | Data bus bits 7-0 |
| 22 | GND | System ground |

**Solder Side (lettered A-Z):**

| Pin | Signal | Description |
|-----|--------|-------------|
| A | GND | System ground |
| B | ROMH | 8K ROM @ $E000 (buffered) |
| C | RESET | 6502 RESET (active low) |
| D | NMI | Non-maskable interrupt (active low) |
| E | φ2 | Phase 2 system clock |
| F-Y | A15-A0 | Address bus bits 15-0 |
| Z | GND | System ground |

### Power Specifications

**Maximum current:**
- User Port + Cartridge combined: 450mA maximum
- Exceeding this can damage C64

### Important Expansion Port Signals

**DOT CLOCK (Pin 6):**
- 8.18 MHz NTSC / 7.88 MHz PAL
- All system timing derived from this

**BA - Bus Available (Pin 12):**
- Goes low 3 cycles before VIC-II takes bus
- Stays low until VIC-II finishes display fetch
- Unbuffered, 1 LS TTL load max

**DMA - Direct Memory Access (Pin 13):**
- Pull low to request bus control
- 6510 address/data/R-W lines go hi-Z
- **Only pull low when φ2 is low**
- External device must follow VIC-II timing
- Line is pulled up on C64

### Memory Decode Signals

**ROML (Pin 11):** 8K ROM at $8000-$9FFF
**ROMH (Pin B):** 8K ROM at $E000-$FFFF
**I/O1 (Pin 7):** I/O area $DE00-$DEFF
**I/O2 (Pin 10):** I/O area $DF00-$DFFF

**GAME and EXROM pins:**
- Control memory configuration
- Determine cartridge mapping mode

### Caution

**⚠️ Direct bus access can damage C64:**
- No protection circuitry
- Incorrect voltages can destroy chips
- Short circuits can damage system
- Test carefully before connection

---

## Z-80 Microprocessor Cartridge

### Overview

**Purpose:** Run CP/M software on C64

**CP/M characteristics:**
- Not computer-dependent OS
- Uses memory space for operating system
- Access to large CP/M software library
- Programs portable to other CP/M systems

### Advantages

- Run CP/M and Z-80 software on C64
- Large software library available
- Programs portable to other CP/M computers
- External cartridge (no internal installation)

### Disadvantages

- Shorter programs (OS uses memory)
- No C64 screen editing capabilities
- Cannot use both processors simultaneously

### Installation

**Physical:**
- Plug Z-80 cartridge into expansion port
- No internal installation required
- No messy wires

**Software:**
- Provided with CP/M diskette
- Load and run CP/M program

### Running CP/M

```basic
1. LOAD CP/M program from disk
2. Type RUN
3. Press RETURN
```

### Memory Configuration

**6510 mode:** 64K RAM accessible
**Z-80 mode:** 48K RAM accessible

**Memory cannot be used simultaneously by both processors**

### Memory Address Translation

Z-80 addresses are offset by $1000 (4096) from 6510 addresses:

| Z-80 Address | 6510 Address |
|--------------|--------------|
| $0000-$0FFF | $1000-$1FFF |
| $1000-$1FFF | $2000-$2FFF |
| $2000-$2FFF | $3000-$3FFF |
| $3000-$3FFF | $4000-$4FFF |
| $4000-$4FFF | $5000-$5FFF |
| $5000-$5FFF | $6000-$6FFF |
| $6000-$6FFF | $7000-$7FFF |
| $7000-$7FFF | $8000-$8FFF |
| $8000-$8FFF | $9000-$9FFF |
| $9000-$9FFF | $A000-$AFFF |
| $A000-$AFFF | $B000-$BFFF |
| $B000-$BFFF | $C000-$CFFF |
| $C000-$CFFF | $D000-$DFFF |
| $D000-$DFFF | $E000-$EFFF |
| $E000-$EFFF | $F000-$FFFF |
| $F000-$FFFF | $0000-$0FFF |

**Pattern:** Z-80 address + $1000 = 6510 address (wrapping at 64K)

### Z-80 Enable/Disable Program

```basic
10 REM Z80 CARD CONTROL PROGRAM
20 REM STORES Z80 DATA AT $1000 (Z80=$0000)
30 REM THEN TURNS OFF 6510 IRQS AND ENABLES
40 REM Z80 CARD. Z80 CARD MUST BE TURNED OFF
50 REM TO REENABLE 6510 SYSTEM.
100 REM STORE Z80 DATA
110 READ B : REM Get size of Z80 code
120 FOR I=4096 TO 4096+B-1 : REM Move code
130 READ A : POKE I,A
140 NEXT I
200 REM RUN Z80 CODE
210 POKE 56333,127 : REM Turn off 6510 IRQs
220 POKE 56832,0   : REM Turn on Z80 card
230 POKE 56333,129 : REM Turn on 6510 IRQs when Z80 done
240 END
1000 REM Z80 MACHINE LANGUAGE DATA SECTION
1010 DATA 18 : REM Size of data
1100 REM Z80 TURN ON CODE
1110 DATA 0,0,0 : REM Z80 card requires turn on time at $0000
1200 REM Z80 TASK DATA
1210 DATA 33,2,245 : REM LD HL,NN (screen location)
1220 DATA 52 : REM INC HL (increment)
1300 REM Z80 SELF-TURN OFF DATA
1310 DATA 62,1 : REM LD A,N
1320 DATA 50,0,206 : REM LD (NN),A (I/O location)
1330 DATA 0,0,0 : REM NOP, NOP, NOP
1340 DATA 195,0,0 : REM JMP $0000
```

### Z-80 Operation Details

**Switching processors:**
1. Disable 6510 interrupts
2. Enable Z-80 via I/O register
3. Z-80 executes code
4. Z-80 disables itself when done
5. 6510 resumes operation

**Sophisticated timing:**
- Only one processor active at a time
- Automatic coordination
- No conflicts

---

## Quick Reference Tables

### I/O Device Numbers

| Device | Number | Type |
|--------|--------|------|
| Cassette | 1 | Sequential |
| RS-232/Modem | 2 | Serial |
| Screen | 3 | Display |
| Printer | 4-5 | Serial bus |
| Disk | 8-11 | Serial bus |

### Serial Bus Devices

| Number | Device | Notes |
|--------|--------|-------|
| 4 or 5 | VIC-1525 Printer | User selectable |
| 8 | VIC-1541 Disk | Default |
| 9-11 | Additional Disks | Requires address change |

### User Port Power

| Pin | Type | Maximum |
|-----|------|---------|
| 2 | +5V DC | 100mA |
| 10-11 | 9V AC | 50mA each |

### Expansion Port Power

| Combined Devices | Maximum |
|------------------|---------|
| User Port + Cartridge | 450mA |

### CIA #2 Addresses

| Register | Address | Hex | Purpose |
|----------|---------|-----|---------|
| Port A | 56576 | $DD00 | I/O + RS-232 Sout |
| Port B | 56577 | $DD01 | User Port I/O |
| DDR A | 56578 | $DD02 | Data direction A |
| DDR B | 56579 | $DD03 | Data direction B |

---

## Best Practices

### RS-232

1. **Always check transmission complete before CLOSE**
2. **Save ST to variable for multiple checks**
3. **Use GET# instead of INPUT# for reliability**
4. **Monitor buffer status to prevent overflow**
5. **Translate PETSCII to ASCII when needed**

### User Port

1. **Set DDR before using port**
2. **Check pin capabilities (buffered vs unbuffered)**
3. **Don't exceed current limits**
4. **Use external power for high-current devices**
5. **Implement proper handshaking for reliable communication**

### Serial Bus

1. **Only one talker at a time**
2. **Multiple listeners allowed**
3. **Device must respond within timeout**
4. **Use proper secondary addresses**
5. **Close files when done**

### Expansion Port

1. **⚠️ Can damage C64 - test carefully**
2. **Respect current limits**
3. **Pull DMA low only when φ2 is low**
4. **Conform to VIC-II timing when using DMA**
5. **Check GAME/EXROM configuration**

### Z-80 Cartridge

1. **Cannot use both processors simultaneously**
2. **Account for $1000 address offset**
3. **Disable 6510 IRQs before switching**
4. **Z-80 must disable itself when done**
5. **CP/M limits available RAM to 48K**

---

## Common Pitfalls

### RS-232

- Closing before data transmitted (use ST check loop)
- Using INPUT# instead of GET# (causes hangs)
- Not translating PETSCII to ASCII
- Ignoring status bits
- Opening without sufficient free memory

### User Port

- Not setting DDR before using port
- Exceeding current limits
- Confusing input/output bit meanings (0/1)
- Forgetting handshaking for reliable transfer
- Accessing FLAG2 without enabling interrupts

### Serial Bus

- Multiple talkers simultaneously (causes collision)
- Not checking device present error
- Wrong device numbers
- Forgetting secondary addresses
- Exceeding 5-device limit

### Expansion Port

- Hot-plugging (power on during insertion)
- Pulling DMA when φ2 high
- Exceeding 450mA current limit
- Wrong GAME/EXROM configuration
- Unbuffered signals with >1 LS TTL load

### Z-80

- Forgetting $1000 address offset
- Attempting to use both CPUs simultaneously
- Not disabling 6510 IRQs
- Exceeding 48K RAM limit under CP/M
- Wrong memory translation

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 6
- **Related References:**
  - Appendix I: Hardware pinouts detail
  - Appendix M: 6526 CIA chip specifications
  - VIC-II chip specifications for DMA timing
  - Disk drive manual for serial bus programming
  - Z-80 Reference Guide for CP/M details

---

**Document Version:** 1.0 (Part 2 of 2)
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications

**Note:** This completes Chapter 6 coverage. Combined with Part 1, provides complete I/O programming reference for all C64 peripherals and expansion options.
