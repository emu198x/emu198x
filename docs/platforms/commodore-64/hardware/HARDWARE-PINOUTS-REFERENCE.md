# Hardware Pinouts Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Appendix I

---

## Overview

The Commodore 64 provides seven different types of I/O connections for peripherals, expansion cartridges, and custom hardware. This reference documents the complete pinout specifications for each port.

**Available ports:**
1. Game I/O (Control Ports 1 & 2)
2. Cartridge Expansion Slot
3. Audio/Video Port
4. Serial I/O (Disk/Printer)
5. Modulator Output
6. Cassette Port
7. User Port

---

## 1. Game I/O Ports (Joystick/Paddle/Light Pen)

The C64 has two 9-pin D-sub connectors for game controllers, located on the right side of the case.

### Control Port 1

**Connector Type:** 9-pin D-sub male (DE-9)
**Location:** Right side of C64, labeled "CONTROL PORT 1"

| Pin | Signal | Description | Notes |
|-----|--------|-------------|-------|
| 1 | JOY A0 | Joystick up | Active low (0V = pressed) |
| 2 | JOY A1 | Joystick down | Active low (0V = pressed) |
| 3 | JOY A2 | Joystick left | Active low (0V = pressed) |
| 4 | JOY A3 | Joystick right | Active low (0V = pressed) |
| 5 | POT AY | Paddle Y-axis | Analog input 0-470kΩ |
| 6 | BUTTON A/LP | Fire button / Light pen | Active low |
| 7 | +5V | Power supply | MAX. 50mA |
| 8 | GND | Ground | 0V reference |
| 9 | POT AX | Paddle X-axis | Analog input 0-470kΩ |

**Pin numbering:**
```
     1  2  3  4  5
      ○  ○  ○  ○  ○
        ○  ○  ○  ○
        6  7  8  9
```

### Control Port 2

**Connector Type:** 9-pin D-sub male (DE-9)
**Location:** Right side of C64, labeled "CONTROL PORT 2"

| Pin | Signal | Description | Notes |
|-----|--------|-------------|-------|
| 1 | JOY B0 | Joystick up | Active low (0V = pressed) |
| 2 | JOY B1 | Joystick down | Active low (0V = pressed) |
| 3 | JOY B2 | Joystick left | Active low (0V = pressed) |
| 4 | JOY B3 | Joystick right | Active low (0V = pressed) |
| 5 | POT BY | Paddle Y-axis | Analog input 0-470kΩ |
| 6 | BUTTON B/LP | Fire button / Light pen | Active low |
| 7 | +5V | Power supply | MAX. 50mA |
| 8 | GND | Ground | 0V reference |
| 9 | POT BX | Paddle X-axis | Analog input 0-470kΩ |

### Programming Notes

**Reading joystick:**
- Port 1: `PEEK(56320)` - CIA #1 Data Port A
- Port 2: `PEEK(56321)` - CIA #1 Data Port B
- Bit 0 = Up, Bit 1 = Down, Bit 2 = Left, Bit 3 = Right, Bit 4 = Fire
- 0 = pressed, 1 = not pressed (inverted logic)

**Reading paddles:**
- Port 1 X-axis: SID register 54297 ($D419)
- Port 1 Y-axis: SID register 54298 ($D41A)
- Port 2 X-axis: SID register 54299 ($D41B)
- Port 2 Y-axis: SID register 54300 ($D41C)

**Power limitations:**
- Maximum 50mA draw from +5V pin
- Exceeding this can damage the CIA chip
- Use external power for active devices

---

## 2. Cartridge Expansion Slot

**Connector Type:** 44-pin edge connector (2×22)
**Location:** Rear of C64, under the cartridge door

### Complete Pinout

