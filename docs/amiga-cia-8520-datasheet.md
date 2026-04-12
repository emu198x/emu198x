# MOS 8520 CIA Datasheet Reference

Extracted from the Amiga Hardware Reference documentation. The 8520 is a Complex Interface Adapter used as CIA-A and CIA-B in all Amiga models. This document covers the chip itself; see `amiga-hardware-reference.md` for Amiga-specific pin assignments, `amiga-io-audio-expansion.md` for device-level usage, and `amiga-resources.md` for `cia.resource` arbitration.

## Table of Contents

1. [Register Map](#1-register-map)
2. [I/O Ports (PRA, PRB, DDRA, DDRB)](#2-io-ports)
3. [Amiga Port Assignments](#3-amiga-port-assignments)
4. [Interval Timers (Timer A & B)](#4-interval-timers)
5. [Time of Day Counter (TOD)](#5-time-of-day-counter)
6. [Serial Data Register (SDR)](#6-serial-data-register)
7. [Interrupt Control Register (ICR)](#7-interrupt-control-register)
8. [Control Register A (CRA)](#8-control-register-a)
9. [Control Register B (CRB)](#9-control-register-b)
10. [Handshaking (PC and FLAG)](#10-handshaking)
11. [Hardware Interface Signals](#11-hardware-interface-signals)
12. [Address Decoding](#12-address-decoding)
13. [Reset State](#13-reset-state)
14. [Emulator Implementation Notes](#14-emulator-implementation-notes)

---

## 1. Register Map

| Offset | CIA-A Address | CIA-B Address | Register | Access | Function |
|--------|--------------|--------------|----------|--------|----------|
| $0 | $BFE001 | $BFD000 | PRA | R/W | Peripheral Data Register A |
| $1 | $BFE101 | $BFD100 | PRB | R/W | Peripheral Data Register B |
| $2 | $BFE201 | $BFD200 | DDRA | R/W | Data Direction Register A |
| $3 | $BFE301 | $BFD300 | DDRB | R/W | Data Direction Register B |
| $4 | $BFE401 | $BFD400 | TALO | R/W | Timer A Low Byte |
| $5 | $BFE501 | $BFD500 | TAHI | R/W | Timer A High Byte |
| $6 | $BFE601 | $BFD600 | TBLO | R/W | Timer B Low Byte |
| $7 | $BFE701 | $BFD700 | TBHI | R/W | Timer B High Byte |
| $8 | $BFE801 | $BFD800 | TODLO | R/W | TOD Counter bits 7-0 (LSB) |
| $9 | $BFE901 | $BFD900 | TODMID | R/W | TOD Counter bits 15-8 |
| $A | $BFEA01 | $BFDA00 | TODHI | R/W | TOD Counter bits 23-16 (MSB) |
| $B | — | — | — | — | Not used |
| $C | $BFEC01 | $BFDC00 | SDR | R/W | Serial Data Register |
| $D | $BFED01 | $BFDD00 | ICR | R/W | Interrupt Control Register |
| $E | $BFEE01 | $BFDE00 | CRA | R/W | Control Register A |
| $F | $BFEF01 | $BFDF00 | CRB | R/W | Control Register B |

Register offsets are in A11-A8 (steps of $100 in the address space), not A3-A0 like the 6526. CIA-A is on data bus D7-D0 (odd addresses), CIA-B is on D15-D8 (even addresses). Byte access only.

---

## 2. I/O Ports

Each CIA has two 8-bit bidirectional I/O ports (A and B), each with a data register (PRx) and a direction register (DDRx).

### Direction control
- DDRx bit = 1 → corresponding PRx bit is **output**
- DDRx bit = 0 → corresponding PRx bit is **input**

### Read behaviour
Reading PRx returns the **actual pin state**, regardless of DDR setting. If a pin is configured as output, reading returns what the pin is driving (which may differ from the written value if another device is pulling the line).

### Drive capability
Two TTL load units. Passive and active pull-ups for CMOS/TTL compatibility.

---

## 3. Amiga Port Assignments

### CIA-A Port A ($BFE001)

| Bit | Signal | Dir | Function |
|-----|--------|-----|----------|
| 7 | /FIR1 | In | Game port 1 fire button |
| 6 | /FIR0 | In | Game port 0 fire button |
| 5 | /RDY | In | Disk drive ready |
| 4 | /TK0 | In | Disk track 00 sensor |
| 3 | /WPRO | In | Disk write protect |
| 2 | /CHNG | In | Disk change (disk removed since last step) |
| 1 | /LED | Out | Power LED (active low = bright) + audio filter |
| 0 | OVL | Out | Memory overlay (1 = ROM at $000000, 0 = RAM) |

DDRA must be $03 at reset (bits 0-1 output, bits 2-7 input).

### CIA-A Port B ($BFE101)

| Bits | Signal | Dir | Function |
|------|--------|-----|----------|
| 7-0 | P7-P0 | I/O | Centronics parallel data |

### CIA-B Port A ($BFD000)

| Bit | Signal | Dir | Function |
|-----|--------|-----|----------|
| 7 | /DTR | Out | Serial data terminal ready |
| 6 | /RTS | Out | Serial request to send |
| 5 | /CD | In | Serial carrier detect |
| 4 | /CTS | In | Serial clear to send |
| 3 | /DSR | In | Serial data set ready |
| 2 | SEL | In | Centronics printer select |
| 1 | POUT | In | Centronics paper out |
| 0 | BUSY | In | Centronics busy |

### CIA-B Port B ($BFD100)

| Bit | Signal | Dir | Function |
|-----|--------|-----|----------|
| 7 | /MTR | Out | Disk motor (active low; latched on /SELx) |
| 6 | /SEL3 | Out | Select external drive 3 |
| 5 | /SEL2 | Out | Select external drive 2 |
| 4 | /SEL1 | Out | Select external drive 1 |
| 3 | /SEL0 | Out | Select internal drive (DF0:) |
| 2 | /SIDE | Out | Disk head side select (0 = upper) |
| 1 | DIR | Out | Disk step direction (0 = inward/higher tracks) |
| 0 | /STEP | Out | Disk step pulse (3.0 ms minimum width) |

---

## 4. Interval Timers

### Architecture

Each timer is a 16-bit presettable **down counter** with a paired 16-bit **latch** (prescaler).

- **Write** to TALo/TAHi or TBLo/TBHi loads the **latch**, not the counter.
- **Read** from TALo/TAHi or TBLo/TBHi returns the current **counter** value.
- On underflow (counter reaches 0), the latch value is automatically reloaded.

### Clock frequency

- NTSC: 0.715909 MHz (E clock = 7.15909 MHz ÷ 10)
- PAL: 0.709379 MHz (E clock = 7.09379 MHz ÷ 10)

### Timer A clock source (CRA bit 5)

| CRA5 | Source |
|------|--------|
| 0 | Phi2 (E clock) — system timer |
| 1 | Positive transitions on CNT pin — event counter |

### Timer B clock source (CRB bits 6-5)

| CRB6 | CRB5 | Source |
|------|------|--------|
| 0 | 0 | Phi2 (E clock) |
| 0 | 1 | Positive transitions on CNT pin |
| 1 | 0 | Timer A underflow pulses |
| 1 | 1 | Timer A underflow pulses gated by CNT high |

The cascade modes (CRB6=1) allow a 32-bit timer by chaining Timer A → Timer B.

### Run mode (CRx bit 3)

| CRx3 | Mode | Behaviour |
|------|------|-----------|
| 0 | Continuous | Underflow → reload from latch → keep counting → repeat |
| 1 | One-shot | Underflow → reload from latch → stop (CRx0 cleared automatically) |

### Port B output (CRx bit 1)

| CRx1 | Effect |
|------|--------|
| 0 | Port B pin normal I/O |
| 1 | Timer output forced to PB6 (Timer A) or PB7 (Timer B), overriding DDR |

### Output mode (CRx bit 2)

| CRx2 | Mode | Behaviour |
|------|------|-----------|
| 0 | Pulse | Single positive pulse (one Phi2 cycle wide) on each underflow |
| 1 | Toggle | Output inverts on each underflow (set high on start, low on reset) |

### Force load (CRx bit 4)

Strobe bit. Writing 1 immediately loads the latch into the counter. Always reads 0. Writing 0 has no effect. Does not start the timer.

### One-shot auto-start quirk

Writing the timer HIGH byte while in one-shot mode triggers an immediate load from latch to counter AND starts the timer, regardless of the START bit. This is by design — it allows single-write timer setup.

### Underflow behaviour summary

On underflow:
1. Interrupt condition is set (ICR bit 0 for TA, bit 1 for TB)
2. Latch reloads into counter
3. If one-shot: START bit clears, timer stops
4. If continuous: timer keeps running
5. If PBON: PB6/PB7 output pulses or toggles
6. If Timer B is cascaded from Timer A: Timer B decrements

---

## 5. Time of Day Counter (TOD)

### Architecture

24-bit binary up-counter incremented by positive edges on the TOD pin.

- CIA-A TOD pin: connected to the vertical sync (50 Hz PAL / 60 Hz NTSC)
- CIA-B TOD pin: connected to the horizontal sync line

Passive pull-up on the TOD pin.

### Alarm

Each TOD has a paired 24-bit alarm register. When the counter matches the alarm, ICR bit 2 (ALRM) is set.

CRB bit 7 selects whether writes go to the **clock** (0) or the **alarm** (1). Reads always return the clock value regardless of CRB7.

### Latch-on-read protocol

Reading TODHI (MSB) **latches all 24 bits** into a holding register. The TOD counter keeps running underneath. Subsequent reads of TODMID and TODLO return the latched values. Reading TODLO (LSB) **releases the latch**.

**Correct read order:** TODHI → TODMID → TODLO.

If you read only one register, you must still read TODLO afterward to release the latch, or subsequent reads of any TOD register will return stale data.

### Write protocol

Writing any TOD register **stops the clock**. The clock does not restart until TODLO (LSB) is written.

**Correct write order:** TODHI → TODMID → TODLO (which restarts the clock).

### Bit layout

```
TODHI  (offset $A): bits 23-16
TODMID (offset $9): bits 15-8
TODLO  (offset $8): bits 7-0
```

---

## 6. Serial Data Register (SDR)

Buffered 8-bit synchronous serial shift register.

### Mode selection (CRA bit 6)

| CRA6 | Mode | Clock | Data |
|------|------|-------|------|
| 0 | Input (receive) | External CNT | SP pin → SDR |
| 1 | Output (transmit) | Timer A / 2 | SDR → SP pin |

### Input mode

- Data shifts in on each **rising edge** of CNT.
- After 8 CNT pulses, byte transfers to SDR and ICR bit 3 (SP) is set.
- MSB first.

### Output mode

- Timer A must be running in continuous mode (provides baud rate clock).
- Data shifts out at **half** the Timer A underflow rate.
- Maximum rate: Phi2 ÷ 4.
- CNT outputs the shift clock derived from Timer A.
- After 8 bits transmitted, ICR bit 3 (SP) is set; SDR can be reloaded immediately for continuous transmission.
- When idle (no data to send): CNT goes high, SP holds last transmitted bit.

### Bidirectional bus

Both CNT and SP are open-drain outputs, allowing a shared serial bus with one master and multiple slaves.

### Amiga usage

- CIA-A SDR: keyboard interface (keyboard is the clock master, CIA-A receives)
- CIA-B SDR: unused

---

## 7. Interrupt Control Register (ICR)

The ICR address serves two distinct registers depending on read vs write.

### Read: DATA register

Returns pending interrupt state and **clears all bits on read**. The /IRQ pin returns high after the read.

```
Bit 7: IR    1 = at least one enabled interrupt is pending
Bit 6: 0     (forced zero)
Bit 5: 0     (forced zero)
Bit 4: FLG   FLAG pin negative edge detected
Bit 3: SP    Serial port byte transferred (8 bits shifted)
Bit 2: ALRM  TOD alarm match
Bit 1: TB    Timer B underflow
Bit 0: TA    Timer A underflow
```

**Critical for emulators:** Reading ICR is **destructive** — all bits clear. This is why `cia.resource` exists on the Amiga: two readers would race and lose each other's interrupts. The `SetICR` / `AbleICR` functions in `cia.resource` wrap this safely.

IR (bit 7) is set when any DATA bit AND its corresponding MASK bit are both 1. If an interrupt source fires but its mask bit is 0, the DATA bit is still set but IR stays 0 and /IRQ is not asserted.

### Write: MASK register

Controls which interrupt sources can assert /IRQ.

```
Bit 7: S/C   Set/Clear control
Bit 6: —     (no effect)
Bit 5: —     (no effect)
Bit 4: FLG   FLAG interrupt mask
Bit 3: SP    Serial port interrupt mask
Bit 2: ALRM  TOD alarm interrupt mask
Bit 1: TB    Timer B interrupt mask
Bit 0: TA    Timer A interrupt mask
```

### Set/Clear protocol

This is the same convention used by Paula's INTENA, DMACON, and ADKCON:

- **Bit 7 = 1 (Set):** Bits written as 1 **set** the corresponding mask bits. Bits written as 0 have no effect.
- **Bit 7 = 0 (Clear):** Bits written as 1 **clear** the corresponding mask bits. Bits written as 0 have no effect.

Examples:
- Enable Timer A only: write `$81` (S/C=1, TA=1)
- Disable Timer B: write `$02` (S/C=0, TB=1)
- Enable all: write `$9F` (S/C=1, all source bits=1)
- Disable all: write `$1F` (S/C=0, all source bits=1)

---

## 8. Control Register A (CRA)

```
Bit 7: —        Unused (reads 0)
Bit 6: SPMODE   Serial port direction: 0=input, 1=output
Bit 5: INMODE   Timer A clock: 0=Phi2, 1=CNT transitions
Bit 4: LOAD     Force load (strobe, reads 0)
Bit 3: RUNMODE  0=continuous, 1=one-shot
Bit 2: OUTMODE  PB6 output: 0=pulse, 1=toggle
Bit 1: PBON     Timer A → PB6: 0=disabled, 1=enabled
Bit 0: START    Timer A: 0=stop, 1=run
```

---

## 9. Control Register B (CRB)

```
Bit 7: ALARM    TOD write target: 0=clock, 1=alarm
Bit 6: INMODE1  Timer B clock select (high bit)
Bit 5: INMODE0  Timer B clock select (low bit)
Bit 4: LOAD     Force load (strobe, reads 0)
Bit 3: RUNMODE  0=continuous, 1=one-shot
Bit 2: OUTMODE  PB7 output: 0=pulse, 1=toggle
Bit 1: PBON     Timer B → PB7: 0=disabled, 1=enabled
Bit 0: START    Timer B: 0=stop, 1=run
```

Timer B clock modes (bits 6-5):

| CRB6 | CRB5 | Source |
|------|------|--------|
| 0 | 0 | Phi2 |
| 0 | 1 | CNT positive transitions |
| 1 | 0 | Timer A underflows |
| 1 | 1 | Timer A underflows while CNT is high |

---

## 10. Handshaking (PC and FLAG)

### PC output (active low pulse)

PC goes low on the **third Phi2 cycle** after a Port B access (read or write). Signals "data accepted" or "data ready" to external hardware. Returns high after one Phi2 cycle.

16-bit handshake: access Port A first, then Port B. The PC pulse after the Port B access acknowledges the full 16-bit transfer.

### FLAG input (active low edge)

Negative edge on FLAG sets ICR bit 4 (FLG). Passive pull-up on pin. Typically receives PC output from another CIA or a general-purpose interrupt input.

### Amiga usage

- CIA-A FLAG: directly active — directly active from accent connector
- CIA-B FLAG: directly active — directly active from accent connector

---

## 11. Hardware Interface Signals

### Phi2 (Clock input)

- NTSC: 0.715909 MHz
- PAL: 0.709379 MHz
- Source: 680x0 E clock (CPU clock ÷ 10)
- All internal operations are synchronous to Phi2

### /CS (Chip Select)

Active low. Device responds to R/W and address lines only when /CS is low AND Phi2 is high. High /CS tri-states the data bus and ignores control signals.

### R/W (Read/Write)

- High: read (data from CIA to bus)
- Low: write (data from bus to CIA)

### RS3-RS0 (Register Select)

Directly selects one of 16 internal registers.

### DB7-DB0 (Data Bus)

Tri-state. Driven only during reads (/CS low, R/W high, Phi2 high).

### /IRQ (Interrupt Request)

**Open-drain output.** High-impedance when no interrupt; pulled low when interrupt conditions met (DATA & MASK both set). Multiple /IRQ outputs can be wired-OR'd.

- CIA-A /IRQ → INT2 (68000 autovector level 2)
- CIA-B /IRQ → INT6 (68000 autovector level 6)

### /RES (Reset)

Active low input. Effects:
- All port pins → input (DDRx = $00)
- Port data registers → $00
- Control registers → $00
- Timer latches → $FFFF (all ones)
- All other registers → $00

---

## 12. Address Decoding

CIA-A is selected when A13=0, A12=1 (active region $BFE001). CIA-B is selected when A13=1, A12=0 (active region $BFD000).

```
CIA-A: address bit pattern  101x xxxx xx01 rrrr xxxx xxx0  → $BFEr01
CIA-B: address bit pattern  101x xxxx xx10 rrrr xxxx xxx1  → $BFDr00
```

Where `rrrr` = A11-A8 = register select (RS3-RS0).

CIA-A appears on data bus bits D7-D0 (byte at odd addresses).
CIA-B appears on data bus bits D15-D8 (byte at even addresses).

**Byte access only.** Word or long accesses to CIA address space have undefined behaviour.

---

## 13. Reset State

After /RES assertion:

| Register | Reset value | Notes |
|----------|-------------|-------|
| PRA | $00 | But pins read high (pull-ups) since DDRA=$00 |
| PRB | $00 | But pins read high (pull-ups) since DDRB=$00 |
| DDRA | $00 | All inputs |
| DDRB | $00 | All inputs |
| TALO/TAHI | reads counter | Latch = $FFFF |
| TBLO/TBHI | reads counter | Latch = $FFFF |
| TOD | $000000 | Stopped until LSB written |
| SDR | $00 | |
| ICR (DATA) | $00 | No pending interrupts |
| ICR (MASK) | $00 | All interrupts disabled |
| CRA | $00 | Timer A stopped, input mode, Phi2, continuous |
| CRB | $00 | Timer B stopped, input mode, Phi2, continuous, clock mode |

**Critical for Amiga boot:** Kickstart explicitly sets CIA-A DDRA to $03 (bits 0-1 output for OVL and /LED) early in the reset sequence. The hardware reset sets DDRA to $00, making OVL an input — the pull-up on the OVL line holds it high, which keeps the ROM overlay active. Kickstart writes $02 to PRA after setting DDRA=$03 to clear OVL and light the LED.

---

## 14. Emulator Implementation Notes

### ICR read-clear atomicity

The destructive read of ICR is the single most important CIA behaviour to get right. An interrupt can arrive between the CPU's read cycle beginning and the ICR bits being cleared. WinUAE handles this with event-queue scheduling (`cia.cpp:811`): the interrupt sets the DATA bit, and the read clears it — but if both happen in the same Phi2 cycle, the interrupt wins (the bit is set, then cleared, and the ISR sees it).

### Timer latch vs counter

Writes always go to the latch, reads always come from the counter. The counter is loaded from the latch only on: (a) underflow, (b) FORCE LOAD strobe, or (c) one-shot high-byte write auto-start. An emulator that conflates latch and counter will break any code that reads back a timer while writing a new period.

### TOD latch-on-read

The latch protocol means a reader must always complete the MSB→LSB read sequence. Code that reads only TODMID without first reading TODHI will get the current value (no latch). Code that reads TODHI but not TODLO will leave the latch engaged — all subsequent TOD reads return the latched value until TODLO is read.

### Timer cascade for 32-bit timing

Timer B counting Timer A underflows (CRB6:5 = 10) creates a 32-bit timer. Timer A must be running. The cascade does not require Timer B's START bit — but Timer B's START must still be set for it to count. An underflow of the 32-bit pair sets Timer B's underflow interrupt.

### One-shot high-byte auto-start

Writing TAHI in one-shot mode both loads the counter AND starts the timer. This is intentional and documented. Some Amiga code relies on this for single-write timer setup (write low byte, write high byte = timer is loaded and running).

### PB6/PB7 output override

When PBON=1, the timer output overrides DDR for that pin. The pin becomes output regardless of the DDR setting. Reading PRB still returns the pin state (which is now the timer output, not the written PRB value).

### E-clock synchronisation cost

CIA register access must synchronise to the E clock. This costs 5-9.5 colour clocks depending on the phase of E when the access starts (see `amiga-cycle-accurate.md` §10). This is why rapid CIA polling is expensive and why `cia.resource` + interrupt-driven access is preferred.

---

## Source

Extracted from the Amiga Hardware Reference Manual (Appendix F: 8520 CIA Chip), supplemented with Amiga-specific integration notes from the Hardware Reference Manual body text and cross-referenced with `amiga-hardware-reference.md`, `amiga-cycle-accurate.md`, and `amiga-resources.md`.
