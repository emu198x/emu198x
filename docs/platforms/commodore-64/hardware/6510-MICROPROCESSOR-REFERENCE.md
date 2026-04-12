# 6510 Microprocessor Reference

**⚠️ FOR LESSON CREATION: See [6510-QUICK-REFERENCE.md](6510-QUICK-REFERENCE.md) first (85% smaller, programming-focused)**

**Source:** C64 Programmer's Reference Guide - Appendix L
**For:** Code Like It's 198x - Assembly Language Lessons
**Applies to:** Commodore 64 (all revisions)

---

## Quick Start

**If you're creating lessons**, you probably want [6510-QUICK-REFERENCE.md](6510-QUICK-REFERENCE.md) instead. It contains:
- Instruction set organized by category
- Addressing mode examples
- Common code patterns
- Cycle counts for timing
- **12KB vs 85KB** (this file)

**This comprehensive reference** contains hardware specifications useful for deep technical work:
- Pin configurations and electrical specifications
- Clock timing and signal descriptions
- Complete opcode table with hex values
- Hardware architecture details

---

## Document Status

This reference covers the 6510 CPU used in the Commodore 64. It will be expanded as additional sections are processed.

**Status: COMPLETE**

All content from C64 Programmer's Reference Guide Appendix L has been processed.

**Documented:**
- ✅ CPU Architecture & Features
- ✅ Pin Configuration & Block Diagram
- ✅ Electrical Characteristics & Timing
- ✅ Signal Descriptions (all pins)
- ✅ Addressing Modes (all 13 modes)
- ✅ Instruction Set (all 56 opcodes)
- ✅ Complete Opcode Reference (with cycle counts)
- ✅ Programming Model & Memory Map
- ✅ Application Notes

---

## Table of Contents