| Pin | Side A | Side B | Pin | Description |
|-----|--------|--------|-----|-------------|
| 1 | GND | GND | A | Ground |
| 2 | +5V | ROMH | B | +5V power / ROMH select |
| 3 | +5V | RESET | C | +5V power / Reset line |
| 4 | IRQ | NMI | D | IRQ interrupt / NMI interrupt |
| 5 | R/W | S02 | E | Read/Write signal / Phase 2 clock |
| 6 | Dot Clock | A15 | F | Dot clock / Address line 15 |
| 7 | I/O 1 | A14 | H | I/O select 1 / Address 14 |
| 8 | GAME | A13 | J | GAME control / Address 13 |
| 9 | EXROM | A12 | K | EXROM control / Address 12 |
| 10 | I/O 2 | A11 | L | I/O select 2 / Address 11 |
| 11 | ROML | A10 | M | ROML select / Address 10 |
| 12 | BA | A9 | N | Bus available / Address 9 |
| 13 | DMA | A8 | P | DMA signal / Address 8 |
| 14 | D7 | A7 | R | Data bit 7 / Address 7 |
| 15 | D6 | A6 | S | Data bit 6 / Address 6 |
| 16 | D5 | A5 | T | Data bit 5 / Address 5 |
| 17 | D4 | A4 | U | Data bit 4 / Address 4 |
| 18 | D3 | A3 | V | Data bit 3 / Address 3 |
| 19 | D2 | A2 | W | Data bit 2 / Address 2 |
| 20 | D1 | A1 | X | Data bit 1 / Address 1 |
| 21 | D0 | A0 | Y | Data bit 0 / Address 0 |
| 22 | GND | GND | Z | Ground |

### Physical Layout

```
Top Edge (looking at connector from front):
22 21 20 19 18 17 16 15 14 13 12 11 10 9  8  7  6  5  4  3  2  1
■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■
■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■
Z  Y  X  W  V  U  T  S  R  P  N  M  L  K  J  H  F  E  D  C  B  A

Bottom Edge
```

### Signal Groups

**Data Bus (D0-D7):**
- Bidirectional 8-bit data bus
- Pins 14-21 (side A)

**Address Bus (A0-A15):**
- 16-bit address bus
- Pins F-Y (side B)

**Control Signals:**
- ROMH, ROML: ROM chip select signals
- I/O 1, I/O 2: I/O area select signals
- GAME, EXROM: Memory configuration control
- R/W: Read/Write direction
- IRQ, NMI: Interrupt request lines
- RESET: System reset
- DMA: Direct Memory Access
- BA: Bus Available (for DMA)

**Clock Signals:**
- Dot Clock: 8.18 MHz (NTSC) / 7.88 MHz (PAL)
- S02: Phase 2 clock (1 MHz)

**Power:**
- +5V: Regulated 5V supply
- GND: Ground (multiple pins for stability)

### Programming Notes

**Cartridge types:**
- 8K cartridges: Use ROML ($8000-$9FFF)
- 16K cartridges: Use ROML + ROMH ($8000-$9FFF, $A000-$BFFF)
- Ultimax mode: GAME=0, EXROM=1
- Standard mode: GAME=1, EXROM=0

**Auto-start cartridges:**
- Cold start vector at $8000 or $A000
- Must contain specific cartridge signature
- BASIC ROM disabled when EXROM=0

---

## 3. Audio/Video Port

**Connector Type:** 5-pin DIN (240°)
**Location:** Rear of C64, labeled "AUDIO/VIDEO"

### Pinout

| Pin | Signal | Description |
|-----|--------|-------------|
| 1 | LUMINANCE | Luma (Y) - Brightness signal |
| 2 | GND | Ground |
| 3 | AUDIO OUT | Composite audio output |
| 4 | VIDEO OUT | Composite video (CVBS) |
| 5 | AUDIO IN | Audio input (from cassette) |

### Physical Layout

```
     ___
   /  3  \
  |2     4|
   \ 1 5 /
     ---
(Looking into jack on C64)
```

### Video Specifications

**Composite video (Pin 4):**
- Combined sync, luminance, and chrominance
- NTSC: 525 lines, 60 Hz
- PAL: 625 lines, 50 Hz
- 1V peak-to-peak, 75Ω

**Luminance (Pin 1):**
- Brightness information only
- For use with separate chroma encoder
- Better quality than composite alone

