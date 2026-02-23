# 6526 CIA (Complex Interface Adapter) Reference

**⚠️ FOR LESSON CREATION: See [CIA-QUICK-REFERENCE.md](CIA-QUICK-REFERENCE.md) first (90% smaller, programming-focused)**

**Source:** C64 Programmer's Reference Guide - Appendix M
**For:** Code Like It's 198x - Assembly Language Lessons
**Applies to:** Commodore 64 (all revisions)

---

## Quick Start

**If you're creating lessons**, you probably want [CIA-QUICK-REFERENCE.md](CIA-QUICK-REFERENCE.md) instead. It contains:
- Joystick reading patterns
- Keyboard scanning basics
- Timer delay code
- Essential registers only
- **12KB vs 107KB** (this file)

**This comprehensive reference** contains hardware specifications useful for deep technical work:
- Pin configurations and electrical specifications
- Timing diagrams and hardware protocols
- Complete register bit descriptions
- Advanced handshaking and serial bus details

---

## Document Status

This reference covers the 6526 Complex Interface Adapter (CIA) chips used in the Commodore 64. The C64 has two CIA chips that handle I/O, timers, and system timing.

**Currently Documented:**
- ✅ CIA Overview & Features
- ✅ Pin Configuration
- ✅ Block Diagram
- ✅ Electrical Characteristics
- ✅ Complete Register Map
- ✅ Detailed Register Descriptions
- ✅ Interface Signal Descriptions
- ✅ Timing Characteristics (Read/Write Cycles)
- ✅ Functional Description (Complete)
- ✅ Programming Guide (All Features)

---

## Table of Contents

