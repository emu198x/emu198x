# CIA 8520 — Timers, I/O Ports, TOD, Serial

The Amiga has two MOS 8520 CIA chips. They provide timers, I/O ports, a
time-of-day clock, and a serial shift register. The two CIAs serve different
purposes but are identical silicon — the differences are in what's wired to
their pins.

## Address Decoding

| CIA | Base address | Accent | Active bytes |
|-----|-------------|--------|-------------|
| CIA-A | $BFE001 | Accent on odd bytes | $BFE001, $BFE101, ..., $BFEF01 |
| CIA-B | $BFD000 | Accent on even bytes | $BFD000, $BFD100, ..., $BFDF00 |

Register offsets are multiplied by $100 (256) because each register occupies
one byte within a 256-byte-aligned address range. This is the Amiga's
register-select wiring, not a CIA feature.

### Register Map

| Offset | CIA-A ($BFExxx) | CIA-B ($BFDxxx) | Name |
|--------|----------------|----------------|------|
| $000 | $BFE001 | $BFD000 | PRA — Port A data |
| $100 | $BFE101 | $BFD100 | PRB — Port B data |
| $200 | $BFE201 | $BFD200 | DDRA — Data direction A |
| $300 | $BFE301 | $BFD300 | DDRB — Data direction B |
| $400 | $BFE401 | $BFD400 | TALO — Timer A low byte |
| $500 | $BFE501 | $BFD500 | TAHI — Timer A high byte |
| $600 | $BFE601 | $BFD600 | TBLO — Timer B low byte |
| $700 | $BFE701 | $BFD700 | TBHI — Timer B high byte |
| $800 | $BFE801 | $BFD800 | TOD-LO — Time of Day low |
| $900 | $BFE901 | $BFD900 | TOD-MID — Time of Day mid |
| $A00 | $BFEA01 | $BFDA00 | TOD-HI — Time of Day high |
| $B00 | — | — | (unused) |
| $C00 | $BFEC01 | $BFDC00 | SDR — Serial Data Register |
| $D00 | $BFED01 | $BFDD00 | ICR — Interrupt Control Register |
| $E00 | $BFEE01 | $BFDE00 | CRA — Control Register A |
| $F00 | $BFEF01 | $BFDF00 | CRB — Control Register B |

## Port Wiring

### CIA-A Port A ($BFE001)

| Bit | Direction | Signal | Purpose |
|-----|-----------|--------|---------|
| 7 | In | /GAMEPORT1 | Gameport 1 active (active low) |
| 6 | In | /GAMEPORT0 | Gameport 0 active (active low) |
| 5 | Out | /DSKRDY | Disk ready (directly active low) |
| 4 | Out | /DSKTRACK0 | Drive at track 0 |
| 3 | Out | /DSKPROT | Disk write-protected |
| 2 | Out | /DSKCHANGE | Disk has been changed |
| 1 | Out | LED | Power LED (0 = bright, 1 = dim/filter) |
| 0 | Out | OVL | Overlay — maps ROM at $000000 when set |

**Overlay (bit 0):** At reset, OVL is set (ROM overlays chip RAM at $000000
so the CPU can read the reset vectors). Kickstart clears OVL early in boot to
expose chip RAM. This is a write-once-in-practice operation.

### CIA-A Port B ($BFE101)

Connected to the parallel port data lines. Read/write 8 bits of parallel data.

### CIA-B Port A ($BFD100)

| Bit | Direction | Signal | Purpose |
|-----|-----------|--------|---------|
| 7 | Out | /MTR | Floppy motor on (active low) |
| 6 | Out | /SEL3 | Select drive 3 |
| 5 | Out | /SEL2 | Select drive 2 |
| 4 | Out | /SEL1 | Select drive 1 |
| 3 | Out | /SEL0 | Select drive 0 |
| 2 | Out | /SIDE | Head select (0 = upper, 1 = lower) |
| 1 | Out | DIR | Step direction (0 = inward, 1 = outward) |
| 0 | Out | /STEP | Step pulse (active low edge) |

### CIA-B Port B ($BFD000)

Connected to the parallel port data lines. Read/write 8 bits of parallel data.