**Audio (Pin 3):**
- Line-level output from SID chip
- Mono signal
- ~1V peak-to-peak

### Programming Notes

**Video output is automatic:**
- VIC-II generates all video signals
- No software control of output format
- Quality varies by monitor/TV connection

**Audio mixing:**
- SID output appears at pin 3
- Can be mixed externally with audio input (pin 5)
- Cassette audio fed to pin 5

---

## 4. Serial I/O Port (IEC Bus)

**Connector Type:** 6-pin DIN (240°)
**Location:** Rear of C64, labeled "SERIAL"

### Pinout

| Pin | Signal | Description |
|-----|--------|-------------|
| 1 | SERIAL SRQIN | Service request input |
| 2 | GND | Ground |
| 3 | SERIAL ATN IN/OUT | Attention (bus control) |
| 4 | SERIAL CLK IN/OUT | Clock (data synchronization) |
| 5 | SERIAL DATA IN/OUT | Data line |
| 6 | RESET | System reset |

### Physical Layout

```
     ___
   /  5  \
  |4     6|
   \ 2 3 /
     ---
    / 1 \
(Looking into jack on C64)
```

### IEC Bus Protocol

**Signal functions:**
- **ATN (Attention):** C64 asserts to gain control of bus
- **CLK (Clock):** Synchronizes data transfer
- **DATA:** Carries actual data bits
- **SRQIN:** Peripheral can request service (rarely used)
- **RESET:** Resets all devices on bus

**Electrical characteristics:**
- TTL logic levels (0V = logic 0, 5V = logic 1)
- Open-collector outputs (pull-down only)
- Requires pull-up resistors in devices
- Maximum bus length: ~2 meters practical

### Programming Notes

**BASIC commands:**
- `LOAD "filename",8` - Load from device 8 (disk)
- `SAVE "filename",8` - Save to device 8
- `OPEN 1,8,15,"command"` - Send disk command

**Device numbers:**
- 8-11: Disk drives (default: 8)
- 4-7: Printers (default: 4)

**Kernal routines:**
- SETLFS ($FFBA) - Set logical file parameters
- SETNAM ($FFBD) - Set filename
- OPEN ($FFC0) - Open file
- CLOSE ($FFC3) - Close file
- CHKIN ($FFC6) - Set input channel
- CHKOUT ($FFC9) - Set output channel
- CLRCHN ($FFCC) - Clear I/O channels

---

## 5. Modulator Output

**Connector Type:** RCA/RF coaxial
**Location:** Rear of C64
**Output:** Channel 3 or 4 RF modulated signal

**Notes:**
- Internal RF modulator converts video/audio to TV channel
- Lower quality than direct video connection
- Used for connection to TV antenna input
- Switch on modulator selects channel 3 or 4

---

## 6. Cassette Port

**Connector Type:** 6-pin edge connector
**Location:** Rear of C64, labeled "CASSETTE"

### Pinout

| Pin | Signal | Description |
|-----|---------|-------------|
| A (1) | GND | Ground |
| B (2) | +5V | Power supply to cassette |
| C (3) | Cassette Motor | Motor control (relay) |
| D (4) | Cassette Read | Data input from tape |
| E (5) | Cassette Write | Data output to tape |
| F (6) | Cassette Sense | Detects key press on cassette |

### Physical Layout

```
  1  2  3  4  5  6
 ■  ■  ■  ■  ■  ■

 ■  ■  ■  ■  ■  ■
  A  B  C  D  E  F
```

### Signal Details

**Motor control (Pin C):**
- Controls cassette motor relay
- Active high (5V = motor on)
- Controlled by Kernal routines

**Read (Pin D):**
- TTL-level input
- Reads data pulses from tape
- Connected to CIA #1 flag input

**Write (Pin E):**
- TTL-level output
- Writes data pulses to tape
- Controlled by Kernal timing

**Sense (Pin F):**
- Detects cassette PLAY/RECORD keys
- Prevents motor start without key pressed
- Safety feature to protect tapes

### Programming Notes