1. [Overview](#overview)
2. [CIA Chips in the C64](#cia-chips-in-the-c64)
3. [Key Features](#key-features)
4. [Pin Configuration](#pin-configuration)
5. [Block Diagram](#block-diagram)
6. [Electrical Specifications](#electrical-specifications)
7. [Interface Signals](#interface-signals)
8. [Timing Characteristics](#timing-characteristics)
9. [Register Map](#register-map)
10. [Register Descriptions](#register-descriptions)
11. [Functional Description](#functional-description)
12. [Quick Reference](#quick-reference)

---

## Overview

The 6526 Complex Interface Adapter (CIA) is a sophisticated peripheral interface chip designed for the 65xx family of microprocessors. It provides flexible I/O capabilities, precision timing, and interrupt management.

### What the CIA Does

The CIA is a **multi-function peripheral chip** that handles:
- **Parallel I/O** - 16 programmable I/O lines (two 8-bit ports)
- **Timers** - Two independent 16-bit interval timers
- **Time-of-Day Clock** - 24-hour AM/PM clock with alarm
- **Serial I/O** - 8-bit shift register for serial communication
- **Handshaking** - Automated read/write handshaking protocols
- **Interrupt Generation** - Multiple interrupt sources with masking

### Why Two CIAs?

The C64 uses **two CIA chips** because 16 I/O lines aren't enough for all the peripherals:
- **CIA #1 ($DC00-$DC0F)** - Keyboard, joysticks, paddles
- **CIA #2 ($DD00-$DD0F)** - Serial bus, RS-232, User Port, NMI generation

### CIA vs Other Chips

| Feature | CIA (6526) | VIC-II | SID |
|---------|-----------|---------|-----|
| **Primary Function** | I/O & Timing | Video | Sound |
| **I/O Lines** | 16 (two 8-bit ports) | 0 | 0 |
| **Timers** | 2 × 16-bit | 0 | 3 × 24-bit |
| **Interrupt Sources** | 5 (FLAG, Timer A/B, TOD, Serial) | 4 (raster, sprite) | 0 |
| **Real-Time Clock** | Yes (TOD) | No | No |

---

## CIA Chips in the C64

The Commodore 64 contains two 6526 CIA chips with different responsibilities.

### CIA #1 - Keyboard and User Input

**Base Address:** `$DC00` (56320 decimal)
**IRQ Line:** Connected to CPU IRQ (maskable interrupt)

| Register Range | Function |
|----------------|----------|
| **$DC00** | Port A Data (keyboard rows, joystick 2) |
| **$DC01** | Port B Data (keyboard columns, joystick 1) |
| **$DC02** | Port A DDR (Data Direction Register) |
| **$DC03** | Port B DDR |
| **$DC04-$DC05** | Timer A (16-bit) |
| **$DC06-$DC07** | Timer B (16-bit) |
| **$DC08-$DC0B** | Time-of-Day clock |
| **$DC0C** | Serial shift register |
| **$DC0D** | Interrupt control register |
| **$DC0E** | Timer A control |
| **$DC0F** | Timer B control |

**Primary Uses:**
- Keyboard matrix scanning (8×8 matrix)
- Joystick 1 and 2 reading
- Paddle/mouse input
- Timer-based interrupts for music/effects
- Time-of-day clock

**Example - Reading Keyboard:**
```assembly
LDA #$00    ; Set Port A as outputs (keyboard rows)
STA $DC02
LDA #$FF    ; Set Port B as inputs (keyboard columns)
STA $DC03

LDA #$FE    ; Scan row 0 (activate row 0)
STA $DC00
LDA $DC01   ; Read columns
            ; Bits 0-7 represent keys in that row
```

### CIA #2 - Serial Bus and System Control

**Base Address:** `$DD00` (56576 decimal)
**NMI Line:** Connected to CPU NMI (non-maskable interrupt)

| Register Range | Function |
|----------------|----------|
| **$DD00** | Port A Data (serial bus, VIC bank, RS-232) |
| **$DD01** | Port B Data (RS-232, User Port) |
| **$DD02** | Port A DDR |
| **$DD03** | Port B DDR |
| **$DD04-$DD05** | Timer A (16-bit) |
| **$DD06-$DD07** | Timer B (16-bit) |
| **$DD08-$DD0B** | Time-of-Day clock (usually unused) |
| **$DD0C** | Serial shift register |
| **$DD0D** | Interrupt control register (triggers NMI) |
| **$DD0E** | Timer A control |
| **$DD0F** | Timer B control |

**Primary Uses:**
- IEC serial bus control (disk drive, printer)
- VIC-II memory bank selection (bits 0-1 of $DD00)
- RS-232 serial communication
- User Port parallel I/O
- RESTORE key NMI generation

**Example - Switching VIC-II Bank:**
```assembly
; VIC-II bank selection via CIA #2 Port A bits 0-1
; Bits are inverted: 00 = bank 3, 11 = bank 0

LDA $DD00
AND #%11111100  ; Clear bits 0-1
ORA #%00000011  ; Set both bits (select bank 0: $0000-$3FFF)
STA $DD00
```

### Critical Difference: IRQ vs NMI

| CIA #1 | CIA #2 |
|--------|--------|
| Generates **IRQ** (maskable) | Generates **NMI** (non-maskable) |
| Can be disabled with SEI | Cannot be disabled |
| Used for regular timing | Used for critical events (RESTORE key) |
| Multiple sources can share IRQ line | Dedicated NMI line |

---

## Key Features

The 6526 CIA provides sophisticated peripheral interfacing capabilities that were advanced for 1982.

### 1. Dual 8-Bit Parallel I/O Ports

**16 I/O lines total** organized as two ports (A and B):
- Each bit individually programmable as input or output
- Built-in pull-up resistors on all I/O pins
- TTL compatible (can drive 2 TTL loads)
- CMOS compatible inputs

**Per-Bit Configuration:**
```assembly
; Configure Port A: bits 0-3 outputs, bits 4-7 inputs
LDA #%00001111
STA $DC02       ; CIA #1 Port A DDR
                ; 1 = output, 0 = input
```

### 2. Two Independent 16-Bit Timers

**Timer A and Timer B** - Each can:
- Count system clock cycles (φ2)
- Count external events (CNT pin)
- Count Timer A underflows (Timer B only)
- Generate interrupts on underflow
- Operate in one-shot or continuous mode
- Be linked together for 32-bit timing

**Timer Range:**
- **Minimum:** 1 cycle (~1 microsecond at 1 MHz)
- **Maximum:** 65535 cycles (~65.5 milliseconds at 1 MHz)
- **Linked (32-bit):** ~71 minutes maximum

**Common Uses:**
- **Music/Sound:** Precise timing for note duration
- **Raster Effects:** Synchronize with screen refresh
- **Delays:** Accurate timing loops
- **Event Counting:** External pulse counting

### 3. Time-of-Day (TOD) Clock

**24-hour clock** with:
- Hours (12-hour AM/PM or 24-hour format)
- Minutes (00-59)
- Seconds (00-59)
- Tenths of seconds (0-9)
- Programmable alarm function
- 50Hz or 60Hz input (matches power frequency)

**Typical Use:**
- Real-time clock for applications
- Timestamp for data logging
- Alarm-based interrupts
- Rarely used in games (timers preferred)

### 4. 8-Bit Serial Shift Register

**Serial I/O** capabilities:
- Converts parallel data to serial (and vice versa)
- Programmable shift direction
- Can be clocked by Timer A
- Used for custom serial protocols

**In the C64:**
- **CIA #1:** Serial port (rarely used)
- **CIA #2:** IEC serial bus communication (disk drives)

### 5. Handshaking Support

**Automated handshaking** for parallel data transfer:
- **PC (Port Control)** output signals data ready
- **FLAG** input signals data accepted
- 8-bit or 16-bit handshaking modes
- Reduces CPU overhead for I/O transfers

### 6. Flexible Interrupt System

**5 interrupt sources per CIA:**
1. Timer A underflow
2. Timer B underflow
3. Time-of-Day alarm
4. Serial shift register full/empty
5. FLAG pin (external signal)

**Features:**
- Individual interrupt enable/disable masks
- Interrupt status register
- One-shot or continuous interrupt generation
- **CIA #1:** Generates IRQ (maskable)
- **CIA #2:** Generates NMI (non-maskable)

---

## Pin Configuration

The 6526 is a 40-pin DIP (Dual Inline Package) chip.

```
        ┌───────────┐
  Vss  │1        40│ CNT
  PA₀  │2        39│ SP
  PA₁  │3        38│ RS₀
  PA₂  │4        37│ RS₁
  PA₃  │5        36│ RS₂
  PA₄  │6        35│ RS₃
  PA₅  │7        34│ RES
  PA₆  │8        33│ DB₀
  PA₇  │9        32│ DB₁
  PB₀  │10       31│ DB₂
  PB₁  │11       30│ DB₃
  PB₂  │12       29│ DB₄
  PB₃  │13       28│ DB₅
  PB₄  │14       27│ DB₆
  PB₅  │15       26│ DB₇
  PB₆  │16       25│ φ2
  PB₇  │17       24│ FLAG
  PC   │18       23│ CS
  TOD  │19       22│ R/W
  Vcc  │20       21│ IRQ
        └───────────┘
```

### Pin Descriptions

#### Power and Ground
| Pin | Name | Description |
|-----|------|-------------|
| 1 | Vss | Ground (0V) |
| 20 | Vcc | +5V power supply |

#### Port A (8-bit Parallel I/O)
| Pins | Name | Description |
|------|------|-------------|
| 2-9 | PA₀-PA₇ | Port A I/O lines (individually programmable) |

**In C64 CIA #1:**
- PA₀-PA₇: Keyboard row selection, joystick 2

**In C64 CIA #2:**
- PA₀-PA₁: VIC-II bank selection (inverted)
- PA₂: Serial bus ATN OUT
- PA₃-PA₇: Serial bus and RS-232 control

#### Port B (8-bit Parallel I/O)
| Pins | Name | Description |
|------|------|-------------|
| 10-17 | PB₀-PB₇ | Port B I/O lines (individually programmable) |

**In C64 CIA #1:**
- PB₀-PB₇: Keyboard column reading, joystick 1, paddles

**In C64 CIA #2:**
- PB₀-PB₇: User Port parallel I/O, RS-232 data

#### Data Bus (CPU Interface)
| Pins | Name | Description |
|------|------|-------------|
| 26-33 | DB₀-DB₇ | 8-bit bidirectional data bus (CPU connection) |

#### Register Select
| Pins | Name | Description |
|------|------|-------------|
| 35-38 | RS₀-RS₃ | Register select (4 bits = 16 registers) |

#### Control Signals
| Pin | Name | Description |
|-----|------|-------------|
| 18 | PC | Port Control output (handshaking) |
| 21 | IRQ | Interrupt Request output (active low) |
| 22 | R/W | Read/Write input (1=read, 0=write) |
| 23 | CS | Chip Select input (active low) |
| 24 | FLAG | External interrupt input (active low) |
| 25 | φ2 | System clock input (~1 MHz) |
| 34 | RES | Reset input (active low) |

#### Special Function Pins
| Pin | Name | Description |
|-----|------|-------------|
| 19 | TOD | Time-of-Day clock input (50/60 Hz) |
| 39 | SP | Serial Port (shift register I/O) |
| 40 | CNT | Counter input (Timer external clock) |

### C64-Specific Pin Connections

**CIA #1:**
- **IRQ** → CPU IRQ line (shared with VIC-II)
- **TOD** → 60 Hz signal (NTSC) or 50 Hz (PAL)
- **PA/PB** → Keyboard matrix, joysticks
- **FLAG** → Cassette port (data input)

**CIA #2:**
- **IRQ** → CPU NMI line
- **FLAG** → RESTORE key (triggers NMI)
- **PA** → Serial bus, VIC bank switching
- **PB** → User Port, RS-232

---

## Block Diagram

The 6526 contains several independent functional blocks:

```
┌─────────────────────────────────────────────────────┐
│                                                     │
│  ┌──────────────┐                                  │
│  │  Data Bus    │ ←→ DB₀-DB₇                       │
│  │  Buffers     │                                  │
│  └──────┬───────┘                                  │
│         │                                          │
│    ┌────┴─────┬──────────┬──────────┬────────┐    │
│    │          │          │          │        │    │
│  ┌─▼──┐    ┌─▼──┐    ┌─▼──┐    ┌─▼──┐  ┌──▼──┐  │
│  │PRA │    │PRB │    │DDRA│    │DDRB│  │ TOD │  │
│  │    │    │    │    │    │    │    │  │     │  │
│  └─┬──┘    └─┬──┘    └────┘    └────┘  └──┬──┘  │
│    │         │                             │     │
│  ┌─▼────┐  ┌─▼────┐                    ┌──▼──┐  │
│  │ PA   │  │ PB   │                    │ TOD/│  │
│  │Buffer│  │Buffer│                    │ALARM│  │
│  └─┬────┘  └─┬────┘                    └─────┘  │
│    │         │                                   │
│  PA₀-PA₇   PB₀-PB₇                              │
│                                                  │
│  ┌──────────┐  ┌──────────┐  ┌────────┐        │
│  │ Timer A  │  │ Timer B  │  │ Serial │        │
│  │  (CRA)   │  │  (CRB)   │  │  Port  │        │
│  └────┬─────┘  └────┬─────┘  └───┬────┘        │
│       │             │            │              │
│     CNT          Timer A        SP              │
│                                                  │
│  ┌─────────────────────────────┐                │
│  │   Interrupt Control Logic   │                │
│  │         (INT/MASK)           │                │
│  └──────────────┬───────────────┘                │
│                 │                                │
│               IRQ                                │
│                 │                                │
│  ┌──────────────▼───────────────┐                │
│  │   Chip Access Control        │                │
│  │   (R/W, φ2, CS, RS0-RS3)    │                │
│  └──────────────────────────────┘                │
│                                                  │
└─────────────────────────────────────────────────┘
```

### Functional Blocks

1. **Data Bus Buffers** - Interface to CPU data bus (bidirectional)
2. **Port A/B Registers (PRA/PRB)** - Data registers for I/O ports
3. **Data Direction Registers (DDRA/DDRB)** - Configure I/O direction per bit
4. **Port Buffers** - Drive external I/O pins with pull-ups
5. **Timer A** - 16-bit down counter with control register (CRA)
6. **Timer B** - 16-bit down counter with control register (CRB)
7. **Time-of-Day/Alarm** - Real-time clock with alarm comparison
8. **Serial Port** - 8-bit shift register for serial I/O
9. **Interrupt Control** - Manages 5 interrupt sources with masking
10. **Chip Access Control** - Register selection and bus timing

---

## Electrical Specifications

Understanding the electrical characteristics helps with hardware interfacing and debugging.

### Operating Conditions

| Parameter | Min | Typical | Max | Unit |
|-----------|-----|---------|-----|------|
| **Supply Voltage (Vcc)** | 4.75V | 5.0V | 5.25V | V |
| **Ground (Vss)** | 0V | 0V | 0V | V |
| **Operating Temperature** | 0°C | 25°C | 70°C | °C |
| **Storage Temperature** | -55°C | — | 150°C | °C |
| **Clock Frequency** | — | 1 MHz | 2 MHz | MHz |

**Note:** While the 6526 can operate at 2 MHz, the C64 runs at ~1 MHz (0.985 MHz PAL, 1.023 MHz NTSC).

### Input Voltage Levels

| Signal Type | Low (0) | High (1) | Unit |
|-------------|---------|----------|------|
| **Logic Inputs** | -0.3V to +0.8V | +2.4V to Vcc | V |
| **Data Bus (DB₀-DB₇)** | -0.3V to +0.8V | +2.4V to Vcc | V |

### Output Voltage Levels

| Condition | Min | Typical | Max | Unit |
|-----------|-----|---------|-----|------|
| **Output High (VOH)** | +2.4V | — | Vcc | V |
| **Output Low (VOL)** | 0V | — | +0.4V | V |

**Load Conditions:**
- VOH measured at IOH = -200µA (sourcing)
- VOL measured at IOL = 3.2mA (sinking)

### Current Specifications

| Parameter | Min | Typical | Max | Unit |
|-----------|-----|---------|-----|------|
| **Input Leakage Current** | — | 1.0µA | 2.5µA | µA |
| **Output High Current (sourcing)** | -200µA | -1mA | — | µA |
| **Output Low Current (sinking)** | 3.2mA | — | — | mA |
| **Three-State Leakage** | — | ±1.0µA | ±10.0µA | µA |
| **Power Supply Current (ICC)** | — | 70mA | 100mA | mA |

**Drive Capability:**
- Each I/O pin can drive **2 TTL loads**
- Each I/O pin can sink **3.2mA** (enough for LED with resistor)

### Port Input Pull-Up Resistors

| Parameter | Min | Typical | Max | Unit |
|-----------|-----|---------|-----|------|
| **Pull-up Resistance (RPI)** | 3.1kΩ | 5.0kΩ | — | kΩ |

**Why This Matters:**
- All I/O pins have internal pull-up resistors
- When configured as inputs, pins read HIGH if not driven LOW externally
- This is why keyboard scanning works: pressing a key connects row to column, pulling line LOW

**Example - Keyboard Matrix:**
```
Port A (rows): OUTPUT, driven LOW to scan
Port B (columns): INPUT, pulled HIGH by resistors

When key pressed:
- Row connects to column
- Column pulled LOW through key switch
- Reading Port B shows 0 bit where key is pressed
```

### Capacitance

| Parameter | Typical | Max | Unit |
|-----------|---------|-----|------|
| **Input Capacitance (CIN)** | 7pF | 10pF | pF |
| **Output Capacitance (COUT)** | 7pF | 10pF | pF |

**Impact:**
- Low capacitance = fast signal transitions
- Can drive moderately long PCB traces
- Additional capacitance from cables/connectors affects signal quality

### Protection Features

**All inputs contain protection circuitry against:**
- Electrostatic discharge (ESD)
- High voltage transients
- Reverse polarity (within limits)

**Caution:** Voltages exceeding absolute maximum ratings may still cause permanent damage. Use proper handling procedures (anti-static wrist strap, grounded work surface).

---

## Interface Signals

The 6526 CIA communicates with the system through several critical interface signals. Understanding these signals is essential for proper timing and integration with the 6510 CPU.

### φ2 — Clock Input

**Pin:** 25
**Type:** TTL-compatible input
**Function:** System clock and timing reference

The φ2 clock is used for:
- Internal CIA operation (counters, timers, registers)
- Timing reference for data bus communication
- Synchronizing all register reads/writes

**In the C64:**
- PAL systems: 985,248 Hz (0.985 MHz)
- NTSC systems: 1,022,727 Hz (1.023 MHz)

**Key Points:**
- All CIA operations are synchronized to φ2
- Register access must occur when φ2 is HIGH
- Timer counters decrement on φ2 rising edge

```
φ2 Timing:
    ┌───┐   ┌───┐   ┌───┐
    │   │   │   │   │   │
────┘   └───┘   └───┘   └────
    ↑       ↑       ↑
    Register access valid here
```

---

### CS — Chip Select Input

**Pin:** 23
**Type:** Active low
**Function:** Enables CIA for register access

**Operation:**
- **CS LOW + φ2 HIGH:** CIA responds to R/W and address lines
- **CS HIGH:** CIA ignores all control signals

**In the C64:**
- CIA #1: Activated when address is $DC00-$DC0F
- CIA #2: Activated when address is $DD00-$DD0F
- Address decoder generates CS from address bus

**Timing:**
- Must be LOW during φ2 HIGH phase
- Typical: CS activated by φ2 rising edge

```assembly
; In C64, CS is automatic - just use the address
LDA $DC00       ; CS for CIA #1 automatically activated
LDA $DD00       ; CS for CIA #2 automatically activated
```

---

### R/W — Read/Write Input

**Pin:** 22
**Type:** TTL input
**Function:** Controls data transfer direction

**States:**
- **R/W = 1 (HIGH):** Read from CIA (data out)
- **R/W = 0 (LOW):** Write to CIA (data in)

**Operation:**
```
READ (R/W = 1):
  CS LOW + φ2 HIGH + R/W HIGH
  → CIA drives data bus with register contents
  → CPU reads data

WRITE (R/W = 0):
  CS LOW + φ2 HIGH + R/W LOW
  → CPU drives data bus
  → CIA latches data into register
```

**In the C64:**
- CPU automatically controls R/W during LDA (read) and STA (write)
- No special setup needed in assembly code

---

### RS3-RS0 — Address Inputs

**Pins:** 35-38
**Type:** TTL inputs
**Function:** Select internal register (16 registers total)

| RS3 | RS2 | RS1 | RS0 | Register | Offset |
|-----|-----|-----|-----|----------|--------|
| 0 | 0 | 0 | 0 | Port A Data (PRA) | +$00 |
| 0 | 0 | 0 | 1 | Port B Data (PRB) | +$01 |
| 0 | 0 | 1 | 0 | Data Direction A (DDRA) | +$02 |
| 0 | 0 | 1 | 1 | Data Direction B (DDRB) | +$03 |
| 0 | 1 | 0 | 0 | Timer A Low | +$04 |
| 0 | 1 | 0 | 1 | Timer A High | +$05 |
| 0 | 1 | 1 | 0 | Timer B Low | +$06 |
| 0 | 1 | 1 | 1 | Timer B High | +$07 |
| 1 | 0 | 0 | 0 | TOD 10ths | +$08 |
| 1 | 0 | 0 | 1 | TOD Seconds | +$09 |
| 1 | 0 | 1 | 0 | TOD Minutes | +$0A |
| 1 | 0 | 1 | 1 | TOD Hours | +$0B |
| 1 | 1 | 0 | 0 | Serial Data (SDR) | +$0C |
| 1 | 1 | 0 | 1 | Interrupt Control (ICR) | +$0D |
| 1 | 1 | 1 | 0 | Control Register A (CRA) | +$0E |
| 1 | 1 | 1 | 1 | Control Register B (CRB) | +$0F |

**In the C64:**
- Lower 4 bits of address bus connect to RS0-RS3
- Example: Address $DC05 → RS = %0101 → Timer A High

---

### DB7-DB0 — Data Bus Inputs/Outputs

**Pins:** 26-33
**Type:** Bidirectional, three-state
**Function:** 8-bit data transfer

**States:**
1. **High Impedance (default):**
   - CS HIGH or φ2 LOW or R/W LOW
   - Data bus floating (doesn't affect system bus)

2. **Output (read mode):**
   - CS LOW + φ2 HIGH + R/W HIGH
   - CIA drives data onto bus
   - CPU reads register value

3. **Input (write mode):**
   - CS LOW + φ2 HIGH + R/W LOW
   - CPU drives data onto bus
   - CIA latches data into register

**Key Points:**
- Multiple devices can share data bus (three-state design)
- Only one device drives bus at a time
- CIA automatically manages output enable

---

### IRQ — Interrupt Request Output

**Pin:** 21
**Type:** Open drain, active low
**Function:** Signals interrupt to CPU

**Electrical Characteristics:**
- **Open drain:** Can only pull LOW, not drive HIGH
- **External pull-up:** 3.3kΩ resistor holds line HIGH
- **Wired-OR:** Multiple IRQ sources can share line

**Operation:**
```
Normal state: HIGH (pulled up by resistor)
Interrupt occurs: LOW (CIA pulls down)
```

**In the C64:**
- **CIA #1 IRQ** → CPU IRQ pin (maskable with SEI/CLI)
- **CIA #2 IRQ** → CPU NMI pin (non-maskable)
- VIC-II also shares IRQ line with CIA #1

**Shared IRQ Behavior:**
```
       +5V
        │
       3.3kΩ (pull-up resistor)
        │
   ─────┴───────┬───────────→ To CPU IRQ pin
                │
         ┌──────┴──────┐
         │             │
      [CIA #1]     [VIC-II]
      IRQ out      IRQ out
    (open drain) (open drain)

When either CIA #1 OR VIC-II interrupts:
  - Line pulled LOW
  - CPU enters IRQ handler
  - Handler must read both ICR registers to determine source
```

**IRQ Handler Pattern:**
```assembly
IRQ_HANDLER:
    ; Check CIA #1
    LDA $DC0D       ; Read ICR (clears flags)
    BNE CIA_INT     ; Bit 7=1: CIA interrupt

    ; Check VIC-II
    LDA $D019       ; VIC-II interrupt register
    BNE VIC_INT     ; VIC-II interrupt

    ; Not ours - exit
    JMP $EA31       ; Jump to KERNAL IRQ handler

CIA_INT:
    ; Handle CIA #1 interrupt
    ; (ICR already read above, flags cleared)
    RTI

VIC_INT:
    ; Handle VIC-II interrupt
    STA $D019       ; Acknowledge VIC-II
    RTI
```

---

### RES — Reset Input

**Pin:** 34
**Type:** Active low
**Function:** Resets all CIA internal registers

**Reset Behavior:**

When RES is pulled LOW:
1. **Port registers (PRA/PRB):** Set to $00
2. **Data direction (DDRA/DDRB):** Set to $00 (all inputs)
3. **Timer latches:** Set to $FFFF (all ones)
4. **Control registers (CRA/CRB):** Set to $00 (timers stopped)
5. **Interrupt control (ICR):** Set to $00 (all interrupts disabled)
6. **TOD registers:** Set to $00
7. **Serial data (SDR):** Set to $00

**Important:** Although port registers are set to $00, reading the ports returns $FF due to internal pull-up resistors.

**In the C64:**
- RES connected to system reset line
- Pressing RESTORE key generates NMI (not reset!)
- Power-on or RESET button activates RES

**After Reset:**
```assembly
; Typical post-reset state
; Port A/B: all inputs (DDR = $00)
LDA $DC00       ; Returns $FF (pull-ups active)
LDA $DC01       ; Returns $FF

; Timers: stopped, latches = $FFFF
LDA $DC0E       ; CRA = $00 (START bit clear)
LDA $DC04       ; Timer value undefined until written

; Interrupts: all disabled
LDA $DC0D       ; ICR = $00 (no interrupts enabled)
```

**Reset Timing:**
- RES must be LOW for minimum 10 clock cycles
- All registers cleared synchronously with φ2
- Reset completes on rising edge of φ2 after RES goes HIGH

---

## Timing Characteristics

Understanding CIA timing is critical for cycle-accurate programming, debugging hardware issues, and ensuring reliable operation.

### Clock Timing

**φ2 Clock Requirements:**

| Parameter | 1 MHz System | 2 MHz System | Unit |
|-----------|--------------|--------------|------|
| **Cycle Time (TCYC)** | 1000-20,000 | 500-20,000 | ns |
| **Rise/Fall Time (TR/TF)** | ≤25 | ≤25 | ns |
| **Clock HIGH Width (TCHW)** | 420-10,000 | 200-10,000 | ns |
| **Clock LOW Width (TCLW)** | 420-10,000 | 200-10,000 | ns |

**C64 Actual Timing:**
- PAL: 1.015 µs cycle time (985,248 Hz)
- NTSC: 0.978 µs cycle time (1,022,727 Hz)

**Why This Matters:**
- Minimum clock HIGH time: 420 ns (42% duty cycle)
- Maximum clock frequency: 2 MHz (500 ns cycle)
- C64 operates well within specs (~1 MHz)

---

### Write Cycle Timing

**CPU writes to CIA register (STA instruction):**

| Parameter | Description | 1 MHz | 2 MHz | Unit |
|-----------|-------------|-------|-------|------|
| **TWCS** | CS low while φ2 high | 420 min | 200 min | ns |
| **TADS** | Address setup time | 0 min | 0 min | ns |
| **TADH** | Address hold time | 10 min | 5 min | ns |
| **TRWS** | R/W setup time | 0 min | 0 min | ns |
| **TRWH** | R/W hold time | 0 min | 0 min | ns |
| **TDS** | Data bus setup time | 150 min | 75 min | ns |
| **TDH** | Data bus hold time | 0 min | 0 min | ns |
| **TPD** | Output delay from φ2 | ≤1000 | ≤500 | ns |

**Write Cycle Sequence:**

```
φ2:     ────┐           ┌────
            │           │
            └───────────┘
            ↑     ↑     ↑
            │     │     └─ φ2 falls: CIA latches data
            │     └─ Data valid (TDS before φ2 falls)
            └─ CS, R/W, Address valid

Timing:
  1. Address, CS, R/W setup: before or with φ2 rising
  2. Data setup: 150ns before φ2 falling edge
  3. CIA samples data on φ2 falling edge
  4. Data hold: 0ns (can change immediately after)
```

**Critical Timing Point:**
- **Data setup time (TDS):** Data must be stable 150 ns before φ2 falls
- This ensures CIA correctly latches the written value

**Example Write Operation:**
```assembly
STA $DC00       ; Write to Port A

; Timing (C64 @ 1 MHz):
; Cycle 1: CPU outputs address $DC00
; Cycle 2: φ2 high, CS active, R/W low, data valid
;          CIA latches data on φ2 falling edge
; Cycle 3: CPU continues to next instruction
```

---

### Read Cycle Timing

**CPU reads from CIA register (LDA instruction):**

| Parameter | Description | 1 MHz | 2 MHz | Unit |
|-----------|-------------|-------|-------|------|
| **TWCS** | CS low while φ2 high | 420 min | 200 min | ns |
| **TADS** | Address setup time | 0 min | 0 min | ns |
| **TADH** | Address hold time | 10 min | 5 min | ns |
| **TRWS** | R/W setup time | 0 min | 0 min | ns |
| **TRWH** | R/W hold time | 0 min | 0 min | ns |
| **TPS** | Port setup time | 300 min | 150 min | ns |

**Read Cycle Sequence:**

```
φ2:     ────┐           ┌────
            │           │
            └───────────┘
            ↑     ↑     ↑
            │     │     └─ φ2 falls: CIA tristates data bus
            │     └─ Data valid from CIA (CPU reads here)
            └─ CS, R/W, Address valid

Timing:
  1. Address, CS, R/W setup: before or with φ2 rising
  2. CIA enables data bus outputs
  3. Data valid after TPD delay (≤1000ns)
  4. CPU reads data during φ2 HIGH
  5. CIA tristates bus when φ2 falls
```

**Port Setup Time (TPS):**
- **300 ns minimum** (for reading port pins)
- External signals must be stable 300 ns before CIA samples them
- Applies to Port A/B reads when pins configured as inputs

**Example Read Operation:**
```assembly
LDA $DC01       ; Read from Port B (keyboard columns)

; Timing (C64 @ 1 MHz):
; Cycle 1: CPU outputs address $DC01
; Cycle 2: φ2 high, CS active, R/W high
;          CIA drives data bus with PRB value
;          CPU reads data before φ2 falls
; Cycle 3: CIA tristates bus, CPU continues
```

---

### Timing Diagrams

#### Write Timing Diagram

```
           ┌─TADS─┐◄───TADH───►
Address: ──────────X═══════════════X────
                   └─Valid────────┘

           ┌─TRWS─┐◄───TRWH───►
R/W:     ──────────┐               ┌────
(LOW)              └───────────────┘

                   ┌─────TWCS─────┐
φ2:      ──────────┐               ┌────
                   └───────────────┘

           ◄────TDS────►┐◄─TDH─►
Data:    ─────────X═════════════X────
                  └Valid───────┘

CS:      ──────────┐               ┌────
(LOW)              └───────────────┘
                   ↑               ↑
                   │               └─ Data latched here
                   └─ φ2 rise: CIA selected
```

#### Read Timing Diagram

```
           ┌─TADS─┐◄───TADH───►
Address: ──────────X═══════════════X────
                   └─Valid────────┘

           ┌─TRWS─┐◄───TRWH───►
R/W:     ──────────┐               ┌────
(HIGH)             └───────────────┘

                   ┌─────TWCS─────┐
φ2:      ──────────┐               ┌────
                   └───────────────┘

                   ◄──TPD──►
Data:    ─────────────X═════════════X────
                      └Valid───────┘
                      ↑           ↑
                      │           └─ CPU reads here
                      └─ CIA drives bus

CS:      ──────────┐               ┌────
(LOW)              └───────────────┘
```

---

### Timing Considerations for Assembly Programming

#### 1. Reading Timers (Race Condition)

```assembly
; PROBLEM: Timer might underflow between reads
READ_TIMER_UNSAFE:
    LDA $DC04       ; Read low byte
    STA TEMP_LO
    ; >>> Timer underflows here! $00FF → $0000 → $FFFF (reload)
    LDA $DC05       ; Read high byte (now $FF instead of $00!)
    STA TEMP_HI
    ; Result: $FFXX instead of $00XX - totally wrong!

; SOLUTION: Read twice and verify consistency
READ_TIMER_SAFE:
    LDA $DC05       ; High byte first
    STA TEMP_HI
    LDA $DC04       ; Low byte
    STA TEMP_LO
    LDA $DC05       ; High byte again
    CMP TEMP_HI     ; Changed?
    BNE READ_TIMER_SAFE ; Yes: retry
    ; Now TEMP_HI:TEMP_LO is consistent
```

#### 2. Writing Timers (Latch Behavior)

```assembly
; Writing to timer registers doesn't immediately affect timer!
; Data goes to LATCH, loaded into timer when:
;   - LOAD bit forced (CRA/CRB bit 4)
;   - Timer starts (START bit 0→1)
;   - Timer underflows (continuous mode)

; Method 1: Force immediate load
    LDA #$E8
    STA $DC04       ; Latch low
    LDA #$03
    STA $DC05       ; Latch high
    LDA $DC0E
    ORA #%00010000  ; LOAD bit
    STA $DC0E       ; Loaded NOW (LOAD auto-clears)

; Method 2: Load on start
    LDA #$E8
    STA $DC04
    LDA #$03
    STA $DC05
    LDA #%00010001  ; START + LOAD
    STA $DC0E       ; Loaded when timer starts
```

#### 3. Reading ICR (Clears Flags)

```assembly
; CRITICAL: Reading ICR clears ALL interrupt flags
IRQ_HANDLER:
    LDA $DC0D       ; Read ICR (clears flags!)
    STA TEMP        ; MUST save value

    ; Check Timer A
    AND #%00000001
    BNE HANDLE_TIMER_A

    ; Check Timer B (use saved value!)
    LDA TEMP        ; Reload saved ICR
    AND #%00000010
    BNE HANDLE_TIMER_B

    ; WRONG: Reading ICR again
    ; LDA $DC0D     ; Returns $00! (flags already cleared)
    ; AND #%00000010
    ; BNE ...       ; Never branches!
```

#### 4. TOD Clock Latching

```assembly
; TOD reads are latched to prevent mid-read changes

; CORRECT read order:
    LDA $DC0B       ; Hours FIRST (latches all TOD registers)
    STA HOUR
    LDA $DC0A       ; Minutes (latched value)
    STA MINUTE
    LDA $DC09       ; Seconds (latched value)
    STA SECOND
    LDA $DC08       ; Tenths LAST (unlocks latch)
    STA TENTH

; WRONG read order:
    LDA $DC08       ; Tenths FIRST (unlocks latch immediately!)
    ; >>> Clock ticks here: 09:59:59.9 → 10:00:00.0
    LDA $DC09       ; Reads NEW seconds (00)
    LDA $DC0A       ; Reads NEW minutes (00)
    LDA $DC0B       ; Reads NEW hours (10)
    ; Result: 10:00:00.9 (invalid!)

; TOD writes also latched:
    LDA #$82        ; Hours FIRST (stops clock)
    STA $DC0B
    LDA #$30        ; Minutes (clock stopped)
    STA $DC0A
    LDA #$45        ; Seconds
    STA $DC09
    LDA #$05        ; Tenths LAST (starts clock)
    STA $DC08
```

---

### Hardware Integration Notes

**For hardware designers interfacing with the 6526:**

1. **Address Decoding:**
   - CS must be stable before φ2 rises
   - Typical: Use φ2 to latch address decoder output

2. **Data Bus:**
   - Add external pull-up/pull-down if needed
   - Ensure no bus contention with other devices
   - CIA three-states bus when not selected

3. **IRQ Line:**
   - Requires external 3.3kΩ pull-up to +5V
   - Open drain allows multiple interrupt sources
   - Can be shared with other chips (VIC-II, etc.)

4. **Clock Source:**
   - Must be stable, TTL-compatible square wave
   - 50% duty cycle recommended (42% minimum)
   - Can operate 1 MHz (C64) up to 2 MHz (max spec)

5. **Reset:**
   - Hold RES LOW for minimum 10 φ2 cycles
   - Ensure RES timing during power-on
   - Schmitt trigger recommended for clean reset

---

## Register Map

The 6526 CIA contains 16 internal registers accessed through a 4-bit address (RS0-RS3). Each register is one byte (8 bits).

### Complete Register Table

| Offset | Hex | Register Name | Abbreviation | Read/Write | Function |
|--------|-----|---------------|--------------|------------|----------|
| **0** | $00 | Port A Data | PRA | R/W | Peripheral Data Register A |
| **1** | $01 | Port B Data | PRB | R/W | Peripheral Data Register B |
| **2** | $02 | Data Direction A | DDRA | W | Data Direction Register A |
| **3** | $03 | Data Direction B | DDRB | W | Data Direction Register B |
| **4** | $04 | Timer A Low | TA LO | R/W | Timer A Low Byte |
| **5** | $05 | Timer A High | TA HI | R/W | Timer A High Byte |
| **6** | $06 | Timer B Low | TB LO | R/W | Timer B Low Byte |
| **7** | $07 | Timer B High | TB HI | R/W | Timer B High Byte |
| **8** | $08 | TOD 10ths | TOD 10THS | R/W | Time of Day Clock: 1/10 Seconds |
| **9** | $09 | TOD Seconds | TOD SEC | R/W | Time of Day Clock: Seconds |
| **10** | $0A | TOD Minutes | TOD MIN | R/W | Time of Day Clock: Minutes |
| **11** | $0B | TOD Hours | TOD HR | R/W | Time of Day Clock: Hours + AM/PM |
| **12** | $0C | Serial Data | SDR | R/W | Serial Data Register |
| **13** | $0D | Interrupt Control | ICR | R/W* | Interrupt Control and Status |
| **14** | $0E | Control Register A | CRA | R/W | Timer A Control Register |
| **15** | $0F | Control Register B | CRB | R/W | Timer B Control Register |

**Notes:**
- R/W* for ICR means read behavior differs from write behavior
- All registers are memory-mapped at base address $DC00 (CIA #1) or $DD00 (CIA #2)
- Reading unimplemented bits returns undefined values (typically 0 or 1 randomly)

### Register Access in Assembly

**CIA #1 Examples:**
```assembly
; Reading Port A (keyboard rows, joystick 2)
LDA $DC00       ; PRA - read current port state

; Writing to Port A (set keyboard rows)
LDA #$FE        ; Activate row 0
STA $DC00       ; PRA - output to port

; Configure data direction
LDA #$00        ; All bits as inputs
STA $DC02       ; DDRA - configure port A direction
```

**CIA #2 Examples:**
```assembly
; Switch VIC-II bank via Port A
LDA $DD00       ; PRA - read current state
AND #%11111100  ; Clear bits 0-1
ORA #%00000011  ; Set bank 0 (bits inverted)
STA $DD00       ; PRA - write new bank

; Configure CIA #2 Port A for VIC banking
LDA #%00000011  ; Bits 0-1 as outputs, rest as inputs
STA $DD02       ; DDRA - configure direction
```

### Memory Map Overview

**CIA #1 ($DC00-$DC0F) - Decimal 56320-56335:**
```
$DC00  56320  PRA    - Port A Data (keyboard rows, joystick 2)
$DC01  56321  PRB    - Port B Data (keyboard columns, joystick 1)
$DC02  56322  DDRA   - Port A Data Direction
$DC03  56323  DDRB   - Port B Data Direction
$DC04  56324  TA LO  - Timer A Low Byte
$DC05  56325  TA HI  - Timer A High Byte
$DC06  56326  TB LO  - Timer B Low Byte
$DC07  56327  TB HI  - Timer B High Byte
$DC08  56328  TOD 10THS - Time of Day: Tenths of Seconds
$DC09  56329  TOD SEC   - Time of Day: Seconds
$DC0A  56330  TOD MIN   - Time of Day: Minutes
$DC0B  56331  TOD HR    - Time of Day: Hours (+ AM/PM flag)
$DC0C  56332  SDR    - Serial Data Register
$DC0D  56333  ICR    - Interrupt Control Register
$DC0E  56334  CRA    - Control Register A (Timer A)
$DC0F  56335  CRB    - Control Register B (Timer B)
```

**CIA #2 ($DD00-$DD0F) - Decimal 56576-56591:**
```
$DD00  56576  PRA    - Port A Data (serial bus, VIC bank, RS-232)
$DD01  56577  PRB    - Port B Data (User Port, RS-232)
$DD02  56578  DDRA   - Port A Data Direction
$DD03  56579  DDRB   - Port B Data Direction
$DD04  56580  TA LO  - Timer A Low Byte
$DD05  56581  TA HI  - Timer A High Byte
$DD06  56582  TB LO  - Timer B Low Byte
$DD07  56583  TB HI  - Timer B High Byte
$DD08  56584  TOD 10THS - (usually unused)
$DD09  56585  TOD SEC   - (usually unused)
$DD0A  56586  TOD MIN   - (usually unused)
$DD0B  56587  TOD HR    - (usually unused)
$DD0C  56588  SDR    - Serial Data Register
$DD0D  56589  ICR    - Interrupt Control Register (generates NMI)
$DD0E  56590  CRA    - Control Register A (Timer A)
$DD0F  56591  CRB    - Control Register B (Timer B)
```

---

## Register Descriptions

Complete technical reference for all 16 CIA registers.

---

### Register 0: Port A Data Register (PRA)

**Address:** $DC00 (CIA #1), $DD00 (CIA #2)
**Access:** Read/Write
**Function:** 8-bit parallel I/O port data register

```
Bit:  7   6   5   4   3   2   1   0
      PA7 PA6 PA5 PA4 PA3 PA2 PA1 PA0
```

#### How It Works

Each bit corresponds to one pin of Port A (PA0-PA7). The behavior depends on the Data Direction Register (DDRA):

| DDRA Bit | PRA Write | PRA Read | Pin Behavior |
|----------|-----------|----------|--------------|
| **0** (Input) | Ignored | Reads pin state | High-Z input with pull-up |
| **1** (Output) | Drives pin | Reads output latch | Driven high or low |

#### CIA #1 Port A Usage (C64 Specific)

**Keyboard Matrix - Row Selection:**
```assembly
; Scan keyboard row 0
LDA #$00        ; Set all Port A bits as outputs
STA $DC02       ; DDRA
LDA #$FE        ; Activate row 0 (bit 0 = 0, rest = 1)
STA $DC00       ; PRA - drive row 0 low
LDA $DC01       ; PRB - read columns (see which keys pressed)
```

**Keyboard Matrix Bit Assignment:**
| Bit | Function |
|-----|----------|
| 0 | Row 0 (keys: DEL, RETURN, £, etc.) |
| 1 | Row 1 (keys: 3, W, A, 4, Z, S, E) |
| 2 | Row 2 (keys: 5, R, D, 6, C, F, T, X) |
| 3 | Row 3 (keys: 7, Y, G, 8, B, H, U, V) |
| 4 | Row 4 (keys: 9, I, J, 0, M, K, O, N) |
| 5 | Row 5 (keys: +, P, L, -, ., :, @, ,) |
| 6 | Row 6 (keys: ←, *, ;, HOME, RShift, =, ↑, /) |
| 7 | Row 7 (keys: 1, 2, CTRL, SPACE, C=, Q, Run/Stop) |

**Joystick 2:**
| Bit | Joystick Function |
|-----|-------------------|
| 0 | Up (0 = pressed) |
| 1 | Down |
| 2 | Left |
| 3 | Right |
| 4 | Fire button |

#### CIA #2 Port A Usage (C64 Specific)

**VIC-II Bank Selection:**
| Bit | Function |
|-----|----------|
| 0 | VIC bank bit 0 (inverted) |
| 1 | VIC bank bit 1 (inverted) |

```assembly
; VIC Bank Selection Example
; Bits 0-1 are INVERTED: 11 = bank 0, 00 = bank 3
LDA #%00000011  ; Set bits 0-1 as outputs
STA $DD02       ; DDRA

LDA #%00000011  ; Both bits HIGH = VIC bank 0 ($0000-$3FFF)
STA $DD00       ; PRA

LDA #%00000010  ; Bits = 10 = VIC bank 1 ($4000-$7FFF)
STA $DD00

LDA #%00000001  ; Bits = 01 = VIC bank 2 ($8000-$BFFF)
STA $DD00

LDA #%00000000  ; Bits = 00 = VIC bank 3 ($C000-$FFFF)
STA $DD00
```

**Serial Bus Control:**
| Bit | Function |
|-----|----------|
| 2 | ATN OUT (serial bus attention) |
| 3 | CLK OUT (serial clock output) |
| 4 | DATA OUT (serial data output) |
| 5 | DATA IN (serial data input) |
| 6 | CLK IN (serial clock input) |
| 7 | ATN IN (serial attention input) |

#### Pull-Up Behavior

**Important:** All Port A pins have internal pull-up resistors (~5kΩ typical).

```
When configured as INPUT (DDRA bit = 0):
  - Pin reads HIGH if nothing connected or if external device outputs HIGH
  - Pin reads LOW only if external device actively drives it LOW

When configured as OUTPUT (DDRA bit = 1):
  - Pin can drive HIGH (sources current) or LOW (sinks current)
  - Pull-up disabled when output drives LOW
```

#### Common Mistakes

**Mistake #1: Forgetting to set data direction**
```assembly
; WRONG - Won't work!
LDA #$FE
STA $DC00       ; Tries to write without setting DDRA

; CORRECT
LDA #$FF        ; All bits as outputs
STA $DC02       ; Set DDRA first
LDA #$FE
STA $DC00       ; Now write works
```

**Mistake #2: Reading output pins**
```assembly
; When Port A configured as OUTPUT:
LDA #$FF
STA $DC02       ; All outputs
LDA #$55
STA $DC00       ; Write $55
LDA $DC00       ; Reads output LATCH, not external pins
                ; Returns $55, not what's actually on pins
```

---

### Register 1: Port B Data Register (PRB)

**Address:** $DC01 (CIA #1), $DD01 (CIA #2)
**Access:** Read/Write
**Function:** 8-bit parallel I/O port data register

```
Bit:  7   6   5   4   3   2   1   0
      PB7 PB6 PB5 PB4 PB3 PB2 PB1 PB0
```

#### How It Works

Identical to Port A, but with different pin assignments. Controlled by DDRB ($DC03/$DD03).

#### CIA #1 Port B Usage (C64 Specific)

**Keyboard Matrix - Column Reading:**
```assembly
; After activating a row via Port A, read columns via Port B
LDA #$00        ; Set Port A as outputs (drive rows)
STA $DC02       ; DDRA
LDA #$FF        ; Set Port B as inputs (read columns)
STA $DC03       ; DDRB

LDA #$FE        ; Activate row 0
STA $DC00       ; PRA
LDA $DC01       ; PRB - read which columns are LOW = key pressed
```

**Keyboard Matrix Column Assignment:**
| Bit | Column Function |
|-----|-----------------|
| 0 | Column 0 (leftmost keys in row) |
| 1 | Column 1 |
| 2 | Column 2 |
| 3 | Column 3 |
| 4 | Column 4 |
| 5 | Column 5 |
| 6 | Column 6 |
| 7 | Column 7 (rightmost keys in row) |

**Joystick 1:**
| Bit | Joystick Function |
|-----|-------------------|
| 0 | Up (0 = pressed) |
| 1 | Down |
| 2 | Left |
| 3 | Right |
| 4 | Fire button |

**Paddle Controllers:**
| Bit | Function |
|-----|----------|
| 6 | Paddle 1 fire button (0 = pressed) |
| 7 | Paddle 2 fire button (0 = pressed) |

**Timer Toggle Output:**
| Bit | Function |
|-----|----------|
| 6 | Timer A toggle output (if CRA bit 2 = 1) |
| 7 | Timer B toggle output (if CRB bit 2 = 1) |

#### CIA #2 Port B Usage (C64 Specific)

**User Port Parallel I/O:**
- All 8 bits available for user programs
- Connected to User Port edge connector
- Can be configured as inputs or outputs per-bit
- Commonly used for:
  - Custom hardware interfaces
  - Parallel printer ports
  - MIDI interfaces
  - Network adapters
  - Game controller adapters

**RS-232 Serial Communication:**
| Bit | Function |
|-----|----------|
| 0-7 | RS-232 data lines (software-controlled) |

#### Reading Joysticks

**Complete joystick reading example:**
```assembly
; Configure for joystick reading
LDA #$00        ; Port A = outputs (keyboard rows)
STA $DC02       ; DDRA
LDA #$FF        ; Port B = inputs (keyboard columns, joy 1)
STA $DC03       ; DDRB

; Read Joystick 1 (Port B)
LDA $DC01       ; PRB
AND #%00011111  ; Mask bits 0-4 (joystick)
EOR #%00011111  ; Invert (0=pressed becomes 1=pressed)
; Now: bit 0=Up, 1=Down, 2=Left, 3=Right, 4=Fire

; Read Joystick 2 (Port A)
LDA $DC00       ; PRA
AND #%00011111  ; Mask bits 0-4
EOR #%00011111  ; Invert
```

#### Timer Output on Port B

**Special feature:** Timer A and Timer B can output pulses on Port B bits 6 and 7.

```assembly
; Configure Timer A to toggle PB6
LDA #%01000000  ; Bit 6 as output
STA $DC03       ; DDRB

; Set Timer A to toggle mode
LDA $DC0E       ; CRA
ORA #%00000100  ; Set bit 2 (PB6 toggle mode)
STA $DC0E       ; CRA

; Now Timer A underflows toggle PB6 HIGH/LOW
; Creates square wave output
```

---

### Registers 2-3: Data Direction Registers (DDRA/DDRB)

**Addresses:** $DC02/$DC03 (CIA #1), $DD02/$DD03 (CIA #2)
**Access:** Write only (read returns undefined data)
**Function:** Configure each port bit as input or output

```
Bit:  7   6   5   4   3   2   1   0
      D7  D6  D5  D4  D3  D2  D1  D0

For each bit:
  0 = Input (high-impedance with pull-up)
  1 = Output (driven HIGH or LOW)
```

#### How Data Direction Works

Each bit in the DDR controls the corresponding bit in the port:

```
DDRA/DDRB Bit = 0 (INPUT):
  ┌─────┐
  │ Pin │──→ Input buffer ──→ PRA/PRB (read operation)
  └─────┘
     ↑
  Pull-up resistor (~5kΩ to +5V)

DDRA/DDRB Bit = 1 (OUTPUT):
  ┌─────┐
  │ Pin │←── Output driver ←── PRA/PRB (write operation)
  └─────┘
```

#### Configuration Examples

**Example 1: All inputs**
```assembly
LDA #$00        ; All bits = 0 = all inputs
STA $DC02       ; DDRA
; Port A pins are now high-Z inputs with pull-ups
; Reading $DC00 returns external pin states
```

**Example 2: All outputs**
```assembly
LDA #$FF        ; All bits = 1 = all outputs
STA $DC02       ; DDRA
; Port A pins now driven by output latch
; Writing $DC00 drives pins HIGH or LOW
```

**Example 3: Mixed I/O**
```assembly
LDA #%00001111  ; Bits 0-3 outputs, 4-7 inputs
STA $DC02       ; DDRA
; Lower nibble: outputs (can write)
; Upper nibble: inputs (can read)
```

#### C64 Standard Configuration

**CIA #1 (Keyboard/Joystick):**
```assembly
; Standard keyboard scanning setup
LDA #$00        ; Port A = outputs (drive keyboard rows)
STA $DC02       ; DDRA
LDA #$FF        ; Port B = inputs (read keyboard columns)
STA $DC03       ; DDRB
```

**CIA #2 (Serial Bus/VIC Banking):**
```assembly
; Standard system setup
LDA #%00111111  ; Bits 0-5 outputs (serial/VIC), 6-7 inputs
STA $DD02       ; DDRA
LDA #$00        ; Port B = inputs (User Port default)
STA $DD03       ; DDRB
```

#### Important Notes

1. **DDR is write-only** - Reading DDR registers returns unpredictable data (don't rely on read values)

2. **Reset state** - On power-up or reset, DDRs default to $00 (all inputs)

3. **Shadow registers** - The CIA internally maintains the DDR values even though you can't read them back

4. **Output conflicts** - If you configure a pin as output but external hardware also drives it, contention occurs (can damage hardware)

#### Common Pattern: Read-Modify-Write Port Config

```assembly
; Want to change only bit 2 to output, keep rest as-is
; PROBLEM: Can't read DDR to see current config!

; SOLUTION: Maintain DDR shadow copy in RAM
DDR_SHADOW:
    .BYTE $00   ; Keep track of DDRA configuration

CHANGE_BIT2:
    LDA DDR_SHADOW  ; Load current config from RAM
    ORA #%00000100  ; Set bit 2
    STA DDR_SHADOW  ; Update shadow
    STA $DC02       ; Write to DDRA
    RTS
```

---

### Registers 4-5: Timer A Registers (TA LO/TA HI)

**Addresses:** $DC04/$DC05 (CIA #1), $DD04/$DD05 (CIA #2)
**Access:** Read/Write
**Function:** 16-bit down counter for precise timing

```
$DC04/DD04: TA LO  - Timer A Low Byte  (bits 0-7)
$DC05/DD05: TA HI  - Timer A High Byte (bits 8-15)

16-bit value: %HHHHHHHH LLLLLLLL
Range: $0001-$FFFF (1 to 65535)
```

#### How Timer A Works

Timer A is a **16-bit down counter** that decrements on each clock pulse:

1. **Load:** Write 16-bit value to TA LO/HI (or latch registers)
2. **Start:** Set START bit in Control Register A (CRA)
3. **Count:** Timer decrements each clock cycle (or external event)
4. **Underflow:** When timer reaches $0000, it:
   - Generates interrupt (if enabled)
   - Reloads from latch (continuous mode) OR stops (one-shot mode)
   - Optionally toggles PB6 output

#### Clock Sources

Timer A can count two different sources (selected by CRA bit 5):

| CRA Bit 5 | Source | Frequency | Use Case |
|-----------|--------|-----------|----------|
| **0** | φ2 (system clock) | ~1 MHz | Standard timing, music, delays |
| **1** | CNT pin (external) | Variable | Counting external events |

**φ2 Clock Timing:**
- PAL C64: 985,248 Hz (~0.985 MHz)
- NTSC C64: 1,022,727 Hz (~1.023 MHz)

#### Reading Timer A

```assembly
; Read current timer value (16-bit)
LDA $DC04       ; Read low byte first
STA TEMP_LO
LDA $DC05       ; Read high byte
STA TEMP_HI
; Timer value now in TEMP_HI:TEMP_LO
```

**Warning:** Timer continues counting while you read it!
```assembly
; PROBLEM: Timer might underflow between reads
LDA $DC04       ; Low byte = $FF
; >>> Timer underflows here: $00FF → $0000 → reload to $1000
LDA $DC05       ; High byte = $10 (from reload, not $00!)
; Result: $10FF instead of $00FF - reading error!

; SOLUTION: Read twice and check consistency
READ_LOOP:
    LDA $DC05       ; High byte first
    STA TEMP_HI
    LDA $DC04       ; Low byte
    STA TEMP_LO
    LDA $DC05       ; Read high again
    CMP TEMP_HI     ; Did it change?
    BNE READ_LOOP   ; Yes: retry
; Now TEMP_HI:TEMP_LO is consistent
```

#### Writing Timer A

```assembly
; Set timer to specific value (e.g., 1000 = $03E8)
LDA #$E8        ; Low byte
STA $DC04       ; TA LO
LDA #$03        ; High byte
STA $DC05       ; TA HI

; Timer doesn't start until you set CRA START bit!
LDA $DC0E       ; Read Control Register A
ORA #%00000001  ; Set bit 0 (START)
STA $DC0E       ; Timer now running
```

#### Timer Latch Registers

**Important:** The CIA has **separate latch registers** for loading the timer.

```
Write to TA LO/HI:
  → Data goes to LATCH registers (not timer directly)

Timer loads from latch when:
  1. LOAD bit (CRA bit 4) forced to 1
  2. Timer underflows (in continuous mode)
  3. Timer starts (CRA bit 0: 0→1 transition)
```

**Force load example:**
```assembly
; Set up timer value
LDA #$E8
STA $DC04       ; Latch low = $E8
LDA #$03
STA $DC05       ; Latch high = $03

; Force immediate load into timer
LDA $DC0E       ; CRA
ORA #%00010001  ; Set LOAD (bit 4) and START (bit 0)
STA $DC0E       ; Timer loaded and started
```

#### Common Timer A Values

**For 50 Hz (PAL systems):**
```assembly
; 985248 Hz / 50 Hz = 19704.96 ≈ 19705 cycles = $4CF9
LDA #$F9
STA $DC04       ; TA LO
LDA #$4C
STA $DC05       ; TA HI
```

**For 60 Hz (NTSC systems):**
```assembly
; 1022727 Hz / 60 Hz = 17045.45 ≈ 17045 cycles = $4295
LDA #$95
STA $DC04       ; TA LO
LDA #$42
STA $DC05       ; TA HI
```

**For 1 millisecond:**
```assembly
; ~1000 cycles = $03E8
LDA #$E8
STA $DC04
LDA #$03
STA $DC05
```

**For music note timing (C-4, 261.63 Hz):**
```assembly
; 1022727 Hz / 261.63 Hz = 3908 cycles = $0F44
LDA #$44
STA $DC04
LDA #$0F
STA $DC05
```

#### One-Shot vs Continuous Mode

**One-shot mode (CRA bit 3 = 1):**
```assembly
LDA #$E8
STA $DC04
LDA #$03
STA $DC05
LDA #%00011001  ; START=1, LOAD=1, ONESHOT=1
STA $DC0E
; Timer counts $03E8 → $0000, generates interrupt, then STOPS
```

**Continuous mode (CRA bit 3 = 0):**
```assembly
LDA #$E8
STA $DC04
LDA #$03
STA $DC05
LDA #%00010001  ; START=1, LOAD=1, ONESHOT=0
STA $DC0E
; Timer counts $03E8 → $0000, reloads $03E8, repeats forever
```

---

### Registers 6-7: Timer B Registers (TB LO/TB HI)

**Addresses:** $DC06/$DC07 (CIA #1), $DD06/$DD07 (CIA #2)
**Access:** Read/Write
**Function:** 16-bit down counter (similar to Timer A with additional modes)

```
$DC06/DD06: TB LO  - Timer B Low Byte  (bits 0-7)
$DC07/DD07: TB HI  - Timer B High Byte (bits 8-15)

16-bit value: %HHHHHHHH LLLLLLLL
Range: $0001-$FFFF (1 to 65535)
```

#### How Timer B Differs from Timer A

Timer B operates identically to Timer A, but has **additional clock sources**:

| CRB Bits 6-5 | Clock Source | Use Case |
|--------------|--------------|----------|
| **00** | φ2 (system clock ~1 MHz) | Standard timing |
| **01** | CNT pin (external events) | Event counting |
| **10** | Timer A underflows | Extended 32-bit timing |
| **11** | Timer A underflows (CNT high) | Gated timing |

#### 32-Bit Timing (Timer A + B Linked)

**Most powerful feature:** Link Timer A and Timer B for extended timing.

```assembly
; Create 32-bit timer: Timer A (low) + Timer B (high)
; Maximum delay: $FFFFFFFF cycles = ~71 minutes at 1 MHz!

; Set Timer A for low 16 bits
LDA #$FF
STA $DC04       ; TA LO = $FF
STA $DC05       ; TA HI = $FF

; Set Timer B to count Timer A underflows
LDA #$FF
STA $DC06       ; TB LO = $FF
STA $DC07       ; TB HI = $FF

; Configure Timer B to count Timer A underflows
LDA #%01010001  ; START=1, LOAD=1, INMODE=%10 (Timer A underflow)
STA $DC0F       ; CRB

; Start Timer A (also starts cascade)
LDA #%00010001  ; START=1, LOAD=1
STA $DC0E       ; CRA

; Now have 32-bit timer: $FFFF:$FFFF = 4,294,967,295 cycles
; At 1 MHz: ~4295 seconds = ~71 minutes
```

#### Reading Timer B

Same considerations as Timer A:

```assembly
; Read current Timer B value
READ_TIMER_B:
    LDA $DC07       ; High byte first
    STA TEMP_HI
    LDA $DC06       ; Low byte
    STA TEMP_LO
    LDA $DC07       ; High byte again (check consistency)
    CMP TEMP_HI
    BNE READ_TIMER_B ; Retry if changed
; Timer B value in TEMP_HI:TEMP_LO
```

#### Writing Timer B

```assembly
; Set Timer B to 5000 cycles ($1388)
LDA #$88
STA $DC06       ; TB LO
LDA #$13
STA $DC07       ; TB HI

; Start Timer B in continuous mode, counting φ2
LDA #%00010001  ; START=1, LOAD=1, ONESHOT=0, INMODE=%00
STA $DC0F       ; CRB
```

#### Gated Timing Mode

**Special mode:** Count Timer A underflows only when CNT pin is HIGH.

```assembly
; Timer B counts Timer A underflows, but only while CNT=HIGH
LDA #$FF
STA $DC04       ; TA LO
STA $DC05       ; TA HI
LDA #%00010001  ; Start Timer A
STA $DC0E       ; CRA

LDA #$10
STA $DC06       ; TB counts 16 Timer A underflows
LDA #$00
STA $DC07       ; TB HI

; CRB bits 6-5 = %11: Timer A underflows gated by CNT
LDA #%01110001  ; START=1, LOAD=1, INMODE=%11
STA $DC0F       ; CRB

; Timer B only counts when CNT pin is HIGH
; Used for measuring pulse widths, event durations
```

#### Timer B Toggle Output

Timer B can toggle PB7 (Port B bit 7):

```assembly
; Make Timer B toggle PB7 output
LDA #%10000000  ; Bit 7 = output
STA $DC03       ; DDRB

LDA #$E8        ; Timer value
STA $DC06
LDA #$03
STA $DC07

LDA $DC0F       ; CRB
ORA #%00000100  ; Set bit 2 (PB7 toggle enable)
STA $DC0F       ; Now Timer B underflows toggle PB7
```

#### Common Timer B Uses in C64

**1. Music/Sound Timing:**
```assembly
; Timer A: note frequency (SID register update rate)
; Timer B: note duration counter
```

**2. Long Delays:**
```assembly
; Timer B counts Timer A underflows
; Example: 1 second delay
; Timer A: 20000 cycles (50 times/second)
; Timer B: 50 counts (50 × 20000 = 1,000,000 = 1 second)
```

**3. Event Counting:**
```assembly
; Timer B counts external events via CNT pin
; Used for: disk drive RPM monitoring, tape loading
```

---

### Registers 8-11: Time-of-Day Clock (TOD)

**Addresses:**
- $DC08/$DD08: TOD 10THS (tenths of seconds)
- $DC09/$DD09: TOD SEC (seconds)
- $DC0A/$DD0A: TOD MIN (minutes)
- $DC0B/$DD0B: TOD HR (hours + AM/PM)

**Access:** Read/Write
**Function:** 24-hour clock with alarm capability

```
TOD 10THS ($DC08):  Bits 0-3 = tenths (0-9 BCD)
TOD SEC ($DC09):    Bits 0-6 = seconds (00-59 BCD)
TOD MIN ($DC0A):    Bits 0-6 = minutes (00-59 BCD)
TOD HR ($DC0B):     Bits 0-4 = hours (1-12 BCD), Bit 7 = AM/PM
```

#### TOD Clock Format (BCD)

**Binary Coded Decimal (BCD):** Each nibble represents a decimal digit.

```
Examples:
  $09 = 09 (not 9)
  $23 = 23 (not 35 decimal)
  $59 = 59 (not 89 decimal)

Tenths: $0-$9 (0.0 to 0.9 seconds)
Seconds: $00-$59 (00 to 59 seconds)
Minutes: $00-$59 (00 to 59 minutes)
Hours: $01-$12 (01 to 12, not 0-11 or 0-23!)
```

#### TOD Hours Register Format

```
Bit:  7   6   5   4   3   2   1   0
      PM  -   -   H8  H4  H2  H1  H0

Bit 7: AM/PM flag (0 = AM, 1 = PM)
Bits 4-0: Hours in BCD (1-12, not 0-11!)

Examples:
  $01 = 01:xx:xx AM (1 AM)
  $12 = 12:xx:xx AM (noon)
  $81 = 01:xx:xx PM (1 PM)
  $92 = 12:xx:xx PM (midnight)
```

**Common mistake:** Treating hours as 0-23 instead of 1-12 with AM/PM!

#### Reading TOD Clock

**Important:** Reading is **latched** to prevent time changing mid-read.

```assembly
; Reading TOD (proper sequence)
LDA $DC0B       ; Read hours FIRST (latches all registers)
STA HOUR        ; Save hours
LDA $DC0A       ; Read minutes (returns latched value)
STA MINUTE
LDA $DC09       ; Read seconds (returns latched value)
STA SECOND
LDA $DC08       ; Read tenths (UNLOCKS latch for next read)
STA TENTH

; Now HOUR:MINUTE:SECOND:TENTH is consistent snapshot
```

**Key sequence:**
1. Read HOURS first → **latches** TOD registers
2. Read MINUTES, SECONDS → returns latched values
3. Read TENTHS last → **unlocks** latch

**Wrong order causes problems:**
```assembly
; WRONG: Read tenths first
LDA $DC08       ; Unlocks latch (clock keeps running!)
STA TENTH
; >>> Clock ticks here: 09:59:59.9 → 10:00:00.0
LDA $DC09       ; Reads new seconds (00)
STA SECOND
LDA $DC0A       ; Reads new minutes (00)
STA MINUTE
LDA $DC0B       ; Reads new hours (10)
STA HOUR
; Result: 10:00:00.9 (invalid time!)
```

#### Writing TOD Clock

**Important:** Writing is also **latched** to prevent partial updates.

```assembly
; Setting TOD to 02:30:45.5 PM
LDA #$82        ; 02 PM (%10000010)
STA $DC0B       ; Write hours FIRST (stops TOD clock)

LDA #$30        ; 30 minutes (BCD)
STA $DC0A       ; Write minutes

LDA #$45        ; 45 seconds (BCD)
STA $DC09       ; Write seconds

LDA #$05        ; 5 tenths
STA $DC08       ; Write tenths LAST (starts TOD clock)

; TOD now running from 02:30:45.5 PM
```

**Key sequence:**
1. Write HOURS first → **stops clock**
2. Write MINUTES, SECONDS → clock still stopped
3. Write TENTHS last → **starts clock**

#### TOD Alarm Function

The CIA compares TOD clock with alarm registers. When they match, generates interrupt.

**Alarm vs Clock - CRB bit 7:**
```
CRB bit 7 = 0: TOD registers = CLOCK (normal operation)
CRB bit 7 = 1: TOD registers = ALARM (set alarm time)
```

**Setting an alarm:**
```assembly
; Set alarm for 03:00:00.0 PM

; Switch to ALARM mode
LDA $DC0F       ; CRB
ORA #%10000000  ; Set bit 7 (ALARM mode)
STA $DC0F

; Write alarm time (same format as clock)
LDA #$83        ; 03 PM
STA $DC0B       ; Alarm hours
LDA #$00        ; 00 minutes
STA $DC0A
LDA #$00        ; 00 seconds
STA $DC09
LDA #$00        ; 0 tenths
STA $DC08

; Switch back to CLOCK mode
LDA $DC0F       ; CRB
AND #%01111111  ; Clear bit 7 (CLOCK mode)
STA $DC0F

; Enable TOD alarm interrupt
LDA #%10000100  ; Set bit 7 (write), bit 2 (TOD alarm enable)
STA $DC0D       ; ICR
; When clock reaches 03:00:00.0 PM, interrupt fires
```

#### TOD Frequency Selection

TOD clock requires external 50 Hz or 60 Hz signal on TOD pin.

**CRA bit 7 selects divider:**
```
CRA bit 7 = 0: Divide by 60 (for 60 Hz input) - NTSC
CRA bit 7 = 1: Divide by 50 (for 50 Hz input) - PAL
```

**C64 configuration:**
```assembly
; PAL C64 (50 Hz)
LDA $DC0E       ; CRA
ORA #%10000000  ; Set bit 7 (50 Hz mode)
STA $DC0E

; NTSC C64 (60 Hz)
LDA $DC0E       ; CRA
AND #%01111111  ; Clear bit 7 (60 Hz mode)
STA $DC0E
```

**Default:** C64 KERNAL sets this correctly based on hardware revision.

#### Converting BCD to Binary

```assembly
; Convert BCD to binary (e.g., $23 → 23 decimal = $17)
LDA $DC09       ; Read seconds (BCD)
PHA             ; Save BCD value
AND #$0F        ; Isolate low nibble (ones)
STA TEMP
PLA             ; Restore BCD value
AND #$F0        ; Isolate high nibble (tens)
LSR             ; Divide by 16 to get tens count
LSR
LSR
LSR
STA TEMP2
ASL             ; Multiply by 10 (x2, then x5)
ASL
ASL
CLC
ADC TEMP2
ASL
CLC
ADC TEMP        ; Add ones
; A now contains binary value
```

#### Common TOD Uses

**1. Timestamps:**
```assembly
; Record current time for data logging
LDA $DC0B
STA TIMESTAMP_HR
LDA $DC0A
STA TIMESTAMP_MIN
LDA $DC09
STA TIMESTAMP_SEC
LDA $DC08
; Timestamp recorded
```

**2. Alarm-based events:**
```assembly
; Run routine at specific time daily
; Set alarm for 08:00:00.0 AM
; When interrupt fires, execute morning routine
```

**3. Real-time clock display:**
```assembly
; Update screen with current time
CLOCK_LOOP:
    LDA $DC0B       ; Latch and read hours
    JSR DISPLAY_HOURS
    LDA $DC0A
    JSR DISPLAY_MINUTES
    LDA $DC09
    JSR DISPLAY_SECONDS
    LDA $DC08       ; Unlatch
    JMP CLOCK_LOOP
```

---

### Register 12: Serial Data Register (SDR)

**Address:** $DC0C (CIA #1), $DD0C (CIA #2)
**Access:** Read/Write
**Function:** 8-bit shift register for serial communication

```
Bit:  7   6   5   4   3   2   1   0
      D7  D6  D5  D4  D3  D2  D1  D0

All 8 bits used for serial data (MSB or LSB first, configurable)
```

#### How the Serial Port Works

The Serial Data Register (SDR) converts between **parallel** and **serial** data:

```
Parallel → Serial (OUTPUT):
  1. Write 8-bit byte to SDR
  2. SDR shifts bits out via SP pin (one bit per clock)
  3. After 8 shifts, interrupt fires (if enabled)

Serial → Parallel (INPUT):
  1. Bits shift in via SP pin (one bit per clock)
  2. After 8 bits received, byte appears in SDR
  3. Interrupt fires (if enabled)
```

#### Serial Port Pins

| Pin | Name | Function |
|-----|------|----------|
| **SP** | Serial Port | Data line (bidirectional) |
| **CNT** | Counter | Clock signal (input or output) |

#### Clock Sources for Serial Port

**CRA bit 6 determines shift mode:**

| CRA Bit 6 | Mode | Clock Source | Use |
|-----------|------|--------------|-----|
| **0** | Input | External clock on CNT | Receive serial data |
| **1** | Output | Timer A underflow | Transmit serial data |

#### Transmitting Serial Data (Output Mode)

```assembly
; Setup: Use Timer A to generate serial clock
LDA #$08        ; Timer A = 8 cycles (fast serial)
STA $DC04       ; TA LO
LDA #$00
STA $DC05       ; TA HI

; Configure Timer A for serial output (CRA bit 6 = 1)
LDA #%01010001  ; START=1, LOAD=1, SPMODE=1 (output)
STA $DC0E       ; CRA

; Send byte via serial port
LDA #$55        ; Byte to send (%01010101)
STA $DC0C       ; SDR - starts shifting out on SP pin

; Wait for transmission complete (interrupt or polling)
WAIT_TX:
    LDA $DC0D       ; ICR
    AND #%00001000  ; Check bit 3 (serial port interrupt)
    BEQ WAIT_TX     ; Wait until set
; Transmission complete
```

**Bit order:** Configurable (MSB first or LSB first) - check CRA bit 7 on some CIA variants.

#### Receiving Serial Data (Input Mode)

```assembly
; Setup: External device provides clock on CNT pin
LDA #%00000000  ; SPMODE=0 (input mode), timer not started
STA $DC0E       ; CRA

; Enable serial port interrupt
LDA #%10001000  ; Set bit 7 (write), bit 3 (serial enable)
STA $DC0D       ; ICR

; Wait for byte to arrive
WAIT_RX:
    LDA $DC0D       ; ICR
    AND #%00001000  ; Check bit 3 (serial port interrupt)
    BEQ WAIT_RX     ; Wait until set

; Read received byte
LDA $DC0C       ; SDR - contains received 8-bit value
STA RECEIVED_BYTE
```

#### CIA #2 Serial Port - IEC Bus

**In the C64, CIA #2's serial port is used for the IEC serial bus (disk drives).**

**IEC bus uses bit-banged protocol (not SDR):**
- The KERNAL routines control IEC bus manually via Port A bits
- SDR itself is rarely used in C64 software
- Serial bus timing done by toggling pins, not using shift register

**CIA #2 Port A bits for IEC:**
| Bit | Signal | Function |
|-----|--------|----------|
| 2 | ATN | Attention (device selection) |
| 3 | CLK | Serial clock |
| 4 | DATA | Serial data |

**Why not use SDR for IEC?**
- IEC protocol requires precise handshaking
- Multiple devices on bus (needs addressing)
- Variable timing requirements
- More control needed than simple shift register provides

#### Custom Serial Protocols

**User Port applications can use SDR:**

```assembly
; Example: Simple SPI-like protocol on User Port
; Clock: PB7 (output)
; Data: SP pin (CIA serial port)

; Configure for output
LDA #%10000000  ; PB7 = output (manual clock)
STA $DD03       ; DDRB

; Send byte with manual clocking
LDA #$AA        ; Data to send
STA $DD0C       ; SDR

SHIFT_LOOP:
    ; Toggle clock
    LDA $DD01       ; PRB
    EOR #%10000000  ; Flip bit 7
    STA $DD01       ; Output clock pulse
    ; (shift happens automatically in SDR)
    ; Repeat 8 times for 8 bits
```

#### Serial Port Interrupts

**ICR bit 3:** Serial port interrupt flag/enable

```assembly
; Enable serial port interrupt
LDA #%10001000  ; Set bit 7 (write), bit 3 (SP enable)
STA $DC0D       ; ICR

; In IRQ handler:
IRQ_HANDLER:
    LDA $DC0D       ; Read ICR (clears interrupt)
    AND #%00001000  ; Check serial port bit
    BEQ NOT_SERIAL
    ; Handle serial transfer complete
    LDA $DC0C       ; Read/write SDR
NOT_SERIAL:
    ; Handle other interrupts
    RTI
```

#### Common Uses

**1. Tape Port (CIA #1):**
- Cassette data loading uses serial port
- KERNAL reads serial bits for tape read

**2. User Port (CIA #2):**
- Custom serial peripherals (MIDI, modems)
- Networking (RS-232, parallel cables)

**3. Shift Register Chaining:**
- Expand I/O ports using external 74HC595 chips
- Load multiple shift registers in sequence

---

### Register 13: Interrupt Control Register (ICR)

**Address:** $DC0D (CIA #1), $DD0D (CIA #2)
**Access:** Read/Write (behavior differs!)
**Function:** Control and monitor interrupt sources

```
Bit:  7   6   5   4   3   2   1   0
      IR  -   -  FLG  SP ALM  TB  TA
```

#### ICR Bit Definitions

| Bit | Name | Read Operation | Write Operation |
|-----|------|----------------|-----------------|
| **7** | IR | Any interrupt occurred | Set/Clear control bit |
| **6-5** | - | Unused (reads 0) | Unused |
| **4** | FLG | FLAG pin interrupt | Enable/disable FLAG |
| **3** | SP | Serial port interrupt | Enable/disable serial port |
| **2** | ALM | TOD alarm interrupt | Enable/disable TOD alarm |
| **1** | TB | Timer B interrupt | Enable/disable Timer B |
| **0** | TA | Timer A interrupt | Enable/disable Timer A |

#### Reading ICR (Interrupt Status)

**When you read ICR:**
1. Returns current interrupt status
2. **Clears all interrupt flags** (important!)
3. Bit 7 (IR) = 1 if ANY interrupt occurred

```assembly
; Read interrupt status
LDA $DC0D       ; ICR - read and clear
                ; Bit 7 = 1: interrupt occurred
                ; Bits 0-4: which source(s) caused it
```

**Example: Check which interrupt fired**
```assembly
IRQ_HANDLER:
    LDA $DC0D       ; Read ICR (clears flags!)
    STA TEMP        ; Save for testing

    AND #%00000001  ; Check Timer A (bit 0)
    BNE TIMER_A_INT

    LDA TEMP
    AND #%00000010  ; Check Timer B (bit 1)
    BNE TIMER_B_INT

    LDA TEMP
    AND #%00010000  ; Check FLAG pin (bit 4)
    BNE FLAG_INT

    ; Handle other interrupts
    RTI

TIMER_A_INT:
    ; Handle Timer A interrupt
    JMP DONE_INT

TIMER_B_INT:
    ; Handle Timer B interrupt
    JMP DONE_INT

FLAG_INT:
    ; Handle FLAG pin interrupt
    JMP DONE_INT

DONE_INT:
    RTI
```

**Critical:** Reading ICR clears ALL interrupt flags, even ones you don't check!

#### Writing ICR (Interrupt Enable/Disable)

**When you write ICR:**
- Bit 7 = control bit (1 = set masks, 0 = clear masks)
- Bits 0-4 = which interrupt sources to enable/disable

```
Write $DC0D with bit 7 = 1: SET interrupt masks (enable)
Write $DC0D with bit 7 = 0: CLEAR interrupt masks (disable)
```

**Enable interrupts:**
```assembly
; Enable Timer A interrupt
LDA #%10000001  ; Bit 7=1 (set), bit 0=1 (Timer A)
STA $DC0D       ; ICR

; Enable Timer A and Timer B
LDA #%10000011  ; Bit 7=1 (set), bits 0-1=1 (both timers)
STA $DC0D

; Enable all interrupt sources
LDA #%10011111  ; Bit 7=1 (set), bits 0-4=1 (all sources)
STA $DC0D
```

**Disable interrupts:**
```assembly
; Disable Timer A interrupt
LDA #%00000001  ; Bit 7=0 (clear), bit 0=1 (Timer A)
STA $DC0D       ; ICR

; Disable all interrupts
LDA #%01111111  ; Bit 7=0 (clear), bits 0-6=1 (all)
STA $DC0D
```

**Enable one, disable another:**
```assembly
; Disable Timer A, keep others unchanged
LDA #%00000001  ; Bit 7=0 (clear), bit 0=1
STA $DC0D

; Enable Timer B, keep others unchanged
LDA #%10000010  ; Bit 7=1 (set), bit 1=1
STA $DC0D
```

#### CIA #1 vs CIA #2 Interrupts

**Critical difference:**

| CIA | Interrupt Line | Maskable? | Function |
|-----|----------------|-----------|----------|
| **CIA #1** | IRQ | Yes (SEI/CLI) | Keyboard, timers, TOD |
| **CIA #2** | NMI | No | RESTORE key, timers |

**CIA #1 IRQ Example:**
```assembly
; Setup CIA #1 Timer A interrupt
SEI             ; Disable IRQ during setup

LDA #$00        ; Timer value (low)
STA $DC04
LDA #$40        ; Timer value (high)
STA $DC05

LDA #%10000001  ; Enable Timer A interrupt
STA $DC0D       ; CIA #1 ICR

LDA #<IRQ_HANDLER
STA $0314       ; IRQ vector (low)
LDA #>IRQ_HANDLER
STA $0315       ; IRQ vector (high)

LDA #%00010001  ; Start Timer A
STA $DC0E       ; CRA

CLI             ; Enable IRQ
; Timer A interrupts now active
```

**CIA #2 NMI Example:**
```assembly
; Setup CIA #2 NMI (RESTORE key)
LDA #%10010000  ; Enable FLAG interrupt (bit 4)
STA $DD0D       ; CIA #2 ICR

LDA #<NMI_HANDLER
STA $0318       ; NMI vector (low)
LDA #>NMI_HANDLER
STA $0319       ; NMI vector (high)

; Now RESTORE key triggers NMI via CIA #2 FLAG pin
```

#### Interrupt Masking vs Occurrence

**Important distinction:**
- **Interrupt MASK** (writable): enables/disables interrupt generation
- **Interrupt FLAG** (readable): indicates interrupt occurred

```
Interrupt occurs when:
  1. Event happens (timer underflow, FLAG pin, etc.)
  2. AND corresponding mask bit is set
  3. Then: IRQ/NMI line asserted, flag set in ICR

Interrupt does NOT occur when:
  1. Event happens BUT mask bit is clear
  2. Flag still set in ICR (can read it)
  3. But: IRQ/NMI line NOT asserted
```

**Polling example (no interrupt):**
```assembly
; Disable Timer A interrupt but poll for underflow
LDA #%00000001  ; Bit 7=0 (clear mask)
STA $DC0D       ; Disable interrupt

POLL_LOOP:
    LDA $DC0D       ; Read ICR
    AND #%00000001  ; Check Timer A flag
    BEQ POLL_LOOP   ; Wait for underflow
; Timer underflow detected (no interrupt occurred)
```

#### Multiple Interrupt Sources

**ICR bit 7 (IR) is OR of all masked interrupt sources:**

```assembly
; Enable Timer A and Timer B
LDA #%10000011
STA $DC0D

; In IRQ handler
IRQ_HANDLER:
    LDA $DC0D       ; Read ICR
    ; Bit 7 = 1: at least one CIA interrupt
    ; Bits 0-1: which timer(s) caused it

    AND #%00000001  ; Timer A?
    BNE HANDLE_TA

    ; Could be Timer B or both...
```

**Multiple CIAs and VIC-II:**
```assembly
; C64 IRQ can come from CIA #1 OR VIC-II
; Must check both in IRQ handler

IRQ_HANDLER:
    LDA $DC0D       ; CIA #1 ICR
    BNE CIA1_INT    ; Bit 7=1: CIA #1 caused it

    LDA $D019       ; VIC-II interrupt register
    BNE VIC_INT     ; VIC-II caused it

    ; Neither? Might be CIA #2 (but CIA #2 uses NMI!)
    RTI

CIA1_INT:
    ; Handle CIA #1 interrupt
    RTI

VIC_INT:
    ; Handle VIC-II interrupt
    STA $D019       ; Acknowledge VIC-II interrupt
    RTI
```

#### Common Mistakes

**Mistake #1: Not reading ICR in handler**
```assembly
; WRONG: Interrupt flag never cleared!
IRQ_HANDLER:
    ; Do work...
    RTI             ; Flag still set, immediate re-interrupt!

; CORRECT: Always read ICR
IRQ_HANDLER:
    LDA $DC0D       ; Clear interrupt flags
    ; Do work...
    RTI
```

**Mistake #2: Checking ICR bit 7 only**
```assembly
; WRONG: Don't know which interrupt!
IRQ_HANDLER:
    LDA $DC0D
    BMI DO_WORK     ; Just checks bit 7
    RTI
DO_WORK:
    ; Was it Timer A? Timer B? FLAG? Unknown!
```

**Mistake #3: Reading ICR twice**
```assembly
; WRONG: Second read clears flags!
IRQ_HANDLER:
    LDA $DC0D       ; First read (clears flags)
    AND #%00000001  ; Check Timer A
    BNE TIMER_A

    LDA $DC0D       ; Second read - returns 0! (flags already cleared)
    AND #%00000010  ; Always zero!
    BNE TIMER_B     ; Never branches!
```

---

### Registers 14-15: Control Registers (CRA/CRB)

**Addresses:**
- $DC0E/$DD0E: Control Register A (CRA) - Timer A control
- $DC0F/$DD0F: Control Register B (CRB) - Timer B control

**Access:** Read/Write
**Function:** Configure timer behavior, serial port, TOD clock

#### Control Register A (CRA) - $DC0E/$DD0E

```
Bit:  7   6   5   4   3   2   1   0
     TOD  SP SPMODE LOAD RUNMODE PBON OUTMODE START
```

| Bit | Name | Function |
|-----|------|----------|
| **7** | TOD | TOD frequency: 0=60Hz, 1=50Hz |
| **6** | SP | Serial port I/O mode: 0=input, 1=output |
| **5** | SPMODE | Timer A output to serial port: 0=no, 1=yes |
| **4** | LOAD | Force load timer: 1=load, 0=normal (write only) |
| **3** | RUNMODE | Timer mode: 0=continuous, 1=one-shot |
| **2** | PBON | Port B output: 0=normal, 1=timer toggle on PB6 |
| **1** | OUTMODE | Port B output mode: 0=pulse, 1=toggle |
| **0** | START | Timer control: 0=stop, 1=start |

#### CRA Detailed Bit Descriptions

**Bit 0: START - Timer Start/Stop**
```assembly
; Start Timer A
LDA $DC0E       ; CRA
ORA #%00000001  ; Set bit 0
STA $DC0E

; Stop Timer A
LDA $DC0E       ; CRA
AND #%11111110  ; Clear bit 0
STA $DC0E
```

**Bit 1: OUTMODE - Output Mode (PB6)**
```
PBON=0, OUTMODE=x: PB6 normal I/O operation
PBON=1, OUTMODE=0: PB6 outputs single-cycle pulse on underflow
PBON=1, OUTMODE=1: PB6 toggles on each underflow
```

**Bit 2: PBON - Port B On (PB6 Timer Output)**
```assembly
; Make Timer A toggle PB6
LDA #%10000000  ; Set PB6 as output
STA $DC03       ; DDRB

LDA $DC0E       ; CRA
ORA #%00000110  ; Set PBON=1, OUTMODE=1 (toggle)
STA $DC0E
; Now Timer A underflows toggle PB6 HIGH/LOW
```

**Bit 3: RUNMODE - One-Shot vs Continuous**
```assembly
; One-shot mode: timer runs once then stops
LDA $DC0E       ; CRA
ORA #%00001000  ; Set RUNMODE=1 (one-shot)
STA $DC0E

; Continuous mode: timer reloads and continues
LDA $DC0E       ; CRA
AND #%11110111  ; Clear RUNMODE=0 (continuous)
STA $DC0E
```

**Bit 4: LOAD - Force Timer Load**
```assembly
; Set timer value then force load immediately
LDA #$E8
STA $DC04       ; TA LO (goes to latch)
LDA #$03
STA $DC05       ; TA HI (goes to latch)

LDA $DC0E       ; CRA
ORA #%00010000  ; Set LOAD=1 (force load from latch)
STA $DC0E       ; Timer immediately loaded with $03E8
; LOAD bit automatically clears after load
```

**Bit 5: SPMODE - Timer A to Serial Port**
```
SPMODE=0: Timer A independent of serial port
SPMODE=1: Timer A underflow clocks serial port (output mode)
```

**Bit 6: SP - Serial Port Direction**
```
SP=0: Input mode (external clock on CNT)
SP=1: Output mode (Timer A clocks serial port)
```

**Bit 7: TOD - Time-of-Day Frequency**
```
TOD=0: 60 Hz (NTSC)
TOD=1: 50 Hz (PAL)

; PAL C64 setup
LDA $DC0E
ORA #%10000000  ; Set TOD=1 (50 Hz)
STA $DC0E

; NTSC C64 setup
LDA $DC0E
AND #%01111111  ; Clear TOD=0 (60 Hz)
STA $DC0E
```

#### CRA Common Configurations

**Basic timer (continuous mode):**
```assembly
LDA #%00010001  ; START=1, LOAD=1, RUNMODE=0 (continuous)
STA $DC0E       ; CRA
```

**One-shot timer:**
```assembly
LDA #%00011001  ; START=1, LOAD=1, RUNMODE=1 (one-shot)
STA $DC0E       ; CRA
```

**Square wave output on PB6:**
```assembly
LDA #%00010111  ; START=1, LOAD=1, PBON=1, OUTMODE=1 (toggle)
STA $DC0E       ; CRA
```

**Serial port output:**
```assembly
LDA #%01010001  ; START=1, LOAD=1, SP=1 (output), SPMODE=0
STA $DC0E       ; CRA
```

---

#### Control Register B (CRB) - $DC0F/$DD0F

```
Bit:  7   6   5   4   3   2   1   0
    ALARM INMODE LOAD RUNMODE PBON OUTMODE START
```

| Bit | Name | Function |
|-----|------|----------|
| **7** | ALARM | TOD mode: 0=clock, 1=alarm |
| **6-5** | INMODE | Clock source (see table below) |
| **4** | LOAD | Force load timer: 1=load, 0=normal (write only) |
| **3** | RUNMODE | Timer mode: 0=continuous, 1=one-shot |
| **2** | PBON | Port B output: 0=normal, 1=timer toggle on PB7 |
| **1** | OUTMODE | Port B output mode: 0=pulse, 1=toggle |
| **0** | START | Timer control: 0=stop, 1=start |

#### CRB Bits 6-5: INMODE - Timer B Clock Source

| Bit 6 | Bit 5 | Clock Source | Use Case |
|-------|-------|--------------|----------|
| 0 | 0 | φ2 (system clock) | Standard timing |
| 0 | 1 | CNT pin (external) | Event counting |
| 1 | 0 | Timer A underflow | 32-bit timing |
| 1 | 1 | Timer A underflow (CNT high) | Gated timing |

#### CRB Detailed Bit Descriptions

**Bits 0-4, 7:** Same as CRA (see above), except:
- Bit 2 affects **PB7** (not PB6)
- Bit 7 is **ALARM** (not TOD frequency)

**Bits 5-6: INMODE - Timer B Clock Selection**

**Mode %00 - φ2 Clock:**
```assembly
; Timer B counts system clock (1 MHz)
LDA $DC0F       ; CRB
AND #%10011111  ; Clear INMODE bits (00)
STA $DC0F
```

**Mode %01 - CNT Pin:**
```assembly
; Timer B counts external events on CNT pin
LDA $DC0F       ; CRB
AND #%11011111  ; Clear bit 6
ORA #%00100000  ; Set bit 5 (mode = %01)
STA $DC0F
```

**Mode %10 - Timer A Underflow (32-bit mode):**
```assembly
; Timer B counts Timer A underflows
LDA $DC0F       ; CRB
AND #%10111111  ; Clear bit 5
ORA #%01000000  ; Set bit 6 (mode = %10)
STA $DC0F

; Example: 32-bit delay
; Timer A: $FFFF cycles
; Timer B: $1000 counts
; Total: $1000 × $FFFF = 268,369,920 cycles = ~268 seconds
```

**Mode %11 - Gated Timer A:**
```assembly
; Timer B counts Timer A underflows, but only when CNT=HIGH
LDA $DC0F       ; CRB
ORA #%01100000  ; Set both bits 5-6 (mode = %11)
STA $DC0F

; Used for pulse width measurement:
;   - Timer A runs continuously
;   - Timer B counts only while external signal HIGH
;   - Timer B value = pulse width in Timer A units
```

**Bit 7: ALARM - TOD Alarm Mode**

```assembly
; Write alarm time
LDA $DC0F       ; CRB
ORA #%10000000  ; Set ALARM=1 (alarm mode)
STA $DC0F

; Write time to alarm registers
LDA #$83        ; 03:xx:xx PM
STA $DC0B       ; TOD HR
LDA #$00
STA $DC0A       ; TOD MIN
LDA #$00
STA $DC09       ; TOD SEC
LDA #$00
STA $DC08       ; TOD 10THS

; Switch back to clock mode
LDA $DC0F       ; CRB
AND #%01111111  ; Clear ALARM=0 (clock mode)
STA $DC0F
```

#### CRB Common Configurations

**Basic timer (φ2 clock):**
```assembly
LDA #%00010001  ; START=1, LOAD=1, INMODE=%00 (φ2)
STA $DC0F       ; CRB
```

**32-bit timer (counts Timer A):**
```assembly
; Timer A setup (runs fast)
LDA #%00010001  ; START=1, LOAD=1
STA $DC0E       ; CRA

; Timer B setup (counts Timer A underflows)
LDA #%01010001  ; START=1, LOAD=1, INMODE=%10 (Timer A)
STA $DC0F       ; CRB
```

**Event counter (CNT pin):**
```assembly
LDA #%00110001  ; START=1, LOAD=1, INMODE=%01 (CNT)
STA $DC0F       ; CRB
```

#### Reading Control Registers

```assembly
; Check if timer is running
LDA $DC0E       ; CRA
AND #%00000001  ; Check START bit
BNE RUNNING     ; Non-zero = running
; Timer stopped
```

**Note:** Reading CRA/CRB returns current configuration. All bits readable except LOAD (bit 4), which always reads as 0.

---

## Functional Description

This section provides a comprehensive operational guide for all CIA features, explaining how each subsystem works and how to use it effectively in assembly programming.

---

### I/O Ports (PRA, PRB, DDRA, DDRB)

The CIA provides two 8-bit bidirectional I/O ports (Port A and Port B), each with individual bit-level direction control.

#### Port Architecture

Each port consists of:
1. **Peripheral Data Register (PR)** - The actual I/O data register ($DC00/$DC01 or $DD00/$DD01)
2. **Data Direction Register (DDR)** - Controls input/output per bit ($DC02/$DC03 or $DD02/$DD03)

**DDR Bit Operation:**
- **DDR bit = 0:** Corresponding PR bit is an INPUT (high-impedance with pull-up)
- **DDR bit = 1:** Corresponding PR bit is an OUTPUT (actively driven)

**Reading the Port:**
- Reading PR always returns the state of the actual port pins
- This applies to BOTH input and output bits
- Output bits read back the output latch value (not necessarily what's on the pin if external load pulls differently)

#### Pull-Up Resistors

Both Port A and Port B have **passive and active pull-up devices**:
- Provides both CMOS and TTL compatibility
- Each pin can drive 2 TTL loads (fan-out = 2)
- When configured as input, pin reads HIGH unless externally driven LOW

**Example - Keyboard Matrix Scanning:**
```assembly
; Port A: outputs (drive keyboard rows LOW)
LDA #$00
STA $DC02       ; DDRA = $00 (all outputs)

; Port B: inputs (read keyboard columns)
LDA #$FF
STA $DC03       ; DDRB = $FF (all inputs)

; Scan row 0
LDA #$FE        ; Drive row 0 LOW, others HIGH
STA $DC00       ; PRA = $FE

; Read columns (pull-ups hold HIGH, key press pulls LOW)
LDA $DC01       ; PRB - reads which columns are LOW
```

#### Port B Special Functions (PB6, PB7)

**In addition to normal I/O, PB6 and PB7 provide timer output functions:**

- **PB6:** Can output Timer A signals (pulse or toggle)
- **PB7:** Can output Timer B signals (pulse or toggle)
- When timer output enabled, **overrides** DDRB control for that bit
- Automatically forces the pin to output mode

**Timer Output on PB6:**
```assembly
; Make Timer A toggle PB6 every underflow
LDA #%00000110  ; PBON=1 (bit 2), OUTMODE=1 (bit 1 = toggle)
STA $DC0E       ; CRA

; PB6 now toggles every time Timer A underflows
; Useful for generating square waves, clock signals
```

---

### Handshaking

The CIA provides automated handshaking for data transfers using the **PC output pin** and **FLAG input pin**.

#### PC (Port Control) Output

**PC pin behavior:**
- Goes LOW for **one cycle** following a read or write of **PORT B**
- Can indicate "data ready" at PORT B
- Can indicate "data accepted" from PORT B

**Use case:** Parallel data transfer signaling
```assembly
; Write byte to Port B
LDA #$55
STA $DC01       ; Write to PRB
; PC pin goes LOW for 1 cycle automatically
; External device sees PC pulse = "data ready"
```

#### 16-Bit Handshaking

**For 16-bit data transfers using both ports:**
- **Always read or write PORT A first**
- PORT B access triggers PC output
- This provides handshaking on the 16-bit operation

**Example:**
```assembly
; Send 16-bit value: $1234
LDA #$12
STA $DC00       ; PORT A (no PC pulse)
LDA #$34
STA $DC01       ; PORT B (PC pulses LOW - signals "16-bit data ready")
```

#### FLAG Input

**FLAG pin characteristics:**
- **Negative edge sensitive** (triggers on HIGH→LOW transition)
- Can receive PC output from another 6526
- Can be used as general-purpose interrupt input
- Any negative transition sets the FLAG interrupt bit

**Common uses:**
- **CIA #1 FLAG:** Cassette data input
- **CIA #2 FLAG:** RESTORE key (generates NMI!)

**Example - FLAG interrupt:**
```assembly
; Enable FLAG interrupt
LDA #%10010000  ; Set bit 7 (write), bit 4 (FLAG enable)
STA $DC0D       ; ICR

; Now negative edge on FLAG pin generates interrupt
; IRQ handler checks:
IRQ_HANDLER:
    LDA $DC0D       ; Read ICR
    AND #%00010000  ; Check FLAG bit
    BEQ NOT_FLAG
    ; Handle FLAG interrupt
NOT_FLAG:
    RTI
```

---

### Interval Timers (Timer A, Timer B)

Each CIA has two independent 16-bit interval timers for precision timing, pulse generation, and event counting.

#### Timer Architecture

**Each timer consists of:**
1. **16-bit Timer Counter** (read-only) - Current countdown value
2. **16-bit Timer Latch** (write-only) - Reload value storage
3. **Control Register** (CRA/CRB) - Configuration and control

**Key concept:** Writing to timer registers writes to the LATCH, not the counter directly!

**Latch Loading:**
The latch is loaded into the counter when:
1. Timer underflows (reaches $0000) in continuous mode
2. LOAD bit forced to 1 in CRA/CRB
3. Timer starts (START bit 0→1 transition)
4. High byte written while timer stopped

#### Timer Modes

**Start/Stop (CRA/CRB bit 0):**
```assembly
; Start Timer A
LDA $DC0E
ORA #%00000001  ; Set START bit
STA $DC0E

; Stop Timer A
LDA $DC0E
AND #%11111110  ; Clear START bit
STA $DC0E
```

**One-Shot vs Continuous (CRA/CRB bit 3):**
- **Continuous (bit 3 = 0):** Count to zero, reload, repeat forever
- **One-Shot (bit 3 = 1):** Count to zero, reload, STOP (START bit auto-clears)

```assembly
; One-shot: Fire once then stop
LDA #$FF
STA $DC04       ; Timer low
LDA #$FF
STA $DC05       ; Timer high
LDA #%00011001  ; START=1, LOAD=1, RUNMODE=1 (one-shot)
STA $DC0E       ; CRA
; Timer counts $FFFF → $0000, generates interrupt, stops

; Continuous: Repeat forever
LDA #$FF
STA $DC04
LDA #$FF
STA $DC05
LDA #%00010001  ; START=1, LOAD=1, RUNMODE=0 (continuous)
STA $DC0E       ; CRA
; Timer counts $FFFF → $0000, reloads $FFFF, repeats
```

#### Force Load (CRA/CRB bit 4)

**Strobe bit:** Immediately loads latch into counter

```assembly
; Change timer value while running
LDA #$E8
STA $DC04       ; Write to latch
LDA #$03
STA $DC05       ; Write to latch

LDA $DC0E
ORA #%00010000  ; Set LOAD bit
STA $DC0E       ; Counter immediately loaded with $03E8
                ; LOAD bit automatically clears
```

**Important:** LOAD is a strobe - writing 0 has no effect, always reads as 0

#### Timer Input Modes

**Timer A Input (CRA bit 5):**
- **Bit 5 = 0:** Count φ2 clock pulses (~1 MHz)
- **Bit 5 = 1:** Count positive edges on CNT pin (external events)

**Timer B Input (CRB bits 6-5):**

| CRB6 | CRB5 | Clock Source | Use Case |
|------|------|--------------|----------|
| 0 | 0 | φ2 pulses | Standard timing |
| 0 | 1 | CNT positive edges | External event counting |
| 1 | 0 | Timer A underflows | 32-bit cascade timer |
| 1 | 1 | Timer A underflows (CNT high) | Gated/pulse width measurement |

**32-Bit Cascade Timer:**
```assembly
; Timer A = low 16 bits, Timer B = high 16 bits
; Maximum: $FFFFFFFF cycles = ~4295 seconds @ 1 MHz

; Set Timer A
LDA #$FF
STA $DC04
STA $DC05

; Set Timer B to count Timer A underflows
LDA #$FF
STA $DC06
STA $DC07

; Configure Timer B: count Timer A underflows
LDA #%01010001  ; START=1, LOAD=1, INMODE=%10 (Timer A)
STA $DC0F       ; CRB

; Start Timer A
LDA #%00010001  ; START=1, LOAD=1
STA $DC0E       ; CRA

; Now have 32-bit timer: Timer B:Timer A
```

#### Port B Output (PB6/PB7)

**Timers can drive Port B pins (CRA/CRB bits 1-2):**
- **PB On (bit 2):** Enable timer output on PB6 (Timer A) or PB7 (Timer B)
- **Out Mode (bit 1):** Select pulse or toggle

**Pulse Mode (bit 1 = 0):**
- Generates single positive pulse (one cycle duration) on underflow

**Toggle Mode (bit 1 = 1):**
- Toggles output on every underflow
- Set HIGH when timer starts
- Set LOW by RES (reset)

**Square Wave Generation:**
```assembly
; Generate 50 Hz square wave on PB6 using Timer A

; Set PB6 as output (though timer overrides this anyway)
LDA #%01000000
STA $DC03       ; DDRB bit 6

; Timer value for 50 Hz: 985248 / (50 * 2) = 9852 = $2678
LDA #$78
STA $DC04       ; Timer A low
LDA #$26
STA $DC05       ; Timer A high

; Configure: START, LOAD, PBON, OUTMODE (toggle)
LDA #%00010111  ; Bits: START=1, PBON=1, OUTMODE=1
STA $DC0E       ; CRA

; PB6 now outputs 50 Hz square wave (toggles every 9852 cycles)
```

#### Timer Uses

**Common applications:**
1. **Music/Sound Timing** - Note duration, SID register updates
2. **Precise Delays** - Cycle-accurate timing
3. **Pulse Generation** - Variable width pulses, PWM
4. **Frequency Measurement** - Count external pulses
5. **Pulse Width Measurement** - Gated Timer B mode
6. **Interrupt Generation** - Regular IRQ/NMI triggers

---

### Time-of-Day Clock (TOD)

The TOD clock is a special-purpose timer for real-time applications with 24-hour clock capability and alarm function.

#### TOD Architecture

**Four BCD registers:**
1. **TOD 10THS** ($DC08/$DD08) - Tenths of seconds (0-9)
2. **TOD SEC** ($DC09/$DD09) - Seconds (00-59)
3. **TOD MIN** ($DC0A/$DD0A) - Minutes (00-59)
4. **TOD HR** ($DC0B/$DD0B) - Hours (01-12) + AM/PM flag (bit 7)

**BCD Format:** Each nibble = one decimal digit
- $23 = 23 (not 35 decimal!)
- $59 = 59 seconds (maximum)
- $12 = 12 o'clock

**Hours Format:**
- Range: 01-12 (NOT 00-11, NOT 00-23!)
- Bit 7 = AM/PM flag (0=AM, 1=PM)
- Examples: $01=1 AM, $12=noon, $81=1 PM, $92=midnight

#### External Clock Input

**TOD pin requires 50 Hz or 60 Hz TTL signal:**
- Selected by CRA bit 7
- **CRA bit 7 = 0:** 60 Hz input (NTSC)
- **CRA bit 7 = 1:** 50 Hz input (PAL)

**C64 automatically provides correct frequency**

#### Setting TOD Time

**Critical sequence:** Write HOURS first, TENTHS last

```assembly
; Set time to 02:30:45.5 PM

LDA #$82        ; 02 PM (bit 7 = 1 for PM)
STA $DC0B       ; Write HOURS FIRST - stops clock

LDA #$30        ; 30 minutes (BCD)
STA $DC0A       ; Write MINUTES

LDA #$45        ; 45 seconds (BCD)
STA $DC09       ; Write SECONDS

LDA #$05        ; 5 tenths
STA $DC08       ; Write TENTHS LAST - starts clock

; Clock now running from 02:30:45.5 PM
```

**Why this order?**
- Writing HOURS stops the clock (prevents partial updates)
- Writing TENTHS starts the clock
- Ensures all four registers update atomically

#### Reading TOD Time

**Critical sequence:** Read HOURS first, TENTHS last

```assembly
; Read current time

LDA $DC0B       ; Read HOURS FIRST - latches all registers
STA HOUR        ; Save hours

LDA $DC0A       ; Read MINUTES (latched value)
STA MINUTE

LDA $DC09       ; Read SECONDS (latched value)
STA SECOND

LDA $DC08       ; Read TENTHS LAST - unlocks latch
STA TENTH

; Now HOUR:MINUTE:SECOND:TENTH is consistent snapshot
```

**Why this order?**
- Reading HOURS latches all four registers
- Clock continues running, but latches hold snapshot
- Reading TENTHS unlocks the latch for next read
- Prevents time from changing mid-read (race condition)

**Single register read:**
If reading only ONE register (e.g., seconds for simple timing):
- Can read "on the fly" without latching
- **BUT:** Must read TENTHS after reading HOURS to unlock latch
- Otherwise, next read returns stale latched data

```assembly
; Read just seconds (no latching needed)
LDA $DC09       ; Read seconds directly
; Use value immediately
```

#### TOD Alarm Function

**ALARM registers share same addresses as TOD registers**

**Access controlled by CRB bit 7:**
- **CRB bit 7 = 0:** Accessing TOD clock (read time)
- **CRB bit 7 = 1:** Accessing ALARM (write alarm time)

**Important:** ALARM is write-only - reading always returns TOD time!

**Setting an alarm:**
```assembly
; Set alarm for 08:30:00.0 AM

; Switch to ALARM mode
LDA $DC0F       ; CRB
ORA #%10000000  ; Set bit 7 (ALARM mode)
STA $DC0F

; Write alarm time (same format as TOD)
LDA #$08        ; 08 AM (bit 7 = 0)
STA $DC0B       ; Alarm hours
LDA #$30        ; 30 minutes
STA $DC0A       ; Alarm minutes
LDA #$00        ; 00 seconds
STA $DC09       ; Alarm seconds
LDA #$00        ; 0 tenths
STA $DC08       ; Alarm tenths

; Switch back to TOD mode
LDA $DC0F       ; CRB
AND #%01111111  ; Clear bit 7 (TOD mode)
STA $DC0F

; Enable TOD alarm interrupt
LDA #%10000100  ; Set bit 7 (write), bit 2 (ALARM enable)
STA $DC0D       ; ICR

; When TOD reaches 08:30:00.0, generates interrupt
; Check in IRQ handler:
IRQ_HANDLER:
    LDA $DC0D       ; Read ICR
    AND #%00000100  ; Check ALARM bit
    BNE ALARM_INT
    ; ...
ALARM_INT:
    ; Handle alarm interrupt
    RTI
```

---

### Serial Port (SDR)

The serial port is a buffered 8-bit synchronous shift register for serial communication.

#### Serial Port Modes

**CRA bit 6 selects input or output mode:**
- **Bit 6 = 0:** Input mode (receive data)
- **Bit 6 = 1:** Output mode (transmit data)

#### Input Mode (Receiving Data)

**Configuration:**
- Data shifted in on SP pin
- Clock on CNT pin (external clock required)
- Shift on rising edge of CNT
- After 8 CNT pulses, data dumped to SDR and interrupt generated

```assembly
; Setup serial input
LDA #%00000000  ; SPMODE=0 (input)
STA $DC0E       ; CRA

; Enable serial port interrupt
LDA #%10001000  ; Set bit 7 (write), bit 3 (SP enable)
STA $DC0D       ; ICR

; Wait for byte
WAIT_SERIAL:
    LDA $DC0D       ; Read ICR
    AND #%00001000  ; Check SP bit
    BEQ WAIT_SERIAL ; Wait for interrupt

; Read received byte
LDA $DC0C       ; SDR contains received byte
STA RECEIVED_DATA
```

#### Output Mode (Transmitting Data)

**Configuration:**
- Timer A generates baud rate
- Data shifted out on SP pin at **½ Timer A underflow rate**
- Clock output on CNT pin
- After 8 shifts, interrupt generated

**Maximum baud rate:** φ2 / 4 (≈250 kbps @ 1 MHz)

```assembly
; Setup serial output

; Set Timer A for baud rate
; Example: 19200 baud = 1022727 Hz / 19200 / 2 = 26.6 ≈ 27 cycles
LDA #27
STA $DC04       ; Timer A low
LDA #$00
STA $DC05       ; Timer A high

; Configure Timer A: continuous mode, serial port output
LDA #%01010001  ; START=1, LOAD=1, SPMODE=1 (serial output)
STA $DC0E       ; CRA

; Enable serial port interrupt
LDA #%10001000  ; Set bit 7 (write), bit 3 (SP enable)
STA $DC0D       ; ICR

; Transmit byte
LDA #$55        ; Data to send
STA $DC0C       ; SDR - transmission starts immediately

; Wait for transmission complete
WAIT_TX:
    LDA $DC0D       ; Read ICR
    AND #%00001000  ; Check SP bit
    BEQ WAIT_TX     ; Wait for interrupt

; Transmission complete, can send next byte
LDA #$AA
STA $DC0C       ; Next byte (continuous transmission)
```

**Data format:** MSB first (bit 7 shifted out first)

#### Serial Port Timing

**Output mode timing:**
```
Timer A underflow rate = baud rate × 2

Example for 9600 baud:
  Timer A = 1022727 Hz / (9600 × 2) = 53.2 ≈ 53 cycles
```

**CNT pin behavior:**
- Data valid on falling edge of CNT
- Remains valid until next falling edge
- SP pin holds last bit after 8th shift

#### Master/Slave Configuration

**Multiple 6526 chips can share a common serial bus:**
- One CIA acts as MASTER (output mode, sources clock)
- Other CIAs act as SLAVES (input mode, use master's clock)
- Both CNT and SP are **open drain** (allows wired-OR bus)
- Protocol for master/slave selection transmitted over bus or via separate handshaking

**Why C64 doesn't use SDR for IEC bus:**
- IEC protocol requires precise handshaking
- Multiple devices with addressing
- Variable timing requirements
- More control than shift register provides
- KERNAL uses bit-banged protocol via Port A instead

---

### Interrupt Control (ICR)

The ICR provides masking and status for five interrupt sources.

#### Interrupt Sources

1. **Timer A underflow** (bit 0)
2. **Timer B underflow** (bit 1)
3. **TOD alarm match** (bit 2)
4. **Serial port full/empty** (bit 3)
5. **FLAG pin negative edge** (bit 4)

#### ICR Register Structure

**Read-only DATA register (reading ICR):**
- Bits 0-4: Interrupt flags (1 = interrupt occurred)
- Bit 7 (IR): Set if ANY enabled interrupt occurred
- **Reading ICR clears ALL interrupt flags!**

**Write-only MASK register (writing ICR):**
- Bit 7 (SET/CLEAR): Control bit
  - **Bit 7 = 1:** SET mask bits (enable interrupts)
  - **Bit 7 = 0:** CLEAR mask bits (disable interrupts)
- Bits 0-4: Which interrupt sources to set/clear

#### Enabling Interrupts

```assembly
; Enable Timer A interrupt
LDA #%10000001  ; Bit 7=1 (set), bit 0=1 (Timer A)
STA $DC0D       ; ICR

; Enable Timer A and Timer B
LDA #%10000011  ; Bit 7=1 (set), bits 0-1 (both timers)
STA $DC0D       ; ICR

; Enable all interrupts
LDA #%10011111  ; Bit 7=1 (set), bits 0-4 (all sources)
STA $DC0D       ; ICR
```

#### Disabling Interrupts

```assembly
; Disable Timer A interrupt (keep others unchanged)
LDA #%00000001  ; Bit 7=0 (clear), bit 0=1 (Timer A)
STA $DC0D       ; ICR

; Disable all interrupts
LDA #%01111111  ; Bit 7=0 (clear), bits 0-6=1 (all)
STA $DC0D       ; ICR
```

#### Reading Interrupt Status

```assembly
IRQ_HANDLER:
    LDA $DC0D       ; Read ICR (clears flags!)
    STA TEMP        ; MUST save value

    AND #%00000001  ; Check Timer A
    BNE TIMER_A_INT

    LDA TEMP        ; Reload saved ICR
    AND #%00000010  ; Check Timer B
    BNE TIMER_B_INT

    LDA TEMP
    AND #%00000100  ; Check TOD alarm
    BNE ALARM_INT

    LDA TEMP
    AND #%00001000  ; Check serial port
    BNE SERIAL_INT

    LDA TEMP
    AND #%00010000  ; Check FLAG
    BNE FLAG_INT

    RTI             ; No CIA interrupt

TIMER_A_INT:
    ; Handle Timer A
    JMP DONE_INT

TIMER_B_INT:
    ; Handle Timer B
    JMP DONE_INT

; ... other handlers ...

DONE_INT:
    RTI
```

**Critical:** Never read ICR twice - second read returns $00 (flags already cleared)!

#### Interrupt vs Mask vs Flag

**Three concepts:**
1. **Interrupt FLAG:** Set when event occurs (regardless of mask)
2. **Interrupt MASK:** Enables/disables interrupt generation
3. **IR bit (bit 7):** Set only if event occurred AND mask enabled

**Polling without interrupts:**
```assembly
; Disable Timer A interrupt but poll for underflow
LDA #%00000001  ; Bit 7=0 (clear mask)
STA $DC0D       ; Disable interrupt (no IRQ generated)

POLL_LOOP:
    LDA $DC0D       ; Read ICR
    AND #%00000001  ; Check Timer A flag (still set!)
    BEQ POLL_LOOP   ; Wait for underflow

; Underflow detected (no interrupt was generated)
```

**Why IR bit is useful:**
- In multi-chip system, poll IR to determine which CIA generated interrupt
- IR bit cleared by reading ICR
- IRQ line returns HIGH after reading ICR

---

### Control Registers (CRA, CRB)

Complete bit-level reference for timer control.

#### Control Register A (CRA) - $DC0E

```
Bit:  7   6   5   4   3   2   1   0
     TOD  SP SPMODE LOAD RUNMODE PBON OUTMODE START
```

| Bit | Name | Function | Values |
|-----|------|----------|--------|
| **0** | START | Start/Stop Timer A | 0=Stop, 1=Start (auto-clears in one-shot mode) |
| **1** | OUTMODE | PB6 output mode | 0=Pulse (1 cycle), 1=Toggle |
| **2** | PBON | PB6 timer output enable | 0=Normal PB6, 1=Timer A on PB6 (overrides DDR) |
| **3** | RUNMODE | One-shot or continuous | 0=Continuous, 1=One-shot |
| **4** | LOAD | Force load latch → counter | 1=Force load (strobe, always reads 0) |
| **5** | INMODE | Timer A clock source | 0=φ2 clock, 1=CNT pin |
| **6** | SPMODE | Serial port mode | 0=Input (external clock), 1=Output (Timer A clock) |
| **7** | TODIN | TOD frequency | 0=60 Hz (NTSC), 1=50 Hz (PAL) |

#### Control Register B (CRB) - $DC0F

```
Bit:  7   6   5   4   3   2   1   0
    ALARM  INMODE  LOAD RUNMODE PBON OUTMODE START
```

| Bit | Name | Function | Values |
|-----|------|----------|--------|
| **0** | START | Start/Stop Timer B | 0=Stop, 1=Start |
| **1** | OUTMODE | PB7 output mode | 0=Pulse, 1=Toggle |
| **2** | PBON | PB7 timer output enable | 0=Normal PB7, 1=Timer B on PB7 |
| **3** | RUNMODE | One-shot or continuous | 0=Continuous, 1=One-shot |
| **4** | LOAD | Force load latch → counter | 1=Force load (strobe) |
| **5-6** | INMODE | Timer B clock source | See table below |
| **7** | ALARM | TOD/ALARM select | 0=TOD clock, 1=ALARM registers |

**Timer B Input Mode (CRB bits 6-5):**

| Bit 6 | Bit 5 | Clock Source |
|-------|-------|--------------|
| 0 | 0 | φ2 clock pulses |
| 0 | 1 | CNT pin positive edges |
| 1 | 0 | Timer A underflow pulses |
| 1 | 1 | Timer A underflow pulses while CNT high |

#### Common Control Patterns

**Simple continuous timer:**
```assembly
LDA #%00010001  ; START=1, LOAD=1, RUNMODE=0
STA $DC0E       ; CRA - Timer A continuous
```

**One-shot timer:**
```assembly
LDA #%00011001  ; START=1, LOAD=1, RUNMODE=1
STA $DC0E       ; CRA - fires once, stops
```

**Square wave on PB6:**
```assembly
LDA #%00010111  ; START=1, LOAD=1, PBON=1, OUTMODE=1
STA $DC0E       ; CRA - toggles PB6 on underflow
```

**32-bit cascade:**
```assembly
; Timer A
LDA #%00010001  ; START=1, LOAD=1
STA $DC0E       ; CRA

; Timer B (counts Timer A underflows)
LDA #%01010001  ; START=1, LOAD=1, INMODE=%10
STA $DC0F       ; CRB
```

**Serial output:**
```assembly
LDA #%01010001  ; START=1, LOAD=1, SPMODE=1
STA $DC0E       ; CRA - Timer A drives serial clock
```

**TOD frequency (PAL):**
```assembly
LDA $DC0E
ORA #%10000000  ; Set bit 7 (50 Hz)
STA $DC0E       ; CRA
```

---

### Summary: CIA Programming Checklist

**Before using CIA features:**

1. **I/O Ports:**
   - [ ] Set DDR before using port (default is all inputs)
   - [ ] Remember pull-ups on inputs (read HIGH if floating)
   - [ ] Check if timer output overrides DDR for PB6/PB7

2. **Timers:**
   - [ ] Write timer values (goes to latch, not counter!)
   - [ ] Use LOAD bit or START to load latch into counter
   - [ ] Enable interrupt if needed (write ICR with bit 7=1)
   - [ ] Read high byte first to avoid race conditions

3. **TOD Clock:**
   - [ ] Set CRA bit 7 for correct frequency (50/60 Hz)
   - [ ] Write hours first, tenths last (stops/starts clock)
   - [ ] Read hours first, tenths last (latches/unlatches)

4. **Serial Port:**
   - [ ] Configure Timer A for baud rate (output mode)
   - [ ] Set CRA bit 6 for input/output mode
   - [ ] Data format is MSB first
   - [ ] Enable serial interrupt if needed

5. **Interrupts:**
   - [ ] Write ICR with bit 7=1 to enable masks
   - [ ] Read ICR in handler (clears flags - save value!)
   - [ ] Check bit 7 (IR) to see if CIA caused interrupt
   - [ ] Check individual bits to determine source

---

## Quick Reference

### CIA Chip Locations in C64

| CIA | Address Range | Decimal | Function |
|-----|---------------|---------|----------|
| **CIA #1** | $DC00-$DC0F | 56320-56335 | Keyboard, joysticks, IRQ |
| **CIA #2** | $DD00-$DD0F | 56576-56591 | Serial bus, User Port, NMI |

### Register Quick Map

All registers are **read/write** unless noted.

| Offset | CIA #1 ($DC00+) | CIA #2 ($DD00+) | Description |
|--------|-----------------|-----------------|-------------|
| **+$00** | Keyboard rows, Joy2 | Serial bus, VIC bank | Port A Data (PRA) |
| **+$01** | Keyboard cols, Joy1 | User Port, RS-232 | Port B Data (PRB) |
| **+$02** | Port A DDR | Port A DDR | Data Direction A |
| **+$03** | Port B DDR | Port B DDR | Data Direction B |
| **+$04** | Timer A Lo | Timer A Lo | Timer A Low Byte |
| **+$05** | Timer A Hi | Timer A Hi | Timer A High Byte |
| **+$06** | Timer B Lo | Timer B Lo | Timer B Low Byte |
| **+$07** | Timer B Hi | Timer B Hi | Timer B High Byte |
| **+$08** | TOD 10ths | (unused) | TOD Tenths of Second |
| **+$09** | TOD Seconds | (unused) | TOD Seconds |
| **+$0A** | TOD Minutes | (unused) | TOD Minutes |
| **+$0B** | TOD Hours | (unused) | TOD Hours + AM/PM |
| **+$0C** | Serial Data | Serial Data | Serial Shift Register |
| **+$0D** | Interrupt Control | NMI Control | Interrupt Status/Mask |
| **+$0E** | Timer A Control | Timer A Control | Control Register A |
| **+$0F** | Timer B Control | Timer B Control | Control Register B |

### Common Operations Quick Reference

**Read Joystick 1:**
```assembly
LDA $DC00   ; Port A = Joy 2
LDA $DC01   ; Port B = Joy 1
            ; Bit 0 = Up, 1 = Down, 2 = Left, 3 = Right, 4 = Fire
```

**Setup Timer A for 1/50th second:**
```assembly
LDA #$27    ; Low byte of 19656 (1 MHz / 50 Hz)
STA $DC04
LDA #$4C    ; High byte
STA $DC05
```

**Switch VIC Bank (CIA #2):**
```assembly
LDA $DD00
AND #%11111100  ; Clear bits 0-1
ORA #%00000010  ; Set bank (inverted: %10 = bank 1)
STA $DD00
```

### Interrupt Sources

**CIA #1 (IRQ):**
- Timer A underflow
- Timer B underflow
- TOD alarm
- Serial register full
- FLAG pin (cassette)

**CIA #2 (NMI):**
- Timer A/B underflow
- TOD alarm
- Serial register full
- FLAG pin (**RESTORE key**)

---

## Version History

- **v2.0 COMPLETE** - Updated with Appendix M Part 4/4 (functional description)
  - Complete functional description for all CIA subsystems
  - I/O ports operational guide (PR, DDR, pull-ups, handshaking)
  - Interval timers comprehensive guide (modes, input sources, cascade)
  - Time-of-Day clock complete reference (BCD format, latching, alarm)
  - Serial port programming (input/output modes, master/slave)
  - Interrupt control detailed guide (masking, polling, multi-source)
  - Control register bit-level reference (CRA/CRB)
  - Programming checklists for each subsystem
  - Common patterns and configurations
  - Practical assembly examples for all features
  - **Document now COMPLETE - all 4 parts processed**

- **v1.2** - Updated with Appendix M Part 3/4 (interface signals and timing)
  - Complete interface signal descriptions (φ2, CS, R/W, RS, DB, IRQ, RES)
  - Detailed timing characteristics for 1 MHz and 2 MHz operation
  - Write cycle timing specifications and diagrams
  - Read cycle timing specifications and diagrams
  - Timing considerations for assembly programming
  - Race condition examples and solutions
  - IRQ sharing with VIC-II explained
  - Reset behavior documentation
  - Hardware integration notes for designers
  - Practical timing diagrams with ASCII art

- **v1.1** - Updated with Appendix M Part 2/4 (comprehensive register documentation)
  - Complete register map (all 16 registers)
  - Detailed register descriptions with C64-specific examples
  - Port A/B data registers (PRA/PRB)
  - Data direction registers (DDRA/DDRB)
  - Timer A and Timer B registers (16-bit counters)
  - Time-of-Day clock registers (TOD)
  - Serial data register (SDR)
  - Interrupt control register (ICR)
  - Control registers A and B (CRA/CRB)
  - Comprehensive assembly code examples throughout
  - Common mistakes and gotchas documented
  - Reading/writing timing considerations
  - 32-bit timer cascade examples
  - BCD time format explanations
  - Serial port programming patterns
  - Interrupt handling best practices

- **v1.0** - Initial reference from Appendix M Part 1/4
  - CIA overview and features
  - Pin configuration
  - Block diagram
  - Electrical specifications
  - C64-specific usage notes

---

*This reference will be updated as additional sections are processed from Parts 3-4 of Appendix M (timer programming examples, TOD clock details, serial port details, interrupt system details).*