## Port Read/Write Semantics

Reading a port returns a composite of the internal register and external
pin state, masked by the DDR:

```
read_value = (port_register & DDR) | (external_pins & ~DDR)
```

Bits configured as outputs (DDR = 1) read back the last written value. Bits
configured as inputs (DDR = 0) read the external pin state.

## Timers

Each CIA has two 16-bit countdown timers (Timer A and Timer B).

### Timer Modes

**CRA (Timer A control):**

| Bit | Name | Meaning |
|-----|------|---------|
| 0 | START | 1 = timer is running |
| 3 | RUNMODE | 0 = continuous (reload and restart), 1 = one-shot (stop after underflow) |
| 5 | INMODE | 0 = count system clocks, 1 = count CNT pin edges |

**CRB (Timer B control):**

Same as CRA, plus:

| Bits 6-5 | Timer B clock source |
|-----------|---------------------|
| 00 | System clock (E-clock, ~709 kHz) |
| 01 | CNT pin edges |
| 10 | Timer A underflow |
| 11 | Timer A underflow when CNT is high |

Timer B counting Timer A underflows enables long-period timing (up to
~2^32 E-clock cycles = ~100 minutes at PAL frequency).

### Timer Operation

1. Timer counts down from the current value each clock
2. On underflow (counter passes through 0):
   a. Counter reloads from the latch (last value written to TAxx/TBxx)
   b. ICR flag is set (Timer A: bit 0, Timer B: bit 1)
   c. If IRQ is enabled and ICR flag fires, interrupt output asserts
   d. In one-shot mode: START bit is cleared (timer stops)
   e. In continuous mode: timer continues counting from the latch value

### Auto-Start on High-Byte Write

Per the HRM (Appendix F): writing the timer high byte (TAHI/TBHI) in one-shot
mode automatically starts the timer. This is relied upon by graphics.library's
EClock calibration — it writes $FF to the low byte, then $FF to the high byte,
and the timer starts immediately.

In continuous mode, writing the high byte does not auto-start — it only updates
the latch. The timer must be started explicitly via CRA/CRB bit 0.

### Force Load

CRA bit 4 (LOAD) / CRB bit 4: writing 1 immediately loads the latch into the
counter without waiting for underflow. The bit is strobe-only — it reads back
as 0.

### Timer Read Latching

Reading TAHI latches the current timer value, and TALO returns the latched low
byte. This prevents tearing when reading a 16-bit timer that's actively
counting. The latch is transparent until TAHI is read.

## Time of Day (TOD)

Each CIA has a 24-bit TOD counter driven by an external frequency reference:

| Register | Bits | Content |
|----------|------|---------|
| TOD-LO ($x800) | 7-0 | Low byte (increments at input frequency) |
| TOD-MID ($x900) | 7-0 | Middle byte |
| TOD-HI ($xA00) | 7-0 | High byte |

### TOD Clock Source

- CIA-A TOD: driven by the power-line frequency (50 Hz PAL, 60 Hz NTSC)
- CIA-B TOD: driven by the horizontal sync pulse (~15.6 kHz)

The divider ratio depends on the input:
- CIA-A: increments once per power-line cycle
- CIA-B: increments once per hsync

### Read/Write Protocol

**Reading:** Reading TOD-HI latches all three bytes. Subsequent reads of
TOD-MID and TOD-LO return the latched values. Reading TOD-LO releases the
latch (counter updates become visible again).

**Writing:** Writing TOD-HI halts the counter. Subsequent writes to TOD-MID
and TOD-LO update the halted counter. Writing TOD-LO restarts it.

**Alarm:** When CRB bit 7 is set, writes to the TOD registers set the alarm
instead of the counter. When the counter reaches the alarm value, ICR bit 2
(ALARM) fires.

## Serial Shift Register (SDR)

Each CIA has an 8-bit shift register for serial communication:

- CRA bit 6 controls the mode:
  - 0 = input (receive): bits shift in on CNT edges
  - 1 = output (transmit): bits shift out on Timer A underflow

### Keyboard Protocol (CIA-A SDR)