**BASIC commands:**
- `LOAD` - Load from cassette (device 1)
- `SAVE "name",1` - Save to cassette
- `VERIFY` - Verify tape data

**Data format:**
- Uses frequency shift keying (FSK)
- Short pulses = 1, long pulses = 0
- Includes leader tones, sync pulses, checksums
- ~50 bytes/second transfer rate

---

## 7. User Port

**Connector Type:** 24-pin edge connector (2×12)
**Location:** Rear of C64, labeled "USER PORT"

### Complete Pinout

**Numbered side (1-12):**

| Pin | Signal | Notes |
|-----|--------|-------|
| 1 | GND | Ground |
| 2 | +5V | MAX. 100mA |
| 3 | RESET | System reset (active low) |
| 4 | CNT1 | CIA #2 counter input |
| 5 | SP1 | CIA #2 serial port |
| 6 | CNT2 | CIA #2 counter input |
| 7 | SP2 | CIA #2 serial port |
| 8 | PC2 | CIA #2 handshake line |
| 9 | SER. ATN IN | Serial attention input |
| 10 | 9 VAC | MAX. 100mA (from power supply) |
| 11 | 9 VAC | MAX. 100mA (from power supply) |
| 12 | GND | Ground |

**Lettered side (A-N):**

| Pin | Signal | Notes |
|-----|--------|-------|
| A | GND | Ground |
| B | FLAG2 | CIA #2 interrupt input |
| C | PB0 | CIA #2 Port B bit 0 |
| D | PB1 | CIA #2 Port B bit 1 |
| E | PB2 | CIA #2 Port B bit 2 |
| F | PB3 | CIA #2 Port B bit 3 |
| H | PB4 | CIA #2 Port B bit 4 |
| J | PB5 | CIA #2 Port B bit 5 |
| K | PB6 | CIA #2 Port B bit 6 |
| L | PB7 | CIA #2 Port B bit 7 |
| M | PA2 | CIA #2 Port A bit 2 |
| N | GND | Ground |

### Physical Layout

```
  1  2  3  4  5  6  7  8  9  10 11 12
 ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■

 ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■  ■
  A  B  C  D  E  F  H  J  K  L  M  N
(Note: Pin G is missing - not used)
```

### Signal Groups

**Parallel I/O (PB0-PB7, PA2):**
- 8-bit parallel port (Port B)
- 1 additional bit from Port A
- Bidirectional (input or output)
- Controlled via CIA #2 registers

**Serial I/O (SP1, SP2, CNT1, CNT2):**
- Hardware serial shift register
- Can implement custom protocols
- CNT lines for counting external events
- SP lines for serial data

**Handshaking (PC2, FLAG2):**
- PC2: Handshake output
- FLAG2: Interrupt input (falling edge)
- Used for synchronized data transfer

**Power:**
- +5V DC (100mA max)
- 9V AC (100mA max, each pin)
- Multiple grounds for stability

### Programming Notes

**CIA #2 registers:**
- 56576 ($DD00): Data Port A
- 56577 ($DD01): Data Port B
- 56578 ($DD02): Data Direction A (0=input, 1=output)
- 56579 ($DD03): Data Direction B (0=input, 1=output)

**Example: Set PB0-PB7 as outputs:**
```basic
10 POKE 56579,255 : REM All Port B pins = output
20 POKE 56577,170 : REM Binary 10101010
```

**Example: Read PB0-PB7 as inputs:**
```basic
10 POKE 56579,0 : REM All Port B pins = input
20 VALUE = PEEK(56577)
30 PRINT VALUE
```

**Common uses:**
- RS-232 interfaces (modems)
- Parallel printer interfaces
- Custom I/O boards
- Control circuits (relays, motors)
- Data acquisition
- MIDI interfaces

---

## Power Supply Specifications

### Voltage Rails Available

| Rail | Location | Max Current | Notes |
|------|----------|-------------|-------|
| +5V DC | Multiple ports | Varies by port | Regulated 5V for TTL logic |
| 9V AC | User port only | 100mA per pin | Unregulated transformer output |
| GND | All ports | N/A | 0V reference |