1. [Overview](#overview)
2. [6510 vs 6502 Differences](#6510-vs-6502-differences)
3. [CPU Features](#cpu-features)
4. [Architecture](#architecture)
5. [Pin Configuration](#pin-configuration)
6. [I/O Port](#io-port)
7. [Electrical Specifications](#electrical-specifications)
8. [Clock Timing](#clock-timing)
9. [Detailed Timing Specifications](#detailed-timing-specifications)
10. [Signal Descriptions](#signal-descriptions)
11. [Addressing Modes](#addressing-modes)
12. [Instruction Set](#instruction-set)
13. [Opcode Reference](#opcode-reference)
14. [Quick Reference Tables](#quick-reference-tables)
15. [Educational Notes](#educational-notes)
16. [References for Lesson Development](#references-for-lesson-development)

---

## Overview

The 6510 is the CPU used in all Commodore 64 computers. It is essentially a MOS 6502 with an added 8-bit I/O port integrated on-chip. The processor runs at approximately 1 MHz (PAL: 0.985 MHz, NTSC: 1.023 MHz).

### Key Specifications

| Specification | Value |
|--------------|-------|
| **Data Bus** | 8-bit |
| **Address Bus** | 16-bit (64KB addressable) |
| **Clock Speed** | 1 MHz (0.985-1.023 MHz actual) |
| **Instructions** | 56 official opcodes |
| **Addressing Modes** | 13 modes |
| **Power Supply** | +5V single supply |
| **Technology** | N-channel silicon gate, depletion load |
| **Package** | 40-pin DIP |

---

## 6510 vs 6502 Differences

The 6510 is **software-compatible** with the 6502, with one key hardware addition:

### What's Different

| Feature | 6502 | 6510 |
|---------|------|------|
| **I/O Port** | None | 8-bit port at $0000/$0001 |
| **Memory Locations** | $0000-$0001 are RAM | $0000/$0001 control I/O port |
| **Pin Count** | 40 pins | 40 pins (6 are I/O port) |

### What's the Same

- Instruction set (all 56 opcodes)
- Internal architecture
- Addressing modes
- Register set (A, X, Y, SP, PC, P)
- Interrupt handling (IRQ, NMI, RESET)
- Clock timing

**Critical:** Code written for the 6502 runs on the 6510, but you must be aware that locations $0000 and $0001 behave differently.

---

## CPU Features

The 6510 provides these capabilities for assembly language programming:

### Processing Capabilities

- **8-bit parallel processing** - All operations are 8-bit
- **Decimal and binary arithmetic** - BCD mode via Status Register D flag
- **True indexing** - X and Y registers for indexed addressing
- **Programmable stack** - Stack pointer (SP) at $0100-$01FF
- **Variable length stack** - Grows downward from $01FF

### Memory and Bus

- **64KB address space** - $0000 to $FFFF
- **DMA capability** - Three-state address bus allows Direct Memory Access
- **Multiprocessor support** - Can share memory with other processors
- **M6800 bus compatible** - Can use 6800-family peripherals

### Architecture Features

- **Pipeline architecture** - Fetch next instruction while executing current
- **13 addressing modes** - Immediate, Zero Page, Absolute, Indexed, Indirect, etc.
- **Interrupt capability** - IRQ (maskable), NMI (non-maskable)
- **Speed flexibility** - Works with any speed memory (wait states)

---

## Architecture

### Register Set

The 6510 has six registers visible to the programmer:

| Register | Size | Symbol | Description |
|----------|------|--------|-------------|
| **Accumulator** | 8-bit | A | Primary data register for arithmetic/logic |
| **X Index** | 8-bit | X | Index register for addressing modes |
| **Y Index** | 8-bit | Y | Index register for addressing modes |
| **Stack Pointer** | 8-bit | SP | Points to current stack position ($0100-$01FF) |
| **Program Counter** | 16-bit | PC | Points to next instruction to execute |
| **Processor Status** | 8-bit | P | Flags: N V - B D I Z C |

### Status Register Flags (P)

```
Bit:  7   6   5   4   3   2   1   0
     N   V   -   B   D   I   Z   C
```

| Bit | Flag | Name | Set When |
|-----|------|------|----------|
| 7 | N | Negative | Result bit 7 = 1 |
| 6 | V | Overflow | Signed overflow occurred |
| 5 | - | (unused) | Always 1 |
| 4 | B | Break | BRK instruction executed |
| 3 | D | Decimal | Decimal mode enabled |
| 2 | I | Interrupt Disable | Interrupts disabled |
| 1 | Z | Zero | Result = 0 |
| 0 | C | Carry | Carry/borrow occurred |

### Internal Architecture

The 6510 contains these functional blocks:

1. **ALU** - Arithmetic Logic Unit for calculations
2. **Index Registers (X, Y)** - For indexed addressing
3. **Stack Pointer Register** - Manages stack at $0100-$01FF
4. **Accumulator** - Primary data register
5. **Input Data Latch** - Captures data from bus
6. **Instruction Register** - Holds current opcode
7. **Instruction Decode** - Interprets opcodes
8. **Timing Control** - Generates internal timing
9. **Address Bus Buffer** - Three-state 16-bit output
10. **Data Bus Buffer** - Three-state 8-bit bidirectional
11. **Processor Status Register** - Status flags
12. **PCL/PCH** - Program Counter Low/High bytes
13. **I/O Port** - 6-bit peripheral interface

---

## Pin Configuration

The 6510 is a 40-pin DIP (Dual Inline Package). Pin numbering:

```
        ┌───────────┐
  φ1 IN │1        40│ RES
    RDY │2        39│ φ2 IN
    IRQ │3        38│ R/W
    NMI │4        37│ DB₀
    AEC │5        36│ DB₁
    Vcc │6        35│ DB₂
     A₀ │7        34│ DB₃
     A₁ │8        33│ DB₄
     A₂ │9        32│ DB₅
     A₃ │10       31│ DB₆
     A₄ │11       30│ DB₇
     A₅ │12       29│ P₀
     A₆ │13       28│ P₁
     A₇ │14       27│ P₂
     A₈ │15       26│ P₃
     A₉ │16       25│ P₄
    A₁₀ │17       24│ P₅
    A₁₁ │18       23│ A₁₅
    A₁₂ │19       22│ A₁₄
    A₁₃ │20       21│ GND
        └───────────┘
```

### Pin Descriptions

| Pin | Name | Type | Description |
|-----|------|------|-------------|
| 1 | φ1 IN | Input | Phase 1 clock input |
| 2 | RDY | Input | Ready (stretches cycles for slow memory) |
| 3 | IRQ | Input | Interrupt Request (active low, maskable) |
| 4 | NMI | Input | Non-Maskable Interrupt (active low) |
| 5 | AEC | Output | Address Enable Control (DMA support) |
| 6 | Vcc | Power | +5V supply |
| 7-20 | A₀-A₁₃ | Output | Address Bus (lower 14 bits) |
| 21 | GND | Power | Ground (0V) |
| 22-23 | A₁₄-A₁₅ | Output | Address Bus (upper 2 bits) |
| 24-29 | P₀-P₅ | I/O | 6-bit I/O Port (controlled by $0000/$0001) |
| 30-37 | DB₀-DB₇ | I/O | 8-bit Data Bus (three-state) |
| 38 | R/W | Output | Read/Write (1=Read, 0=Write) |
| 39 | φ2 IN | Input | Phase 2 clock input |
| 40 | RES | Input | Reset (active low) |

### Critical Pins for C64 Programming

**You'll encounter these in assembly lessons:**

- **A₀-A₁₅** - Address lines you're reading/writing to
- **DB₀-DB₇** - Data lines carrying your values
- **R/W** - Whether you're reading or writing
- **IRQ/NMI** - Interrupt signals for advanced programming
- **P₀-P₅** - I/O port controlling memory banking

**Note:** Pins P₆ and P₇ don't exist on the physical chip. The "8-bit I/O port" refers to the register at $0001, which has 8 bits, but only 6 connect to external pins.

---

## I/O Port

The 6510's unique feature is its integrated 6-bit I/O port, controlled by memory locations $0000 and $0001.

### Port Control Registers

| Address | Name | Function |
|---------|------|----------|
| **$0000** | DDR | Data Direction Register (0=Input, 1=Output) |
| **$0001** | POR | Peripheral Output Register (data values) |

### C64 Port Usage

In the Commodore 64, the I/O port controls **memory banking**:

| Bit | Pin | C64 Function |
|-----|-----|--------------|
| 0 | P₀ | LORAM - Control BASIC ROM |
| 1 | P₁ | HIRAM - Control Kernal ROM |
| 2 | P₂ | CHAREN - Control Character ROM |
| 3 | P₃ | Cassette Data Output |
| 4 | P₄ | Cassette Switch Sense |
| 5 | P₅ | Cassette Motor Control |
| 6 | — | (No external pin - internal pullup) |
| 7 | — | (No external pin - internal pullup) |

### Memory Banking Example

```assembly
; Switch out BASIC ROM, keep Kernal visible
LDA $01      ; Read current port value
AND #%11111110  ; Clear bit 0 (LORAM=0)
ORA #%00000010  ; Set bit 1 (HIRAM=1)
STA $01      ; Write back to port

; Now $A000-$BFFF shows RAM instead of BASIC ROM
```

### Port Programming Pattern

```assembly
; 1. Set data direction (once at startup)
LDA #%00111111   ; Bits 0-5 = output, 6-7 = input
STA $00          ; Write to DDR

; 2. Write output values (whenever needed)
LDA #%00110111   ; Set bits for desired state
STA $01          ; Write to Port Register
```

**Critical:** The C64's Kernal initializes $00 to $2F and $01 to $37 at startup. Most programs leave $00 alone and only modify $01 for memory banking.

---

## Electrical Specifications

Understanding electrical specs helps debug hardware issues and timing-critical code.

### Power Requirements

| Parameter | Min | Typical | Max | Unit |
|-----------|-----|---------|-----|------|
| **Supply Voltage (Vcc)** | 4.75V | 5.0V | 5.25V | VDC |
| **Power Supply Current** | — | 125mA | — | mA |
| **Ground (Vss)** | 0V | 0V | 0V | VDC |

### Operating Conditions

| Parameter | Min | Max | Unit |
|-----------|-----|-----|------|
| **Operating Temperature** | 0°C | +70°C | °C |
| **Storage Temperature** | -55°C | +150°C | °C |

### Voltage Levels

#### Input Voltages

| Signal | Logic Low (0) | Logic High (1) |
|--------|---------------|----------------|
| **Clock (φ1, φ2)** | -0.3V to +0.2V | Vcc-0.2V to Vcc+1.0V |
| **Logic (RES, IRQ, Data)** | 0V to +0.8V | +2.0V to +5.0V |

#### Output Voltages

| Signal | Logic Low (0) | Logic High (1) | Load Condition |
|--------|---------------|----------------|----------------|
| **All Outputs** | ≤ 0.4V | ≥ 2.4V | IOL=1.6mA, IOH=-100µA |

### Current Specifications

| Parameter | Typical | Max | Unit | Notes |
|-----------|---------|-----|------|-------|
| **Input Leakage (Logic)** | — | 2.5µA | µA | Input pins at 0-5.25V |
| **Input Leakage (Clock)** | — | 100µA | µA | Clock inputs |
| **Three-State Leakage** | — | 10µA | µA | Data bus in high-Z state |

### Capacitance

At 25°C, 1 MHz, Vin = 0V:

| Signal | Typical | Max | Unit |
|--------|---------|-----|------|
| **Input (Logic, P₀-P₅)** | — | 10pF | pF |
| **Input (Data Bus)** | — | 15pF | pF |
| **Output (Address, R/W)** | — | 12pF | pF |
| **Clock φ1** | 30pF | 50pF | pF |
| **Clock φ2** | 50pF | 80pF | pF |

**Why This Matters:** Capacitance affects maximum clock speed and signal integrity. The higher clock input capacitance is why the C64 needs a stronger clock driver.

---

## Clock Timing

The 6510 uses a two-phase clock (φ1 and φ2) that must not overlap.

### Clock Cycle Structure

```
        ←────────── TCYC ──────────→

φ1 IN   ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁
        │  ←─ PWH(φ1) ─→            │
        ▔                            ▔
        ↑TD                          ↑TD
φ2 IN          ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁
               │←─PWH(φ2)─→│
        ▔▔▔▔▔▔▔            ▔▔▔▔▔▔▔▔▔▔
               ←─ TRWS ─→  ←THRW→

R/W     ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔
                   2.0V
        ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ←THA→

ADDRESS ▔════════════════════╳
FROM    │    2.0V
MPU          0.8V
        ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁
        ←─ TADS ─→ ←TAEW─→
                            ←TEDR→
DATA    ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔   2.0V
FROM                   ════════
MEMORY                 0.8V
        ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁   ▔▔▔▔
               ←─ TACC ─→ ←TDSU→ ←THR
```

### Timing Parameters

| Symbol | Parameter | Description |
|--------|-----------|-------------|
| TCYC | Cycle Time | Complete clock cycle duration |
| PWH(φ1) | Pulse Width High φ1 | How long φ1 stays high |
| PWH(φ2) | Pulse Width High φ2 | How long φ2 stays high |
| TD | Delay Time | Rise/fall time between clocks |
| TRWS | R/W Setup Time | R/W valid before φ2 rises |
| THRW | R/W Hold Time | R/W valid after φ2 falls |
| TADS | Address Setup | Address valid before φ2 rises |
| TAEW | Address Extension | Address valid during φ2 high |
| THA | Address Hold | Address valid after φ2 falls |
| TEDR | Enable Data Read | Data bus valid time |
| TACC | Access Time | Memory must respond within |
| TDSU | Data Setup | Data must be stable before φ2 falls |
| THR | Data Hold Read | Data hold after φ2 falls |

### Read Cycle Timing

**During a READ cycle (R/W = 1):**

1. **φ1 rises** - CPU begins internal operations
2. **φ1 falls** - Address becomes valid on bus
3. **φ2 rises** - R/W signal goes high, memory access begins
4. **φ2 high** - Memory must place data on bus
5. **φ2 falls** - CPU latches data from bus
6. **Cycle repeats**

### Write Cycle Timing

**During a WRITE cycle (R/W = 0):**

1. **φ1 rises** - CPU begins internal operations
2. **φ1 falls** - Address becomes valid on bus
3. **φ2 rises** - R/W signal goes low, data appears on bus
4. **φ2 high** - Memory must latch data
5. **φ2 falls** - Write cycle completes
6. **Cycle repeats**

### C64 Clock Speeds

| System | TCYC | Frequency | Notes |
|--------|------|-----------|-------|
| **NTSC** | ~978ns | ~1.023 MHz | North America, Japan |
| **PAL** | ~1015ns | ~0.985 MHz | Europe, Australia |
| **Spec** | 1000ns | 1.000 MHz | Rated specification |

**Why Different?** The VIC-II video chip generates the system clock, and it runs at different speeds for NTSC vs PAL video timing.

---

## Detailed Timing Specifications

This section provides precise timing values for interfacing memory and peripherals with the 6510. All timing is referenced to the two-phase clock signals φ1 and φ2.

### Clock Timing Parameters (1 MHz Operation)

**Vcc = 5V ±5%, Vss = 0V, TA = 0° to 70°C**

| Symbol | Parameter | Min | Typ | Max | Units |
|--------|-----------|-----|-----|-----|-------|
| **TCYC** | Cycle Time | 1000 | — | — | ns |
| **PWH(φ1)** | Clock Pulse Width (φ1 high) | 430 | — | — | ns |
| **PWH(φ2)** | Clock Pulse Width (φ2 high) | 470 | — | — | ns |
| **TF, TR** | Clock Fall Time, Rise Time (measured 0.2V to VCC - 0.2V) | — | — | 25 | ns |
| **TD** | Delay Time between Clocks (measured at 0.2V) | 0 | — | — | ns |

**Critical:** The clocks **must not overlap**. TD (delay time) minimum is 0ns, meaning φ1 must fall before φ2 rises, and φ2 must fall before φ1 rises.

### Clock Timing Parameters (2 MHz Operation)

**Vcc = 5V ±5%, Vss = 0V, TA = 0° to 70°C**

| Symbol | Parameter | Min | Typ | Max | Units |
|--------|-----------|-----|-----|-----|-------|
| **TCYC** | Cycle Time | 500 | — | — | ns |
| **PWH(φ1)** | Clock Pulse Width (φ1 high) | 215 | — | — | ns |
| **PWH(φ2)** | Clock Pulse Width (φ2 high) | 235 | — | — | ns |
| **TF, TR** | Clock Fall Time, Rise Time | — | — | 1.5 | ns |
| **TD** | Delay Time between Clocks | 0 | — | — | ns |

**Note:** While the 6510 is rated for 2 MHz operation, the C64 runs at ~1 MHz due to VIC-II video chip timing requirements and memory access speeds.

### Read/Write Timing Parameters (ILOAD = 1TTL)

| Symbol | Parameter | Min | Typ | Max | Units |
|--------|-----------|-----|-----|-----|-------|
| **TRWS** | Read/Write Setup Time from φ2 | — | 100 | 150 | ns |
| **TADS** | Address Setup Time from φ2 | — | 100 | 150 | ns |
| **TACC** | Memory Read Access Time | — | — | 575 | ns |

### Memory Read Timing (from φ2 falling edge)

| Symbol | Parameter | Min | Typ | Max | Units |
|--------|-----------|-----|-----|-----|-------|
| **TDSU** | Data Stability Time Period | 100 | — | — | ns |
| **THR** | Data Hold Time–Read | 0 | — | — | ns |
| **THW** | Data Hold Time–Write | 30 | — | 75 | ns |
| **TMDS** | Data Setup Time from 6510 | — | 10 | 150 | ns |
| **THA** | Address Hold Time | — | 10 | 30 | ns |
| **THRW** | R/W Hold Time | — | 10 | 30 | ns |

### Memory Write Timing

| Symbol | Parameter | Min | Typ | Max | Units |
|--------|-----------|-----|-----|-----|-------|
| **TAWS** | Delay Time, Address valid to R/W positive transition | 100 | — | — | ns |
| **TAEW** | Delay Time, φ2 positive transition to Data valid on bus | — | — | 180 | ns |
| **TEDR** | Delay Time, Data valid to positive transition on R/W (end of cycle) | — | — | 39.5 | ns |
| **TDSU** | Delay Time, R/W negative transition to φ2 positive transition | — | — | 300 | ns |
| **TWVE** | Delay Time, φ2 negative transition to Peripheral Data valid | — | — | 1 | µs |
| **TPDW** | Delay Time, φ2 negative transition to Peripheral Data valid | — | — | — | ns |
| **TPPSU** | Peripheral Data Setup Time | — | — | 300 | ns |
| **TAES** | Address Enable Setup Time | — | — | 60 | ns |

### What These Numbers Mean

**For Memory Interfacing:**
- **TACC (575ns max)** - Memory must respond within 575ns during a read cycle
- **TDSU (100ns min)** - Data must be stable for at least 100ns before φ2 falls
- **THR (0ns min)** - Data can change immediately after φ2 falls (but typically holds longer)

**For Address Timing:**
- **TADS (100-150ns typ)** - Address becomes valid 100-150ns before φ2 rises
- **THA (10-30ns typ)** - Address remains valid 10-30ns after φ2 falls

**For R/W Signal:**
- **TRWS (100-150ns typ)** - R/W signal becomes valid 100-150ns before φ2 rises
- **THRW (10-30ns typ)** - R/W remains valid 10-30ns after φ2 falls

### Practical Memory Timing Example

For a 1 MHz C64 with 1000ns clock cycle:

```
1. φ1 falls (start of cycle)
2. Address valid: ~100-150ns before φ2 rises
3. φ2 rises (memory access begins)
4. Memory access time: up to 575ns available
5. Data must be stable: 100ns before φ2 falls
6. φ2 falls (CPU latches data)
7. Data can change: immediately (0ns hold)
```

**Total available time for memory:** φ2 high period minus setup time
- φ2 high (1 MHz): ~470ns typical
- Less data setup: -100ns
- **Available:** ~370ns actual memory access window

**Why C64 uses slower RAM:** The C64 uses 200ns RAM chips, well within the 575ns maximum access time, providing good margin for signal degradation and temperature variation.

### Timing Margins and Design Considerations

**Conservative Design (C64 approach):**
- Use memory faster than minimum requirement
- Clock at ~1 MHz instead of 2 MHz maximum
- Provides margin for:
  - Temperature variations (0°C to 70°C range)
  - Component aging
  - Signal integrity issues (capacitance, noise)
  - Voltage fluctuations (±5% on 5V supply)

**Aggressive Design (not recommended):**
- Running at 2 MHz requires:
  - Very fast memory (100ns or better)
  - Excellent power supply regulation
  - Careful PCB layout (minimize capacitance)
  - Controlled temperature environment

---

## Signal Descriptions

This section details each signal pin's function and behavior for hardware interfacing and advanced programming.

### Clock Signals (φ1, φ2)

**Type:** Input (both pins)
**Voltage:** VCC level (5V ±5%)
**Requirement:** Two-phase, non-overlapping

The 6510 requires two clock inputs that must **never overlap** (one must be low before the other goes high).

**φ1 (Pin 1):**
- Input clock phase 1
- Goes high first in each cycle
- CPU performs internal operations during φ1 high
- Typical pulse width: 430ns (1 MHz), 215ns (2 MHz)

**φ2 (Pin 39):**
- Input clock phase 2
- Goes high after φ1 falls
- External bus activity occurs during φ2 high
- Memory reads/writes happen on φ2
- Typical pulse width: 470ns (1 MHz), 235ns (2 MHz)

**In the C64:**
The VIC-II video chip generates both φ1 and φ2 from the system master clock. This allows the VIC-II to "steal" cycles for video access (DMA).

```assembly
; You can't control φ1/φ2 from software, but you can:
; 1. Synchronize with φ2 using CIA timer
; 2. Wait for specific raster lines (sync with video)
; 3. Use cycle-counting for timing-critical code
```

### Address Bus (A0 - A15)

**Type:** Output (16 pins)
**Voltage:** TTL compatible (0V/5V)
**Drive Capability:** 1 standard TTL load + 130pF
**Pins:** 7-20, 22-23

The address bus specifies which memory location or I/O device the CPU wants to access.

**Behavior:**
- Valid during φ2 high
- Can change during φ1 high (don't use during φ1)
- Three-state capable (for DMA)
- Address setup: 100-150ns before φ2 rises
- Address hold: 10-30ns after φ2 falls

**Address Decoding Example:**

```assembly
; In hardware: Chip select logic decodes address bus
; A15 A14 A13 A12 = 1 1 0 1 → $Dxxx (I/O area)
; A11-A8 select specific chip (VIC, SID, CIA, etc.)

; In software: You just write to addresses
LDA $D020    ; VIC-II border color
             ; Hardware automatically decodes $D020
```

**DMA Consideration:**
When AEC goes low (VIC-II needs the bus), the address bus enters high-impedance (three-state) mode, allowing another device to drive the address lines.

### Data Bus (D0 - D7)

**Type:** Bidirectional I/O (8 pins)
**Voltage:** TTL compatible
**Drive Capability:** 1 standard TTL load + 130pF
**Pins:** 30-37

The data bus transfers values between CPU, memory, and peripherals.

**Read Cycle (R/W = 1):**
1. CPU places address on address bus
2. φ2 goes high
3. R/W goes high (read mode)
4. Data bus is in input (high-Z) mode
5. Memory/peripheral places data on bus
6. CPU latches data on φ2 falling edge
7. Data must be stable 100ns before φ2 falls

**Write Cycle (R/W = 0):**
1. CPU places address on address bus
2. φ2 goes high
3. R/W goes low (write mode)
4. CPU drives data bus with value to write
5. Data valid within 180ns of φ2 rising
6. Memory/peripheral latches data during φ2 high
7. CPU releases bus after φ2 falls

**Three-State Behavior:**
- During φ1 high: high-impedance (don't read!)
- During φ2 high (read): input mode (CPU listening)
- During φ2 high (write): output mode (CPU driving)
- During DMA (AEC low): high-impedance

**Assembly Programming:**

```assembly
; Reading data bus
LDA $C000    ; CPU reads from address $C000
             ; 1. Address $C000 appears on A0-A15
             ; 2. R/W goes high
             ; 3. Data from $C000 appears on D0-D7
             ; 4. CPU loads value into A register

; Writing data bus
STA $0400    ; CPU writes to address $0400
             ; 1. Address $0400 appears on A0-A15
             ; 2. R/W goes low
             ; 3. A register value appears on D0-D7
             ; 4. Memory latches the value
```

### Reset (RES)

**Type:** Input
**Pin:** 40
**Active:** Low (pull low to reset)
**Voltage:** TTL compatible

Resets or starts the microprocessor from power-down. This is the most important signal for system initialization.

**Reset Sequence:**

1. **Hold RES low:**
   - Writing to/from CPU is inhibited
   - R/W signal becomes invalid
   - After VCC reaches 4.75V, hold low for at least 2 clock cycles
   - After 2 cycles, R/W becomes valid again

2. **Release RES (rising edge detected):**
   - CPU begins reset sequence
   - 6 clock cycles initialization time
   - Interrupt disable flag (I) is set
   - Program Counter loaded from $FFFC-$FFFD (reset vector)
   - Execution begins at reset vector address

**Power-Up Reset:**
```
VCC reaches 4.75V
     ↓
Hold RES low for ≥ 2 clock cycles
     ↓
Release RES (let it go high)
     ↓
Wait 6 clock cycles (CPU initialization)
     ↓
CPU loads PC from $FFFC-$FFFD
     ↓
Program execution begins
```

**In the C64:**
- Reset vector at $FFFC-$FFFD points to $FCE2 (Kernal cold start)
- Pressing RESTORE key does NOT trigger RES (it triggers NMI)
- Power-on or hardware reset button triggers RES
- Cartridges can intercept reset by providing their own vector

**Software Cannot Trigger Reset:**
There's no instruction to pull RES low - it's hardware-only. But you can simulate a reset:

```assembly
; Simulate reset (not true hardware reset)
JMP ($FFFC)  ; Jump through reset vector

; True reset only via hardware:
; - Power cycle
; - Reset button
; - Watchdog circuit (if present)
```

### Interrupt Request (IRQ)

**Type:** Input
**Pin:** 3
**Active:** Low (pull low to interrupt)
**Voltage:** TTL compatible
**Maskable:** Yes (via I flag in status register)

Requests that an interrupt sequence begin. This is the primary mechanism for time-critical event handling.

**IRQ Sequence:**

1. **External device pulls IRQ low**
2. **CPU completes current instruction** (doesn't stop mid-instruction)
3. **CPU examines I flag** (interrupt disable) in status register
4. **If I = 0 (interrupts enabled):**
   - Push PCH (program counter high byte) to stack
   - Push PCL (program counter low byte) to stack
   - Push P (status register) to stack
   - Set I flag to 1 (disable further interrupts)
   - Load PC from $FFFE-$FFFF (IRQ vector)
   - Jump to interrupt handler
5. **If I = 1 (interrupts disabled):**
   - Ignore the IRQ
   - Continue normal execution

**IRQ Handler Pattern:**

```assembly
; Standard IRQ handler structure
IRQ_HANDLER:
    PHA           ; Save A register
    TXA
    PHA           ; Save X register
    TYA
    PHA           ; Save Y register

    ; Your interrupt code here
    LDA $D019     ; Example: acknowledge VIC-II raster IRQ
    STA $D019

    PLA
    TAY           ; Restore Y
    PLA
    TAX           ; Restore X
    PLA           ; Restore A
    RTI           ; Return from interrupt
                  ; (pulls P, then PC from stack)
```

**In the C64:**
- IRQ vector at $FFFE-$FFFF points to $EA31 (Kernal IRQ handler)
- Kernal handler jumps through RAM vector at $0314-$0315
- Default RAM vector: $EA81 (checks CIA, handles keyboard, etc.)
- To install custom IRQ: change $0314-$0315

**Common IRQ Sources:**
- CIA #1 Timer A/B interrupts
- CIA #2 Timer A/B interrupts
- VIC-II raster interrupts
- VIC-II sprite collisions
- VIC-II lightpen

**Enabling/Disabling:**

```assembly
SEI        ; Set I flag (disable IRQ)
           ; Critical sections go here
CLI        ; Clear I flag (enable IRQ)

; Check if IRQ would trigger:
BIT $DC0D  ; Read CIA #1 interrupt control register
           ; Sets N flag if IRQ pending
```

**Critical Timing:**
- IRQ latency: Current instruction completes first
- Worst case: 7-cycle instruction (e.g., ROR absolute,X)
- Stack overhead: 7 cycles to push PC and P
- Total worst-case latency: ~14 cycles (~14µs at 1 MHz)

### Non-Maskable Interrupt (NMI)

**Type:** Input
**Pin:** 4
**Active:** Low (pull low to interrupt)
**Voltage:** TTL compatible
**Maskable:** No (cannot be disabled)

Similar to IRQ but **cannot be masked** - it always interrupts (except during reset sequence).

**NMI vs IRQ:**

| Feature | NMI | IRQ |
|---------|-----|-----|
| **Can be disabled** | No | Yes (SEI/CLI) |
| **Vector** | $FFFA-$FFFB | $FFFE-$FFFF |
| **Priority** | Higher | Lower |
| **Use Case** | Critical events | Normal interrupts |
| **I flag checked** | No | Yes |

**NMI Sequence:**

1. **External device pulls NMI low**
2. **CPU detects falling edge** (edge-triggered, not level)
3. **CPU completes current instruction**
4. **Regardless of I flag:**
   - Push PCH, PCL, P to stack
   - **I flag is NOT set** (difference from IRQ!)
   - Load PC from $FFFA-$FFFB (NMI vector)
   - Jump to NMI handler

**In the C64:**
- NMI vector at $FFFA-$FFFB points to $FE43 (Kernal NMI handler)
- Kernal handler jumps through RAM vector at $0318-$0319
- Triggered by: RESTORE key (CIA #2 pin)
- Default action: Scan keyboard, check for RUN/STOP + RESTORE

**RESTORE Key Behavior:**

```assembly
; RESTORE key pulls NMI low
; Default Kernal NMI handler:
; 1. Checks if RUN/STOP also pressed
; 2. If yes: warm restart (like RUN/STOP-RESTORE)
; 3. If no: return (RTI)

; Custom NMI handler:
NMI_HANDLER:
    PHA
    ; Your code
    ; Note: I flag NOT automatically set!
    ; IRQs can still occur during NMI handler
    PLA
    RTI
```

**Edge-Triggered Behavior:**
- NMI triggers on **falling edge** (high-to-low transition)
- Holding NMI low won't re-trigger
- Must release and pull low again to re-trigger
- This prevents NMI storms

**Priority:**
If both NMI and IRQ occur simultaneously:
1. NMI is serviced first
2. After RTI from NMI, IRQ is then serviced (if I=0)

### Ready (RDY)

**Type:** Input
**Pin:** 2
**Voltage:** TTL compatible
**Function:** Stretches clock cycles for slow memory

Allows external devices to pause CPU execution by inserting wait states.

**How It Works:**

When RDY is pulled low:
- CPU will complete the current read cycle
- CPU will halt with address bus valid
- CPU will wait (insert wait states)
- CPU resumes when RDY goes high again

**Use Cases:**
1. **Slow memory access:** Give memory extra time to respond
2. **DMA preparation:** Hold CPU while setting up DMA transfer
3. **Synchronization:** Sync CPU with external events

**Limitations:**
- Only affects **read cycles**
- Write cycles cannot be stretched
- CPU must complete any write before RDY can halt it

**In the C64:**
The RDY signal is used during VIC-II DMA (badlines):
```
VIC-II needs to read screen memory
     ↓
VIC-II pulls RDY low at specific times
     ↓
CPU halts (if doing a read)
     ↓
VIC-II reads 40 bytes of screen data + color RAM
     ↓
VIC-II releases RDY (goes high)
     ↓
CPU resumes execution
```

**Cycle Stealing:**
On "bad lines" (when VIC-II fetches character data):
- CPU loses ~40 cycles per raster line
- Happens 200 times per frame (PAL) or 234 times (NTSC)
- Total CPU "theft": ~8000 cycles per frame
- This is why raster code runs slower on badlines

### Address Enable Control (AEC)

**Type:** Output
**Pin:** 5
**Voltage:** TTL compatible
**Function:** Signals when address bus is valid for DMA

Indicates when external devices can safely use the system bus for Direct Memory Access.

**States:**

| AEC | Meaning | Address Bus | Data Bus |
|-----|---------|-------------|----------|
| **High (1)** | CPU active | Valid (CPU driving) | Active |
| **Low (0)** | CPU halted | High-Z (three-state) | High-Z |

**DMA Sequence:**

1. External device needs bus access
2. Device pulls RDY low (halts CPU)
3. CPU completes current cycle
4. CPU puts address and data bus in high-Z mode
5. **AEC goes low** (signals "bus is yours")
6. External device can drive address/data buses
7. External device releases RDY
8. **AEC goes high** (CPU takes bus back)
9. CPU resumes execution

**In the C64:**
VIC-II uses AEC during video access:
- AEC low when VIC-II accesses memory
- AEC high when CPU can access memory
- This coordinates VIC-II and CPU memory sharing

**For Cartridge/Hardware Designers:**
- Monitor AEC to know when bus is available
- Don't drive bus when AEC is high (CPU owns it)
- Safe to drive bus when AEC is low

### Read/Write (R/W)

**Type:** Output
**Pin:** 38
**Voltage:** TTL compatible
**Function:** Indicates read (1) or write (0) operation

Controls the direction of data flow on the data bus.

**States:**

| R/W | Operation | Data Bus Direction |
|-----|-----------|-------------------|
| **1 (High)** | Read | Memory → CPU (CPU is listening) |
| **0 (Low)** | Write | CPU → Memory (CPU is driving) |

**Timing:**
- R/W setup time: 100-150ns before φ2 rises
- R/W hold time: 10-30ns after φ2 falls
- Valid throughout φ2 high period

**Memory Chip Usage:**

For RAM chips (2114, 6116, etc.):
```
R/W ─→ WE (Write Enable) on RAM chip
     (Often inverted: R/W = 0 means WE = 1)
```

For ROM chips:
```
R/W is ignored (ROM never writes)
OE (Output Enable) controlled by φ2 and chip select
```

**Example Memory Interface:**

```
Address Bus → Address pins of RAM
Data Bus ↔ Data pins of RAM
R/W → WE (inverted)
φ2 + ChipSelect → CE (Chip Enable)
```

**In Assembly:**

```assembly
LDA $1000   ; R/W = 1 (read from $1000)
STA $1000   ; R/W = 0 (write to $1000)
INC $1000   ; R/W = 1 then 0 (read-modify-write)

; Read-Modify-Write instructions use BOTH:
; 1. Read cycle (R/W = 1): fetch current value
; 2. Write cycle (R/W = 0): store modified value
```

**Don't Rely On R/W for I/O Timing:**
Some programmers try to use R/W transitions for timing - this is unreliable. Use CIA timers instead.

### I/O Port Pins (P0 - P5)

**Type:** Bidirectional I/O (6 pins)
**Pins:** 24-29
**Voltage:** TTL compatible
**Control:** Via $0000 (DDR) and $0001 (Data)

These are the 6-bit general-purpose I/O port unique to the 6510.

**Per-Bit Control:**

Each bit is individually programmable as input or output via the Data Direction Register (DDR) at $0000:

```assembly
; Set DDR (Data Direction Register at $0000)
LDA #%00111111   ; Bits 0-5: 1=output, 0=input
STA $00          ; (Bits 6-7 don't connect to pins)

; Write output values (POR at $0001)
LDA #%00110111   ; Set output bit values
STA $01          ; Only affects bits set as outputs in DDR
```

**Bit Behavior:**

| DDR Bit | Pin Mode | $0001 Read | $0001 Write |
|---------|----------|------------|-------------|
| **0** | Input | Reads pin state | No effect |
| **1** | Output | Reads last written value | Drives pin |

**In the C64 - Memory Banking:**

| Bit | Pin | C64 Function | Typical State |
|-----|-----|--------------|---------------|
| 0 | P₀ | LORAM (BASIC ROM enable) | 1 (ROM visible) |
| 1 | P₁ | HIRAM (Kernal ROM enable) | 1 (ROM visible) |
| 2 | P₂ | CHAREN (Char ROM/I/O select) | 1 (I/O visible) |
| 3 | P₃ | Cassette Data Output | Varies |
| 4 | P₄ | Cassette Switch Sense (input) | Input |
| 5 | P₅ | Cassette Motor Control | 0 (motor off) |

**Memory Banking Examples:**

```assembly
; Default: $01 = $37 = %00110111
; All ROMs visible, I/O accessible

; Switch out BASIC ROM, keep Kernal
LDA $01
AND #%11111110   ; Clear bit 0 (LORAM=0)
STA $01
; Now $A000-$BFFF shows RAM

; Switch out ALL ROMs (RAM only)
LDA $01
AND #%11111000   ; Clear bits 0,1,2
STA $01
; Now $A000-$BFFF, $D000-$DFFF, $E000-$FFFF are RAM

; Restore default
LDA #$37
STA $01
```

**Critical Warnings:**

1. **Don't bank out Kernal while using it:**
```assembly
JSR $FFD2   ; Kernal routine
LDA #$30
STA $01     ; ← CRASH! Just banked out Kernal mid-routine
```

2. **Always save/restore $01:**
```assembly
; Good practice
LDA $01     ; Save current value
PHA
AND #$FE    ; Modify
STA $01
; ... your code ...
PLA
STA $01     ; Restore original
```

3. **Bits 6-7 don't have pins:**
```assembly
; $01 bits 6-7 are internal pull-ups
; Always read as 1 unless external hardware pulls low
; Generally safe to leave as 1
```

---

## Addressing Modes

The 6510 supports 13 addressing modes that determine how the CPU calculates the effective address for an instruction's operand. Understanding these modes is essential for efficient assembly programming.

### Overview of Addressing Modes

| Mode | Notation | Bytes | Description |
|------|----------|-------|-------------|
| **Implied** | `INX` | 1 | Operand implied by instruction |
| **Accumulator** | `ASL A` | 1 | Operates on accumulator |
| **Immediate** | `LDA #$42` | 2 | Operand is the next byte |
| **Zero Page** | `LDA $80` | 2 | Address in page zero ($00xx) |
| **Zero Page,X** | `LDA $80,X` | 2 | Zero page + X register |
| **Zero Page,Y** | `LDX $80,Y` | 2 | Zero page + Y register |
| **Absolute** | `LDA $C000` | 3 | Full 16-bit address |
| **Absolute,X** | `LDA $C000,X` | 3 | Absolute + X register |
| **Absolute,Y** | `LDA $C000,Y` | 3 | Absolute + Y register |
| **Relative** | `BNE label` | 2 | Offset for branch (-128 to +127) |
| **(Indirect,X)** | `LDA ($40,X)` | 2 | Indexed indirect |
| **(Indirect),Y** | `LDA ($40),Y` | 2 | Indirect indexed |
| **Absolute Indirect** | `JMP ($C000)` | 3 | Only for JMP instruction |

### 1. Implied Addressing

**Format:** Single-byte instruction
**Example:** `INX`, `CLC`, `RTS`

The operand is implied by the operation itself. No address calculation needed.

```assembly
CLC         ; Clear carry flag (implied)
INX         ; Increment X register (implied)
RTS         ; Return from subroutine (implied)
NOP         ; No operation (implied)
```

**Why It's Useful:**
- Smallest possible instruction (1 byte)
- Fastest execution (2 cycles typical)
- Used for register operations and flag manipulation

### 2. Accumulator Addressing

**Format:** Single-byte instruction operating on accumulator
**Example:** `ASL A`, `LSR A`, `ROL A`

Similar to implied, but explicitly operates on the accumulator register.

```assembly
ASL A       ; Arithmetic shift left accumulator
LSR A       ; Logical shift right accumulator
ROL A       ; Rotate left accumulator
ROR A       ; Rotate right accumulator
```

**Note:** The "A" is often written but not required by assemblers (e.g., `ASL` and `ASL A` are equivalent).

**Why It's Useful:**
- Fast bit manipulation (2 cycles)
- Common for multiply/divide by powers of 2

### 3. Immediate Addressing

**Format:** `LDA #value`
**Bytes:** 2 (opcode + value)
**Notation:** Hash sign (#) indicates immediate mode

The operand is the byte immediately following the opcode.

```assembly
LDA #$42    ; Load accumulator with literal value $42
LDX #$00    ; Load X with 0
CPY #$10    ; Compare Y with 16
AND #%00001111  ; Mask lower 4 bits
```

**Why It's Useful:**
- Loading constant values
- Fast (2 cycles)
- Common for initialization

**Common Mistake:**
```assembly
LDA $42     ; ← Loads FROM address $42 (zero page)
LDA #$42    ; ✓ Loads the VALUE $42
```

### 4. Zero Page Addressing

**Format:** `LDA $nn` (where nn = $00-$FF)
**Bytes:** 2 (opcode + zero page address)
**Effective Address:** $00nn

Addresses the first 256 bytes of memory ($0000-$00FF).

```assembly
LDA $80     ; Load from address $0080
STA $FB     ; Store to address $00FB
INC $20     ; Increment memory at $0020
```

**Why It's Useful:**
- Shorter than absolute (2 bytes vs 3)
- Faster than absolute (3 cycles vs 4 for LDA)
- The C64 uses zero page extensively for Kernal/BASIC variables

**Critical Locations in C64:**
- `$00-$01` - CPU I/O port (memory banking)
- `$02-$8F` - Kernal and BASIC working storage
- `$90-$FF` - Available for user programs (but be careful!)

### 5. Indexed Zero Page Addressing (Zero Page,X / Zero Page,Y)

**Format:** `LDA $nn,X` or `LDX $nn,Y`
**Bytes:** 2 (opcode + base address)
**Effective Address:** ($nn + register) AND $FF (wraps within page zero)

Adds index register to zero page base address, wrapping at page boundary.

```assembly
LDX #$05
LDA $80,X   ; Loads from $0085 ($80 + $05)

LDY #$10
LDX $40,Y   ; Loads from $0050 ($40 + $10)
```

**Important:** Zero page indexed addresses **wrap around** within page zero:

```assembly
LDX #$90
LDA $80,X   ; Effective address: $0010 (not $0110!)
            ; Calculation: ($80 + $90) AND $FF = $10
```

**Why It's Useful:**
- Fast array/table access in zero page
- 4 cycles (vs 4-5 for absolute indexed)
- Common for sprite data pointers, lookup tables

**Which Register?**
- Most instructions use X: `LDA $80,X`, `STA $80,X`
- Only `LDX` and `STX` use Y: `LDX $80,Y`, `STX $80,Y`

### 6. Absolute Addressing

**Format:** `LDA $nnnn`
**Bytes:** 3 (opcode + low byte + high byte)
**Effective Address:** $nnnn (full 16-bit address)

Accesses any location in the 64K memory space.

```assembly
LDA $C000   ; Load from address $C000
STA $0400   ; Store to screen memory
JMP $FFCE   ; Jump to address $FFCE
```

**Byte Order (Little-Endian):**
```assembly
LDA $C000   ; Assembled as: AD 00 C0
            ; Opcode: $AD
            ; Low byte: $00
            ; High byte: $C0
```

**Why It's Useful:**
- Access entire memory map
- Required for addresses > $FF
- Standard mode for most operations

### 7. Indexed Absolute Addressing (Absolute,X / Absolute,Y)

**Format:** `LDA $nnnn,X` or `LDA $nnnn,Y`
**Bytes:** 3 (opcode + low byte + high byte)
**Effective Address:** $nnnn + register (full 16-bit addition with carry)

Adds index register to 16-bit base address.

```assembly
LDX #$05
LDA $C000,X ; Loads from $C005

LDY #$28
STA $0400,Y ; Stores to $0428 (screen memory row 1)
```

**Page Boundary Crossing:**
If the addition crosses a page boundary, the instruction takes an extra cycle:

```assembly
LDX #$10
LDA $C0F0,X ; Effective: $C100 (crossed from $C0 to $C1 page)
            ; Takes 5 cycles instead of 4
```

**Why It's Useful:**
- Arrays and tables anywhere in memory
- Screen memory manipulation (40-column rows)
- Sprite data access

**Practical Example - Screen Memory:**
```assembly
; Clear screen row (Y = row number 0-24)
LDX #$00
LDA #$20    ; Space character
LOOP:
  STA $0400,X   ; Store to screen + X offset
  INX
  CPX #$28      ; 40 columns
  BNE LOOP
```

### 8. Relative Addressing

**Format:** `BNE label`
**Bytes:** 2 (opcode + signed offset)
**Effective Address:** PC + offset (-128 to +127 bytes)

Used **only** with branch instructions. The second byte is a signed offset added to the Program Counter.

```assembly
      LDA $C000
      CMP #$42
      BEQ FOUND    ; If equal, branch to FOUND
      JMP NOTFOUND
FOUND:
      ; Offset calculated by assembler
      ; If FOUND is 10 bytes ahead: offset = $0A
      ; If FOUND is 5 bytes back: offset = $FB (-5)
```

**Offset Range:**
- Forward: 0 to 127 bytes (+$00 to +$7F)
- Backward: -128 to -1 bytes ($80 to $FF)

**Branch Instructions (all use relative):**
```assembly
BCC  ; Branch if Carry Clear
BCS  ; Branch if Carry Set
BEQ  ; Branch if Equal (Z=1)
BNE  ; Branch if Not Equal (Z=0)
BMI  ; Branch if Minus (N=1)
BPL  ; Branch if Plus (N=0)
BVC  ; Branch if Overflow Clear
BVS  ; Branch if Overflow Set
```

**Why It's Useful:**
- Position-independent code
- Short branches are fast (2-3 cycles)
- Assembler calculates offset automatically

**Common Issue - Out of Range:**
```assembly
      BEQ DISTANT ; ← Error if DISTANT > 127 bytes away
      ; Solution: Use opposite branch + JMP
      BNE SKIP
      JMP DISTANT
SKIP:
```

### 9. Indexed Indirect Addressing (Indirect,X)

**Format:** `LDA ($nn,X)`
**Bytes:** 2 (opcode + zero page base)
**Effective Address:** Read from ($nn + X) and ($nn + X + 1) in zero page

The X register is added to the zero page address (wrapping at $FF), then the resulting address is used to fetch a 16-bit pointer from zero page.

**Step-by-Step:**
```assembly
LDA ($40,X)  ; X = $05

1. Add X to base: $40 + $05 = $45 (wraps in zero page)
2. Read low byte from $0045: e.g., $00
3. Read high byte from $0046: e.g., $C0
4. Effective address: $C000
5. Load accumulator from $C000
```

**Memory Layout Example:**
```
$0045: $00  ← Low byte of pointer
$0046: $C0  ← High byte of pointer
        ↓
Points to $C000 (actual data location)
```

**Code Example:**
```assembly
; Setup pointer table at $40-$41
LDA #$00
STA $40     ; Low byte of pointer
LDA #$C0
STA $41     ; High byte of pointer

; Access via indexed indirect
LDX #$00
LDA ($40,X) ; Loads from address stored at $40-$41 ($C000)
```

**Why It's Useful:**
- Table of pointers to data structures
- Jump tables
- Indirect addressing with X as table index

**Common Pattern:**
```assembly
; Table of 16-bit addresses
PTRS:
  .WORD $C000  ; Pointer 0 at $40-$41
  .WORD $C100  ; Pointer 1 at $42-$43
  .WORD $C200  ; Pointer 2 at $44-$45

; Access pointer N
LDX N        ; N = pointer number * 2
LDA ($40,X)  ; Load via pointer
```

### 10. Indirect Indexed Addressing (Indirect),Y

**Format:** `LDA ($nn),Y`
**Bytes:** 2 (opcode + zero page address)
**Effective Address:** (Read 16-bit pointer from $nn) + Y

Read a 16-bit pointer from zero page, then add Y register to that address.

**Step-by-Step:**
```assembly
LDA ($40),Y  ; Y = $05

1. Read low byte from $0040: e.g., $00
2. Read high byte from $0041: e.g., $C0
3. Form base address: $C000
4. Add Y register: $C000 + $05 = $C005
5. Load accumulator from $C005
```

**Memory Layout Example:**
```
$0040: $00  ← Low byte of base pointer
$0041: $C0  ← High byte of base pointer
        ↓
Base address $C000 + Y = effective address
```

**Code Example:**
```assembly
; Setup base pointer
LDA #$00
STA $FB     ; Low byte
LDA #$04    ; Screen memory
STA $FC     ; High byte

; Access 40-byte row via Y offset
LDY #$00
LDA #$20    ; Space character
LOOP:
  STA ($FB),Y  ; Store to ($0400) + Y
  INY
  CPY #$28     ; 40 columns
  BNE LOOP
```

**Why It's Useful:**
- Fast array/string processing
- Screen memory manipulation
- Most common indirect mode in C64 programming

**Practical Example - Text Output:**
```assembly
; Print string via pointer
; Pointer at $FB-$FC, Y = character index

      LDY #$00
PRINT:
      LDA ($FB),Y  ; Get character from string
      BEQ DONE     ; $00 = end of string
      JSR $FFD2    ; Kernal CHROUT
      INY
      BNE PRINT    ; Loop (max 256 chars)
DONE:
      RTS
```

### 11. Absolute Indirect Addressing (Indirect)

**Format:** `JMP ($nnnn)`
**Bytes:** 3 (opcode + low byte + high byte)
**Effective Address:** Read 16-bit address from $nnnn
**Only Available For:** `JMP` instruction

The address specifies where to read the actual jump address from.

```assembly
JMP ($C000)  ; Jump to address stored at $C000-$C001
```

**Step-by-Step:**
```assembly
JMP ($0314)  ; IRQ vector

1. Read low byte from $0314: e.g., $81
2. Read high byte from $0315: e.g., $EA
3. Jump to $EA81
```

**Famous 6502 Bug - Page Boundary:**
If the low byte of the indirect address is $FF, the high byte wraps to $xx00 instead of $xx+1$00:

```assembly
JMP ($C0FF)  ; Intended: read from $C0FF and $C100
             ; ACTUAL: reads from $C0FF and $C000!
             ; Bug in original 6502, kept for compatibility
```

**Workaround:**
```assembly
; Don't put indirect vectors at $xxFF
; C64 vectors are safe:
; $0314-$0315 (IRQ)
; $0318-$0319 (NMI)
; $FFFA-$FFFB, $FFFC-$FFFD, $FFFE-$FFFF (hardware vectors)
```

**Why It's Useful:**
- Kernal ROM uses this for vectored jumps
- Allows changing system behavior by updating vectors in RAM

**Common Usage - Custom IRQ:**
```assembly
; Install custom IRQ handler
SEI              ; Disable interrupts
LDA #<MYIRQ      ; Low byte of handler
STA $0314
LDA #>MYIRQ      ; High byte of handler
STA $0315
CLI              ; Enable interrupts

; System now does: JMP ($0314) when IRQ occurs
```

### Addressing Mode Quick Reference

| Mode | Example | Effective Address | Cycles* | Bytes |
|------|---------|------------------|---------|-------|
| **Implied** | `TAX` | (none) | 2 | 1 |
| **Accumulator** | `ASL A` | A register | 2 | 1 |
| **Immediate** | `LDA #$42` | $42 (literal) | 2 | 2 |
| **Zero Page** | `LDA $80` | $0080 | 3 | 2 |
| **Zero Page,X** | `LDA $80,X` | $0080 + X (wrap) | 4 | 2 |
| **Zero Page,Y** | `LDX $80,Y` | $0080 + Y (wrap) | 4 | 2 |
| **Absolute** | `LDA $C000` | $C000 | 4 | 3 |
| **Absolute,X** | `LDA $C000,X` | $C000 + X | 4-5† | 3 |
| **Absolute,Y** | `LDA $C000,Y` | $C000 + Y | 4-5† | 3 |
| **Relative** | `BNE label` | PC + offset | 2-3‡ | 2 |
| **(Indirect,X)** | `LDA ($40,X)` | word at ($40+X) | 6 | 2 |
| **(Indirect),Y** | `LDA ($40),Y` | word at ($40)+Y | 5-6† | 2 |
| **Absolute Indirect** | `JMP ($C000)` | word at $C000 | 5 | 3 |

*Cycles for LDA instruction (varies by instruction)
†Add 1 cycle if page boundary crossed
‡Add 1 cycle if branch taken, +1 more if page boundary crossed

### Choosing the Right Addressing Mode

**For constants:**
```assembly
LDA #$42    ; Immediate (fastest, clearest)
```

**For frequently-accessed variables:**
```assembly
LDA $80     ; Zero Page (fast, small)
```

**For arrays/tables in zero page:**
```assembly
LDA $80,X   ; Zero Page,X (fast iteration)
```

**For large data anywhere in memory:**
```assembly
LDA $C000,Y ; Absolute,Y (flexible, common)
```

**For pointers/indirect access:**
```assembly
LDA ($FB),Y ; Indirect,Y (most flexible)
```

**For loops:**
```assembly
BNE LOOP    ; Relative (position-independent)
```

---

## Instruction Set

The 6510 has 56 official instructions organized into functional groups. Each instruction may support multiple addressing modes.

### Instruction Categories

1. **Load/Store** - Move data between registers and memory
2. **Transfer** - Move data between registers
3. **Stack** - Push/pull data to/from stack
4. **Arithmetic** - Add, subtract, increment, decrement
5. **Logical** - AND, OR, XOR operations
6. **Shift/Rotate** - Bit manipulation
7. **Compare** - Test values without changing them
8. **Branch** - Conditional jumps (short range)
9. **Jump/Subroutine** - Unconditional jumps and calls
10. **Flags** - Set/clear status register bits
11. **System** - Break, no-op, return

### Complete Instruction List (Alphabetical)

| Mnemonic | Description | Flags Affected |
|----------|-------------|----------------|
| **ADC** | Add Memory to Accumulator with Carry | N V Z C |
| **AND** | AND Memory with Accumulator | N Z |
| **ASL** | Shift Left One Bit (Memory or Accumulator) | N Z C |
| **BCC** | Branch on Carry Clear | - |
| **BCS** | Branch on Carry Set | - |
| **BEQ** | Branch on Result Zero | - |
| **BIT** | Test Bits in Memory with Accumulator | N V Z |
| **BMI** | Branch on Result Minus | - |
| **BNE** | Branch on Result Not Zero | - |
| **BPL** | Branch on Result Plus | - |
| **BRK** | Force Break | B I |
| **BVC** | Branch on Overflow Clear | - |
| **BVS** | Branch on Overflow Set | - |
| **CLC** | Clear Carry Flag | C |
| **CLD** | Clear Decimal Mode | D |
| **CLI** | Clear Interrupt Disable Bit | I |
| **CLV** | Clear Overflow Flag | V |
| **CMP** | Compare Memory and Accumulator | N Z C |
| **CPX** | Compare Memory and Index X | N Z C |
| **CPY** | Compare Memory and Index Y | N Z C |
| **DEC** | Decrement Memory by One | N Z |
| **DEX** | Decrement Index X by One | N Z |
| **DEY** | Decrement Index Y by One | N Z |
| **EOR** | Exclusive-OR Memory with Accumulator | N Z |
| **INC** | Increment Memory by One | N Z |
| **INX** | Increment Index X by One | N Z |
| **INY** | Increment Index Y by One | N Z |
| **JMP** | Jump to New Location | - |
| **JSR** | Jump to Subroutine (save return address) | - |
| **LDA** | Load Accumulator with Memory | N Z |
| **LDX** | Load Index X with Memory | N Z |
| **LDY** | Load Index Y with Memory | N Z |
| **LSR** | Shift One Bit Right (Memory or Accumulator) | N Z C |
| **NOP** | No Operation | - |
| **ORA** | OR Memory with Accumulator | N Z |
| **PHA** | Push Accumulator on Stack | - |
| **PHP** | Push Processor Status on Stack | - |
| **PLA** | Pull Accumulator from Stack | N Z |
| **PLP** | Pull Processor Status from Stack | All |
| **ROL** | Rotate One Bit Left (Memory or Accumulator) | N Z C |
| **ROR** | Rotate One Bit Right (Memory or Accumulator) | N Z C |
| **RTI** | Return from Interrupt | All |
| **RTS** | Return from Subroutine | - |
| **SBC** | Subtract Memory from Accumulator with Borrow | N V Z C |
| **SEC** | Set Carry Flag | C |
| **SED** | Set Decimal Mode | D |
| **SEI** | Set Interrupt Disable Status | I |
| **STA** | Store Accumulator in Memory | - |
| **STX** | Store Index X in Memory | - |
| **STY** | Store Index Y in Memory | - |
| **TAX** | Transfer Accumulator to Index X | N Z |
| **TAY** | Transfer Accumulator to Index Y | N Z |
| **TSX** | Transfer Stack Pointer to Index X | N Z |
| **TXA** | Transfer Index X to Accumulator | N Z |
| **TXS** | Transfer Index X to Stack Pointer | - |
| **TYA** | Transfer Index Y to Accumulator | N Z |

### Instruction Groups by Function

#### Load/Store Operations

```assembly
LDA #$42    ; Load accumulator (immediate)
LDA $80     ; Load accumulator (zero page)
LDA $C000   ; Load accumulator (absolute)
LDX #$00    ; Load X register
LDY #$10    ; Load Y register

STA $0400   ; Store accumulator
STX $80     ; Store X register
STY $81     ; Store Y register
```

**Available Addressing Modes:**
- LDA: Immediate, Zero Page, Zero Page,X, Absolute, Absolute,X, Absolute,Y, (Indirect,X), (Indirect),Y
- LDX: Immediate, Zero Page, Zero Page,Y, Absolute, Absolute,Y
- LDY: Immediate, Zero Page, Zero Page,X, Absolute, Absolute,X
- STA: Zero Page, Zero Page,X, Absolute, Absolute,X, Absolute,Y, (Indirect,X), (Indirect),Y
- STX: Zero Page, Zero Page,Y, Absolute
- STY: Zero Page, Zero Page,X, Absolute

#### Transfer Operations

```assembly
TAX         ; Transfer A to X
TAY         ; Transfer A to Y
TXA         ; Transfer X to A
TYA         ; Transfer Y to A
TSX         ; Transfer Stack Pointer to X
TXS         ; Transfer X to Stack Pointer
```

**All transfers:**
- Single byte (implied addressing)
- 2 cycles
- Affect N and Z flags (except TXS)

#### Stack Operations

```assembly
PHA         ; Push accumulator to stack
PHP         ; Push processor status to stack
PLA         ; Pull accumulator from stack
PLP         ; Pull processor status from stack
```

**Stack behavior:**
- Stack at $0100-$01FF
- Stack Pointer (SP) starts at $FF (stack at $01FF)
- SP decrements when pushing (descending stack)
- SP increments when pulling

#### Arithmetic Operations

```assembly
ADC #$10    ; Add with carry: A = A + $10 + C
SBC #$05    ; Subtract with borrow: A = A - $05 - (1-C)

INC $80     ; Increment memory
DEC $80     ; Decrement memory
INX         ; Increment X
INY         ; Increment Y
DEX         ; Decrement X
DEY         ; Decrement Y
```

**Important:**
- ADC and SBC use carry flag
- Always use SEC before SBC for correct subtraction
- Always use CLC before ADC if you don't want carry
- Set D flag (SED) for decimal mode (BCD arithmetic)

#### Logical Operations

```assembly
AND #%11110000  ; Bitwise AND: A = A AND $F0
ORA #%00001111  ; Bitwise OR: A = A OR $0F
EOR #%11111111  ; Bitwise XOR: A = A XOR $FF
```

**Common uses:**
- AND: Mask/clear bits, test bits
- ORA: Set bits
- EOR: Toggle bits, simple encryption

#### Shift and Rotate

```assembly
ASL A       ; Arithmetic Shift Left: C←76543210←0
LSR A       ; Logical Shift Right: 0→76543210→C
ROL A       ; Rotate Left: C←76543210←C
ROR A       ; Rotate Right: C→76543210→C

ASL $80     ; Can also operate on memory
LSR $C000,X ; With various addressing modes
```

**Uses:**
- ASL: Multiply by 2
- LSR: Divide by 2 (unsigned)
- ROL: Rotate through carry for multi-byte shifts
- ROR: Rotate through carry

#### Compare Operations

```assembly
CMP #$42    ; Compare A with $42 (sets flags)
CPX #$00    ; Compare X with $00
CPY #$10    ; Compare Y with $10
```

**Flag results:**
- Z=1 if equal
- C=1 if register ≥ memory
- N=1 if result negative (bit 7 set)

**Common pattern:**
```assembly
CMP #$42
BEQ EQUAL   ; Branch if A = $42
BCS HIGHER  ; Branch if A ≥ $42
BCC LOWER   ; Branch if A < $42
```

#### Branch Instructions

All branches use relative addressing (-128 to +127 bytes):

```assembly
BCC label   ; Branch if Carry Clear (C=0)
BCS label   ; Branch if Carry Set (C=1)
BEQ label   ; Branch if Equal/Zero (Z=1)
BNE label   ; Branch if Not Equal (Z=0)
BMI label   ; Branch if Minus/Negative (N=1)
BPL label   ; Branch if Plus/Positive (N=0)
BVC label   ; Branch if Overflow Clear (V=0)
BVS label   ; Branch if Overflow Set (V=1)
```

**Timing:**
- 2 cycles if branch not taken
- 3 cycles if branch taken (same page)
- 4 cycles if branch taken (different page)

#### Jump and Subroutine

```assembly
JMP $C000       ; Unconditional jump (absolute)
JMP ($0314)     ; Jump indirect (via pointer)

JSR $FFD2       ; Call subroutine (pushes return address)
RTS             ; Return from subroutine (pulls return address)
```

**JSR/RTS behavior:**
```assembly
JSR MYSUBR      ; 1. Push PCH to stack
                ; 2. Push PCL to stack
                ; 3. Jump to MYSUBR

MYSUBR:
  ; ... subroutine code ...
  RTS           ; 1. Pull PCL from stack
                ; 2. Pull PCH from stack
                ; 3. Increment PC
                ; 4. Continue execution
```

**Nesting limit:** 128 levels (stack is 256 bytes, uses 2 bytes per call)

#### Flag Operations

```assembly
; Carry flag
CLC         ; Clear Carry (C=0)
SEC         ; Set Carry (C=1)

; Interrupt disable
CLI         ; Clear Interrupt disable (enable IRQ)
SEI         ; Set Interrupt disable (disable IRQ)

; Decimal mode
CLD         ; Clear Decimal mode (binary arithmetic)
SED         ; Set Decimal mode (BCD arithmetic)

; Overflow
CLV         ; Clear Overflow flag (V=0)
; (No SEV - use BIT instruction or PLP)
```

#### System Operations

```assembly
BRK         ; Software interrupt (break)
NOP         ; No operation (2 cycles, 1 byte)
RTI         ; Return from interrupt
```

**BRK behavior:**
```assembly
BRK         ; 1. Increment PC
            ; 2. Push PCH to stack
            ; 3. Push PCL to stack
            ; 4. Push P to stack (with B flag set)
            ; 5. Set I flag (disable IRQ)
            ; 6. Load PC from $FFFE-$FFFF
```

### Common Instruction Patterns

**Copy byte from one location to another:**
```assembly
LDA $C000   ; Load from source
STA $C100   ; Store to destination
```

**Clear memory location:**
```assembly
LDA #$00
STA $0400
```

**Set bits:**
```assembly
LDA $D020   ; Read current value
ORA #$01    ; Set bit 0
STA $D020   ; Write back
```

**Clear bits:**
```assembly
LDA $D020   ; Read current value
AND #$FE    ; Clear bit 0
STA $D020   ; Write back
```

**Toggle bits:**
```assembly
LDA $D020   ; Read current value
EOR #$01    ; Toggle bit 0
STA $D020   ; Write back
```

**Test bit:**
```assembly
BIT $DC0D   ; Test CIA register
BMI IRQPEND ; Branch if bit 7 set
```

**16-bit addition:**
```assembly
CLC
LDA $80     ; Low byte
ADC $82     ; Add low byte
STA $84     ; Store result low
LDA $81     ; High byte
ADC $83     ; Add high byte with carry
STA $85     ; Store result high
```

**Loop counter:**
```assembly
LDX #$00
LOOP:
  ; ... loop body ...
  INX
  CPX #$10    ; Loop 16 times
  BNE LOOP
```

**Delay loop:**
```assembly
DELAY:
  LDX #$FF
DLOOP:
  DEX
  BNE DLOOP
  RTS
```

---

## Opcode Reference

This section provides the complete opcode map with execution times and memory requirements for all 56 official 6510 instructions.

### How to Read the Opcode Tables

Each instruction listing shows:
- **Opcode** - Hexadecimal byte value ($00-$FF)
- **Bytes** - Total instruction length (1-3 bytes)
- **Cycles** - Execution time in clock cycles
- **Addressing Mode** - How the operand is specified
- **Flags** - Which status register bits are affected

**Cycle Count Notes:**
- `*` = Add 1 cycle if page boundary crossed
- `**` = Add 1 cycle if branch taken, +1 more if page boundary crossed

### Programming Model

The 6510 programmer's model consists of six registers:

```
┌─────────────────────┐
│    A (8-bit)        │  Accumulator
└─────────────────────┘

┌─────────────────────┐
│    X (8-bit)        │  Index Register X
└─────────────────────┘

┌─────────────────────┐
│    Y (8-bit)        │  Index Register Y
└─────────────────────┘

┌──────────┬──────────┐
│ PCH(8)   │ PCL(8)   │  Program Counter (16-bit)
└──────────┴──────────┘

┌─────────────────────┐
│1│    SP (8-bit)     │  Stack Pointer (actual address = $01xx)
└─────────────────────┘

┌─┬─┬─┬─┬─┬─┬─┬─┐
│N│V│-│B│D│I│Z│C│      Processor Status Register
└─┴─┴─┴─┴─┴─┴─┴─┘
 7 6 5 4 3 2 1 0

N = Negative       (1 = negative result)
V = Overflow       (1 = signed overflow)
- = (unused)       (always 1)
B = Break          (1 = BRK instruction)
D = Decimal        (1 = BCD mode)
I = IRQ Disable    (1 = interrupts disabled)
Z = Zero           (1 = result is zero)
C = Carry          (1 = carry/borrow occurred)
```

### Memory Map - 6510 Perspective

```
$FFFF ┌─────────────────────┐
      │                     │
      │  Addressable        │
      │  External           │
      │  Memory             │
      │  (64KB)             │
      │                     │
$0200 ├─────────────────────┤
$01FF │                     │ ← Stack Pointer initialized here
      │  Stack              │
      │  (Page 1)           │   Descends from $01FF to $0100
      │                     │
$0100 ├─────────────────────┤
$00FF │                     │
      │  Page Zero          │   Fast addressing, Kernal/BASIC vars
      │  (Page 0)           │
$0002 ├─────────────────────┤
$0001 │  Output Register    │ ← I/O Port (memory banking in C64)
$0000 │  DDR                │ ← Data Direction Register
      └─────────────────────┘
```

### Complete Opcode Reference Table

This table shows all 256 possible opcodes. Official documented opcodes are listed with full details. Undocumented/illegal opcodes are marked with `---`.

**Legend:**
- **A** = Accumulator
- **abs** = Absolute addressing
- **abs,X** = Absolute indexed with X
- **abs,Y** = Absolute indexed with Y
- **#** = Immediate
- **impl** = Implied
- **ind** = Indirect (JMP only)
- **X,ind** = Indexed indirect
- **ind,Y** = Indirect indexed
- **rel** = Relative
- **zpg** = Zero page
- **zpg,X** = Zero page indexed with X
- **zpg,Y** = Zero page indexed with Y

#### ADC - Add Memory to Accumulator with Carry

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $69 | 2 | 2 | `ADC #$44` |
| Zero Page | $65 | 2 | 3 | `ADC $44` |
| Zero Page,X | $75 | 2 | 4 | `ADC $44,X` |
| Absolute | $6D | 3 | 4 | `ADC $4400` |
| Absolute,X | $7D | 3 | 4* | `ADC $4400,X` |
| Absolute,Y | $79 | 3 | 4* | `ADC $4400,Y` |
| (Indirect,X) | $61 | 2 | 6 | `ADC ($44,X)` |
| (Indirect),Y | $71 | 2 | 5* | `ADC ($44),Y` |

**Operation:** A = A + M + C
**Flags:** N V Z C

---

#### AND - AND Memory with Accumulator

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $29 | 2 | 2 | `AND #$44` |
| Zero Page | $25 | 2 | 3 | `AND $44` |
| Zero Page,X | $35 | 2 | 4 | `AND $44,X` |
| Absolute | $2D | 3 | 4 | `AND $4400` |
| Absolute,X | $3D | 3 | 4* | `AND $4400,X` |
| Absolute,Y | $39 | 3 | 4* | `AND $4400,Y` |
| (Indirect,X) | $21 | 2 | 6 | `AND ($44,X)` |
| (Indirect),Y | $31 | 2 | 5* | `AND ($44),Y` |

**Operation:** A = A AND M
**Flags:** N Z

---

#### ASL - Arithmetic Shift Left

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Accumulator | $0A | 1 | 2 | `ASL A` |
| Zero Page | $06 | 2 | 5 | `ASL $44` |
| Zero Page,X | $16 | 2 | 6 | `ASL $44,X` |
| Absolute | $0E | 3 | 6 | `ASL $4400` |
| Absolute,X | $1E | 3 | 7 | `ASL $4400,X` |

**Operation:** C ← [76543210] ← 0
**Flags:** N Z C

---

#### Branch Instructions

All branch instructions use **Relative** addressing (2 bytes, 2** cycles).

| Instruction | Opcode | Test | Example |
|-------------|--------|------|---------|
| **BCC** - Branch if Carry Clear | $90 | C = 0 | `BCC label` |
| **BCS** - Branch if Carry Set | $B0 | C = 1 | `BCS label` |
| **BEQ** - Branch if Equal | $F0 | Z = 1 | `BEQ label` |
| **BMI** - Branch if Minus | $30 | N = 1 | `BMI label` |
| **BNE** - Branch if Not Equal | $D0 | Z = 0 | `BNE label` |
| **BPL** - Branch if Plus | $10 | N = 0 | `BPL label` |
| **BVC** - Branch if Overflow Clear | $50 | V = 0 | `BVC label` |
| **BVS** - Branch if Overflow Set | $70 | V = 1 | `BVS label` |

**Cycles:** 2 if not taken, 3 if taken (same page), 4 if taken (different page)

---

#### BIT - Test Bits in Memory with Accumulator

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Zero Page | $24 | 2 | 3 | `BIT $44` |
| Absolute | $2C | 3 | 4 | `BIT $4400` |

**Operation:** N = M7, V = M6, Z = (A AND M == 0)
**Flags:** N V Z
**Note:** N and V are set from memory, Z from AND result

---

#### BRK - Force Break

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Implied | $00 | 1 | 7 | `BRK` |

**Operation:** PC+2↓, P↓, PC = ($FFFE)
**Flags:** B I (sets I flag, B flag pushed to stack)

---

#### CLC, CLD, CLI, CLV - Clear Flags

| Instruction | Opcode | Bytes | Cycles | Flag | Example |
|-------------|--------|-------|--------|------|---------|
| **CLC** - Clear Carry | $18 | 1 | 2 | C = 0 | `CLC` |
| **CLD** - Clear Decimal | $D8 | 1 | 2 | D = 0 | `CLD` |
| **CLI** - Clear Interrupt | $58 | 1 | 2 | I = 0 | `CLI` |
| **CLV** - Clear Overflow | $B8 | 1 | 2 | V = 0 | `CLV` |

---

#### CMP - Compare Memory with Accumulator

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $C9 | 2 | 2 | `CMP #$44` |
| Zero Page | $C5 | 2 | 3 | `CMP $44` |
| Zero Page,X | $D5 | 2 | 4 | `CMP $44,X` |
| Absolute | $CD | 3 | 4 | `CMP $4400` |
| Absolute,X | $DD | 3 | 4* | `CMP $4400,X` |
| Absolute,Y | $D9 | 3 | 4* | `CMP $4400,Y` |
| (Indirect,X) | $C1 | 2 | 6 | `CMP ($44,X)` |
| (Indirect),Y | $D1 | 2 | 5* | `CMP ($44),Y` |

**Operation:** A - M (result discarded, flags set)
**Flags:** N Z C

---

#### CPX - Compare Memory with Index X

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $E0 | 2 | 2 | `CPX #$44` |
| Zero Page | $E4 | 2 | 3 | `CPX $44` |
| Absolute | $EC | 3 | 4 | `CPX $4400` |

**Operation:** X - M (result discarded, flags set)
**Flags:** N Z C

---

#### CPY - Compare Memory with Index Y

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $C0 | 2 | 2 | `CPY #$44` |
| Zero Page | $C4 | 2 | 3 | `CPY $44` |
| Absolute | $CC | 3 | 4 | `CPY $4400` |

**Operation:** Y - M (result discarded, flags set)
**Flags:** N Z C

---

#### DEC - Decrement Memory by One

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Zero Page | $C6 | 2 | 5 | `DEC $44` |
| Zero Page,X | $D6 | 2 | 6 | `DEC $44,X` |
| Absolute | $CE | 3 | 6 | `DEC $4400` |
| Absolute,X | $DE | 3 | 7 | `DEC $4400,X` |

**Operation:** M = M - 1
**Flags:** N Z

---

#### DEX, DEY - Decrement Index Registers

| Instruction | Opcode | Bytes | Cycles | Example |
|-------------|--------|-------|--------|---------|
| **DEX** - Decrement X | $CA | 1 | 2 | `DEX` |
| **DEY** - Decrement Y | $88 | 1 | 2 | `DEY` |

**Operation:** X = X - 1 (or Y = Y - 1)
**Flags:** N Z

---

#### EOR - Exclusive-OR Memory with Accumulator

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $49 | 2 | 2 | `EOR #$44` |
| Zero Page | $45 | 2 | 3 | `EOR $44` |
| Zero Page,X | $55 | 2 | 4 | `EOR $44,X` |
| Absolute | $4D | 3 | 4 | `EOR $4400` |
| Absolute,X | $5D | 3 | 4* | `EOR $4400,X` |
| Absolute,Y | $59 | 3 | 4* | `EOR $4400,Y` |
| (Indirect,X) | $41 | 2 | 6 | `EOR ($44,X)` |
| (Indirect),Y | $51 | 2 | 5* | `EOR ($44),Y` |

**Operation:** A = A XOR M
**Flags:** N Z

---

#### INC - Increment Memory by One

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Zero Page | $E6 | 2 | 5 | `INC $44` |
| Zero Page,X | $F6 | 2 | 6 | `INC $44,X` |
| Absolute | $EE | 3 | 6 | `INC $4400` |
| Absolute,X | $FE | 3 | 7 | `INC $4400,X` |

**Operation:** M = M + 1
**Flags:** N Z

---

#### INX, INY - Increment Index Registers

| Instruction | Opcode | Bytes | Cycles | Example |
|-------------|--------|-------|--------|---------|
| **INX** - Increment X | $E8 | 1 | 2 | `INX` |
| **INY** - Increment Y | $C8 | 1 | 2 | `INY` |

**Operation:** X = X + 1 (or Y = Y + 1)
**Flags:** N Z

---

#### JMP - Jump to New Location

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Absolute | $4C | 3 | 3 | `JMP $4400` |
| Indirect | $6C | 3 | 5 | `JMP ($4400)` |

**Operation:** PC = address
**Flags:** None
**Warning:** Indirect mode has page boundary bug at $xxFF

---

#### JSR - Jump to Subroutine

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Absolute | $20 | 3 | 6 | `JSR $4400` |

**Operation:** PC+2↓, PC = address
**Flags:** None

---

#### LDA - Load Accumulator with Memory

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $A9 | 2 | 2 | `LDA #$44` |
| Zero Page | $A5 | 2 | 3 | `LDA $44` |
| Zero Page,X | $B5 | 2 | 4 | `LDA $44,X` |
| Absolute | $AD | 3 | 4 | `LDA $4400` |
| Absolute,X | $BD | 3 | 4* | `LDA $4400,X` |
| Absolute,Y | $B9 | 3 | 4* | `LDA $4400,Y` |
| (Indirect,X) | $A1 | 2 | 6 | `LDA ($44,X)` |
| (Indirect),Y | $B1 | 2 | 5* | `LDA ($44),Y` |

**Operation:** A = M
**Flags:** N Z

---

#### LDX - Load Index X with Memory

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $A2 | 2 | 2 | `LDX #$44` |
| Zero Page | $A6 | 2 | 3 | `LDX $44` |
| Zero Page,Y | $B6 | 2 | 4 | `LDX $44,Y` |
| Absolute | $AE | 3 | 4 | `LDX $4400` |
| Absolute,Y | $BE | 3 | 4* | `LDX $4400,Y` |

**Operation:** X = M
**Flags:** N Z

---

#### LDY - Load Index Y with Memory

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $A0 | 2 | 2 | `LDY #$44` |
| Zero Page | $A4 | 2 | 3 | `LDY $44` |
| Zero Page,X | $B4 | 2 | 4 | `LDY $44,X` |
| Absolute | $AC | 3 | 4 | `LDY $4400` |
| Absolute,X | $BC | 3 | 4* | `LDY $4400,X` |

**Operation:** Y = M
**Flags:** N Z

---

#### LSR - Logical Shift Right

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Accumulator | $4A | 1 | 2 | `LSR A` |
| Zero Page | $46 | 2 | 5 | `LSR $44` |
| Zero Page,X | $56 | 2 | 6 | `LSR $44,X` |
| Absolute | $4E | 3 | 6 | `LSR $4400` |
| Absolute,X | $5E | 3 | 7 | `LSR $4400,X` |

**Operation:** 0 → [76543210] → C
**Flags:** N Z C (N always 0 after LSR)

---

#### NOP - No Operation

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Implied | $EA | 1 | 2 | `NOP` |

**Operation:** (none)
**Flags:** None

---

#### ORA - OR Memory with Accumulator

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $09 | 2 | 2 | `ORA #$44` |
| Zero Page | $05 | 2 | 3 | `ORA $44` |
| Zero Page,X | $15 | 2 | 4 | `ORA $44,X` |
| Absolute | $0D | 3 | 4 | `ORA $4400` |
| Absolute,X | $1D | 3 | 4* | `ORA $4400,X` |
| Absolute,Y | $19 | 3 | 4* | `ORA $4400,Y` |
| (Indirect,X) | $01 | 2 | 6 | `ORA ($44,X)` |
| (Indirect),Y | $11 | 2 | 5* | `ORA ($44),Y` |

**Operation:** A = A OR M
**Flags:** N Z

---

#### PHA, PHP - Push Accumulator/Processor Status on Stack

| Instruction | Opcode | Bytes | Cycles | Example |
|-------------|--------|-------|--------|---------|
| **PHA** - Push Accumulator | $48 | 1 | 3 | `PHA` |
| **PHP** - Push Processor Status | $08 | 1 | 3 | `PHP` |

**Operation:** A↓ (or P↓), SP = SP - 1
**Flags:** None

---

#### PLA, PLP - Pull Accumulator/Processor Status from Stack

| Instruction | Opcode | Bytes | Cycles | Example |
|-------------|--------|-------|--------|---------|
| **PLA** - Pull Accumulator | $68 | 1 | 4 | `PLA` |
| **PLP** - Pull Processor Status | $28 | 1 | 4 | `PLP` |

**Operation:** SP = SP + 1, A↑ (or P↑)
**Flags:** N Z (PLA only); All flags (PLP)

---

#### ROL - Rotate Left One Bit

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Accumulator | $2A | 1 | 2 | `ROL A` |
| Zero Page | $26 | 2 | 5 | `ROL $44` |
| Zero Page,X | $36 | 2 | 6 | `ROL $44,X` |
| Absolute | $2E | 3 | 6 | `ROL $4400` |
| Absolute,X | $3E | 3 | 7 | `ROL $4400,X` |

**Operation:** C ← [76543210] ← C
**Flags:** N Z C

---

#### ROR - Rotate Right One Bit

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Accumulator | $6A | 1 | 2 | `ROR A` |
| Zero Page | $66 | 2 | 5 | `ROR $44` |
| Zero Page,X | $76 | 2 | 6 | `ROR $44,X` |
| Absolute | $6E | 3 | 6 | `ROR $4400` |
| Absolute,X | $7E | 3 | 7 | `ROR $4400,X` |

**Operation:** C → [76543210] → C
**Flags:** N Z C

---

#### RTI - Return from Interrupt

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Implied | $40 | 1 | 6 | `RTI` |

**Operation:** P↑, PC↑
**Flags:** All (restored from stack)

---

#### RTS - Return from Subroutine

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Implied | $60 | 1 | 6 | `RTS` |

**Operation:** PC↑, PC = PC + 1
**Flags:** None

---

#### SBC - Subtract Memory from Accumulator with Borrow

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Immediate | $E9 | 2 | 2 | `SBC #$44` |
| Zero Page | $E5 | 2 | 3 | `SBC $44` |
| Zero Page,X | $F5 | 2 | 4 | `SBC $44,X` |
| Absolute | $ED | 3 | 4 | `SBC $4400` |
| Absolute,X | $FD | 3 | 4* | `SBC $4400,X` |
| Absolute,Y | $F9 | 3 | 4* | `SBC $4400,Y` |
| (Indirect,X) | $E1 | 2 | 6 | `SBC ($44,X)` |
| (Indirect),Y | $F1 | 2 | 5* | `SBC ($44),Y` |

**Operation:** A = A - M - (1 - C)
**Flags:** N V Z C
**Note:** Use SEC before SBC for correct subtraction

---

#### SEC, SED, SEI - Set Flags

| Instruction | Opcode | Bytes | Cycles | Flag | Example |
|-------------|--------|-------|--------|------|---------|
| **SEC** - Set Carry | $38 | 1 | 2 | C = 1 | `SEC` |
| **SED** - Set Decimal | $F8 | 1 | 2 | D = 1 | `SED` |
| **SEI** - Set Interrupt | $78 | 1 | 2 | I = 1 | `SEI` |

---

#### STA - Store Accumulator in Memory

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Zero Page | $85 | 2 | 3 | `STA $44` |
| Zero Page,X | $95 | 2 | 4 | `STA $44,X` |
| Absolute | $8D | 3 | 4 | `STA $4400` |
| Absolute,X | $9D | 3 | 5 | `STA $4400,X` |
| Absolute,Y | $99 | 3 | 5 | `STA $4400,Y` |
| (Indirect,X) | $81 | 2 | 6 | `STA ($44,X)` |
| (Indirect),Y | $91 | 2 | 6 | `STA ($44),Y` |

**Operation:** M = A
**Flags:** None

---

#### STX - Store Index X in Memory

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Zero Page | $86 | 2 | 3 | `STX $44` |
| Zero Page,Y | $96 | 2 | 4 | `STX $44,Y` |
| Absolute | $8E | 3 | 4 | `STX $4400` |

**Operation:** M = X
**Flags:** None

---

#### STY - Store Index Y in Memory

| Mode | Opcode | Bytes | Cycles | Example |
|------|--------|-------|--------|---------|
| Zero Page | $84 | 2 | 3 | `STY $44` |
| Zero Page,X | $94 | 2 | 4 | `STY $44,X` |
| Absolute | $8C | 3 | 4 | `STY $4400` |

**Operation:** M = Y
**Flags:** None

---

#### TAX, TAY, TSX, TXA, TXS, TYA - Transfer Instructions

| Instruction | Opcode | Bytes | Cycles | Flags | Example |
|-------------|--------|-------|--------|-------|---------|
| **TAX** - Transfer A to X | $AA | 1 | 2 | N Z | `TAX` |
| **TAY** - Transfer A to Y | $A8 | 1 | 2 | N Z | `TAY` |
| **TSX** - Transfer SP to X | $BA | 1 | 2 | N Z | `TSX` |
| **TXA** - Transfer X to A | $8A | 1 | 2 | N Z | `TXA` |
| **TXS** - Transfer X to SP | $9A | 1 | 2 | None | `TXS` |
| **TYA** - Transfer Y to A | $98 | 1 | 2 | N Z | `TYA` |

**Operation:** Destination = Source
**Note:** TXS does not affect flags

---

### Opcode Summary by Hexadecimal Value

Quick lookup table showing what instruction each opcode represents:

```
     x0   x1   x2   x3   x4   x5   x6   x7   x8   x9   xA   xB   xC   xD   xE   xF
0x   BRK  ORA  ---  ---  ---  ORA  ASL  ---  PHP  ORA  ASL  ---  ---  ORA  ASL  ---
1x   BPL  ORA  ---  ---  ---  ORA  ASL  ---  CLC  ORA  ---  ---  ---  ORA  ASL  ---
2x   JSR  AND  ---  ---  BIT  AND  ROL  ---  PLP  AND  ROL  ---  BIT  AND  ROL  ---
3x   BMI  AND  ---  ---  ---  AND  ROL  ---  SEC  AND  ---  ---  ---  AND  ROL  ---
4x   RTI  EOR  ---  ---  ---  EOR  LSR  ---  PHA  EOR  LSR  ---  JMP  EOR  LSR  ---
5x   BVC  EOR  ---  ---  ---  EOR  LSR  ---  CLI  EOR  ---  ---  ---  EOR  LSR  ---
6x   RTS  ADC  ---  ---  ---  ADC  ROR  ---  PLA  ADC  ROR  ---  JMP  ADC  ROR  ---
7x   BVS  ADC  ---  ---  ---  ADC  ROR  ---  SEI  ADC  ---  ---  ---  ADC  ROR  ---
8x   ---  STA  ---  ---  STY  STA  STX  ---  DEY  ---  TXA  ---  STY  STA  STX  ---
9x   BCC  STA  ---  ---  STY  STA  STX  ---  TYA  STA  TXS  ---  ---  STA  ---  ---
Ax   LDY  LDA  LDX  ---  LDY  LDA  LDX  ---  TAY  LDA  TAX  ---  LDY  LDA  LDX  ---
Bx   BCS  LDA  ---  ---  LDY  LDA  LDX  ---  CLV  LDA  TSX  ---  LDY  LDA  LDX  ---
Cx   CPY  CMP  ---  ---  CPY  CMP  DEC  ---  INY  CMP  DEX  ---  CPY  CMP  DEC  ---
Dx   BNE  CMP  ---  ---  ---  CMP  DEC  ---  CLD  CMP  ---  ---  ---  CMP  DEC  ---
Ex   CPX  SBC  ---  ---  CPX  SBC  INC  ---  INX  SBC  NOP  ---  CPX  SBC  INC  ---
Fx   BEQ  SBC  ---  ---  ---  SBC  INC  ---  SED  SBC  ---  ---  ---  SBC  INC  ---
```

**Note:** `---` indicates undocumented/illegal opcodes. These may have various effects but are not officially supported.

### Cycle Timing Notes

**Page Boundary Crossing Penalty:**
When using indexed addressing modes (Absolute,X, Absolute,Y, Indirect,Y), if the effective address crosses a page boundary (e.g., $C0FF + $01 = $C100), add 1 additional cycle.

**Branch Timing:**
- Not taken: 2 cycles
- Taken (same page): 3 cycles
- Taken (different page): 4 cycles

**Examples:**
```assembly
LDA $C0FF,X   ; X = $01
              ; Effective address = $C100
              ; Crosses page boundary ($C0 → $C1)
              ; Takes 5 cycles instead of 4

BNE FORWARD   ; FORWARD is 10 bytes ahead (same page)
              ; Branch taken: 3 cycles

BNE DISTANT   ; DISTANT is 200 bytes ahead (different page)
              ; Branch taken: 4 cycles
```

### Application Notes

**Zero Page Optimization:**
The 6510's I/O port at $0000-$0001 enhances zero page addressing by allowing peripheral devices to modify memory locations directly. This enables powerful programming techniques:

1. **Dynamic indirect addressing** - Peripherals can update pointer tables in zero page
2. **Hardware-assisted state machines** - External devices can change program flow by modifying zero page vectors
3. **Fast I/O** - Zero page instructions are 1 byte shorter and 1 cycle faster than absolute addressing

**Example - Peripheral-Controlled Pointers:**
```assembly
; Setup: Configure $0001 bits as inputs via DDR at $0000
LDA #$00
STA $00     ; All bits input

; External hardware can now change $0001
; Use it as part of indirect addressing
LDY #$00
LDA ($01),Y ; Loads from address determined by hardware!
```

This technique was revolutionary in 1975 and remains unique to the 6510 family.

---

## Quick Reference Tables

### CPU At a Glance

```
Data:      8-bit
Address:   16-bit (64KB)
Clock:     ~1 MHz (two-phase)
Registers: A, X, Y, SP, PC, P
Stack:     $0100-$01FF (256 bytes, descending)
Vectors:   NMI=$FFFA, RESET=$FFFC, IRQ=$FFFE
```

### Registers Quick Ref

| Reg | Size | Range | Description |
|-----|------|-------|-------------|
| A | 8-bit | $00-$FF | Accumulator (main data) |
| X | 8-bit | $00-$FF | X Index register |
| Y | 8-bit | $00-$FF | Y Index register |
| SP | 8-bit | $00-$FF | Stack Pointer (add $0100 for actual address) |
| PC | 16-bit | $0000-$FFFF | Program Counter |
| P | 8-bit | %NV-BDIZC | Processor Status flags |

### Status Flag Quick Ref

```assembly
; Reading flags
BPL label  ; Branch if N=0 (Plus)
BMI label  ; Branch if N=1 (Minus)
BVC label  ; Branch if V=0 (no oVerflow)
BVS label  ; Branch if V=1 (oVerflow Set)
BCC label  ; Branch if C=0 (Carry Clear)
BCS label  ; Branch if C=1 (Carry Set)
BNE label  ; Branch if Z=0 (Not Equal)
BEQ label  ; Branch if Z=1 (EQual)

; Setting flags
SEC        ; Set Carry flag
CLC        ; Clear Carry flag
SED        ; Set Decimal mode
CLD        ; Clear Decimal mode
SEI        ; Set Interrupt disable
CLI        ; Clear Interrupt disable
CLV        ; Clear oVerflow flag
```

### I/O Port Quick Ref (C64 Specific)

```assembly
; Default values (Kernal sets at startup)
$00 = $2F    ; DDR: bits 0-5 output, 6-7 input
$01 = $37    ; POR: all ROMs visible

; Common memory configurations
$30 = %00110000  ; RAM only (no ROMs)
$31 = %00110001  ; RAM + Character ROM
$35 = %00110101  ; RAM + I/O + Character ROM
$36 = %00110110  ; RAM + I/O + Kernal ROM
$37 = %00110111  ; All ROMs + I/O (default)
```

### Interrupt Vectors

| Vector | Address | Type | Priority |
|--------|---------|------|----------|
| **NMI** | $FFFA-$FFFB | Non-Maskable | Highest |
| **RESET** | $FFFC-$FFFD | Reset | — |
| **IRQ/BRK** | $FFFE-$FFFF | Maskable | Lowest |

**Note:** In C64, these point to Kernal ROM routines that jump through RAM vectors at $0314-$0315 (IRQ) and $0318-$0319 (NMI).

---

## Educational Notes

### For Lesson Planning

**Tier 1 (Discovery):** Don't mention 6510 specifics yet - students are learning BASIC
**Tier 2 (Mastery):** Introduce PEEKing hardware registers, maybe simple $01 changes
**Tier 3 (Assembly):** Full 6510 programming with this reference
**Tier 4 (Artistry):** Advanced techniques like cycle counting, timing-critical code

### Key Concepts for Assembly Lessons

1. **Registers are precious** - Only 3 data registers (A, X, Y), plan usage carefully
2. **Memory is slow** - Accessing RAM takes CPU cycles, keep hot data in registers
3. **Flags matter** - Status register affects all branching decisions
4. **Stack is limited** - Only 256 bytes, don't nest too deeply
5. **Banking is essential** - Must switch ROMs out to access full 64KB RAM

### Common Student Questions

**Q: Why can't I write to $D000-$DFFF?**
A: That's I/O space. You need to bank out the I/O chips by changing $01 to see RAM there.

**Q: Why does my code crash after changing $01?**
A: You probably banked out the Kernal ROM while it was running. Save/restore $01 carefully.

**Q: What's the difference between IRQ and NMI?**
A: IRQ can be disabled (SEI instruction), NMI cannot. NMI is for critical events only.

**Q: Why is the stack at $0100-$01FF specifically?**
A: It's hardwired in the CPU. The 8-bit stack pointer (SP) is automatically prefixed with $01.

---

## References for Lesson Development

### Essential Memory Locations

- `$0000` - Data Direction Register (DDR) for I/O port
- `$0001` - I/O Port (memory banking control)
- `$0100-$01FF` - Stack (256 bytes)
- `$FFFA-$FFFB` - NMI vector (lo/hi)
- `$FFFC-$FFFD` - RESET vector (lo/hi)
- `$FFFE-$FFFF` - IRQ/BRK vector (lo/hi)

### Where to Learn More

- **Official Commodore References:**
  - "Commodore 64 Programmer's Reference Guide" (this document)
  - MOS Technology 6500 series datasheets

- **Modern Resources:**
  - Western Design Center W65C02S datasheet (enhanced 6502)
  - www.oxyron.de - Advanced C64 programming techniques
  - Codebase64.org - Assembly programming wiki

---

## Version History

- **v2.0 - FINAL** - Completed with Appendix L Part 4/4 (2025-01-18)
  - Complete opcode reference with all 56 instructions
  - Hexadecimal opcode lookup table
  - Detailed cycle timing for every instruction
  - Page boundary crossing documentation
  - Programming model diagram
  - Memory map from 6510 perspective
  - Application notes on zero page optimization
  - Peripheral-controlled addressing examples
  - **Document is now COMPLETE**

- **v1.2** - Updated with Appendix L Part 3/4
  - Complete addressing modes documentation (all 13 modes)
  - Detailed explanations with step-by-step examples
  - Page boundary crossing behavior
  - Famous 6502 indirect jump bug documentation
  - Complete instruction set reference (all 56 opcodes)
  - Instruction categories and flag effects
  - Common coding patterns and idioms
  - Practical examples for each addressing mode

- **v1.1** - Updated with Appendix L Part 2/4
  - Detailed timing specifications (1 MHz and 2 MHz)
  - Complete signal descriptions for all pins
  - Memory interface timing examples
  - Interrupt handling (IRQ/NMI) details
  - DMA and cycle-stealing behavior
  - Practical assembly programming examples

- **v1.0** - Initial reference from Appendix L Part 1/4
  - CPU architecture and features
  - Pin configuration and block diagram
  - Electrical specifications and timing
  - I/O port documentation

---

**This reference is now complete and ready for use in developing assembly language lessons for the Code Like It's 198x curriculum.**