The Amiga keyboard controller communicates via CIA-A's serial port:

1. **Keyboard sends data:** Each keypress sends an 8-bit code via the KDAT
   line (active on CIA-A SP pin), clocked by KCLK.

2. **Encoding:** The raw keycode is transformed: `KDAT = NOT(keycode ROL 1)`.
   The keyboard sends the bitwise inverse of the left-rotated keycode. CIA-A
   captures the inverted bits; software must decode by inverting and rotating
   right.

3. **Handshake:** After receiving a byte, software acknowledges by:
   a. Set CRA bit 6 = 1 (switch SP to output mode)
   b. Wait at least 75µs
   c. Set CRA bit 6 = 0 (switch SP back to input mode)
   The falling edge on the SP line signals the keyboard to send the next byte.

4. **Power-up sequence:** At power-on, the keyboard sends:
   - $FD (init keycode — signals keyboard is alive)
   - $FE (term keycode — signals power-up sequence complete)
   If no handshake arrives within ~141ms (100K E-clock ticks), the keyboard
   resends. During boot, 20+ bytes may be sent before KS enables the CIA-A SP
   interrupt — this is normal.

### Interrupt on SDR

When 8 bits have been shifted in (or out), ICR bit 3 (SP) fires. For keyboard
input, this means a complete keycode byte is ready in the SDR register.

## Interrupt Control Register (ICR)

The ICR is the most unusual register in the CIA — it behaves differently on
read and write.

### ICR Read ($xD00)

Returns the interrupt status flags and clears them:

| Bit | Name | Source |
|-----|------|--------|
| 7 | IR | Any interrupt is active (OR of enabled flags) |
| 4 | FLAG | FLAG pin negative edge detected |
| 3 | SP | Serial port (8 bits shifted) |
| 2 | ALARM | TOD alarm match |
| 1 | TB | Timer B underflow |
| 0 | TA | Timer A underflow |

**All bits are cleared by reading.** Software must read ICR exactly once per
interrupt and save the value — reading again returns 0.

### ICR Write ($xD00) — Interrupt Mask

Writing controls the interrupt enable mask:

| Bit | Meaning |
|-----|---------|
| 7 | SET/CLR — 1 = set listed bits, 0 = clear listed bits |
| 4-0 | Interrupt source enable bits |

This uses the same SET/CLR convention as Amiga custom registers. Writing $81
enables Timer A interrupt. Writing $01 disables it. Writing $7F disables all
sources.

### CIA-A as PORTS, CIA-B as EXTER

Paula sees the CIA interrupt outputs as single interrupt sources:
- CIA-A IRQ output → Paula INTREQ bit 3 (PORTS, level 2)
- CIA-B IRQ output → Paula INTREQ bit 13 (EXTER, level 6)

Inside the CIA interrupt handler, software reads the CIA's ICR to determine
which specific CIA source (timer, serial, alarm, flag) caused the interrupt.

## CIA-A FLAG Pin

CIA-A's FLAG input is active-low and negative-edge triggered. It's connected to
the /INDEX signal from the floppy drive (one pulse per disk rotation) and to
the parallel port BUSY signal. A falling edge on FLAG sets ICR bit 4.

On the C64, the CIA FLAG pin is used for tape turbo-loader edge detection. On
the Amiga, its primary use is the disk index pulse for rotation timing.

## E-Clock Frequency

The CIAs are clocked by the E-clock, derived from the CPU clock:

| Region | E-clock | Derivation |
|--------|---------|------------|
| PAL | 709,379 Hz | CPU clock ÷ 5 (every 10th CCK) |
| NTSC | 715,909 Hz | CPU clock ÷ 5 (every 10th CCK) |

Timer values count in E-clock ticks, not colour clocks. To convert:
- 1 E-clock tick = 10 CCKs = 5 CPU cycles
- Timer value N = N × 10 CCKs delay

Graphics.library measures one video frame with CIA-A Timer B to calibrate the
E-clock rate. The measured value (stored at GfxBase+$22) is used throughout
the OS for timing calculations. If CIA timers don't count, this value is 0
and causes a DIVU-by-zero crash in the STRAP display routine.