### Current Limits by Port

| Port | Maximum Draw | Notes |
|------|--------------|-------|
| Control ports | 50mA each | Total from both ports: 100mA |
| User port +5V | 100mA | Pin 2 only |
| User port 9VAC | 100mA per pin | Pins 10 & 11 |
| Cartridge port | 500mA typical | Check C64 model specs |

### Important Warnings

**Do not exceed current limits:**
- Overloading can damage CIA chips
- May cause system instability
- Can damage power supply

**External power recommended for:**
- Motors and relays
- High-current LEDs
- Active logic circuits
- Multiple peripherals

---

## Connector Availability

### Modern Replacements

| Original Part | Modern Equivalent | Notes |
|---------------|-------------------|-------|
| 9-pin D-sub male | DE-9 male | Joystick ports |
| 5-pin DIN 240° | DIN-5 180° or 240° | A/V port |
| 6-pin DIN 240° | DIN-6 240° | Serial, cassette |
| Edge connectors | Custom PCB | Cartridge, user port |

### Pin Numbering Standards

**D-sub connectors:**
- Numbered 1-9
- Looking at male pins (on C64)
- Top row: 1-5, bottom row: 6-9

**DIN connectors:**
- Numbered clockwise from top
- Looking into female jack (on C64)

**Edge connectors:**
- Numbered/lettered both sides
- Component side vs. solder side
- Confirm orientation before connecting

---

## Safety and Best Practices

### Electrical Safety

1. **Always power off before connecting:**
   - Turn off C64 and peripherals
   - Prevents damage from hot-plugging
   - Protects against shorts during insertion

2. **Check polarity:**
   - Verify pin assignments
   - Wrong polarity can destroy chips
   - Use multimeter to confirm signals

3. **Use current limiting:**
   - Add resistors for LED circuits
   - Fuses for motor circuits
   - Protect C64 from overload

### ESD Protection

- Use anti-static wrist strap
- Touch grounded metal before handling
- Avoid touching exposed connector pins
- Store cartridges in anti-static bags

### Signal Integrity

- Keep cable runs short (<2 meters)
- Use shielded cables for high-speed signals
- Avoid parallel runs with power cables
- Terminate unused inputs properly

---

## Quick Reference: Register Addresses

### CIA Chip Registers

**CIA #1 (Keyboard, Joysticks):**
- 56320-56335 ($DC00-$DC0F)

**CIA #2 (User Port, Serial):**
- 56576-56591 ($DD00-$DD0F)

### VIC-II Chip Registers

**Video control:**
- 53248-53294 ($D000-$D02E)

### SID Chip Registers

**Sound and paddle inputs:**
- 54272-54296 ($D400-$D418)
- Paddle inputs: 54297-54300 ($D419-$D41C)

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Appendix I
- **Related References:**
  - CIA chip datasheet (MOS 6526/8520)
  - VIC-II chip datasheet (MOS 6567/6569)
  - IEC bus specification
  - User port programming guides

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications

---

## Appendix: Connector Diagrams

### D-Sub 9 (Male) - Joystick Ports

```
     1  2  3  4  5
      ○  ○  ○  ○  ○
        ○  ○  ○  ○
        6  7  8  9

Shell: Connected to GND
Pin 1: Upper left
Pin 5: Upper right
Pin 6: Lower left
Pin 9: Lower right
```

### 5-Pin DIN - Audio/Video

```
        3
       ○
    ○     ○
    2     4
     ○   ○
      1 5

Looking into female jack on C64
Numbering: Clockwise from top
```

### 6-Pin DIN - Serial/Cassette

```
        5
       ○
    ○     ○
    4     6
     ○   ○
      2 3
       ○
       1

Looking into female jack on C64
Pin 1: Bottom center
Pins 2-6: Clockwise from bottom
```

### Edge Connectors

Always verify pinout with multimeter before connecting custom hardware. Edge connectors can be inserted backwards on some devices.

---

**End of Hardware Pinouts Reference**
