# 6581 SID Chip Technical Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Official Commodore 64 Programmer's Reference Guide, Appendix O

---

## Overview

The 6581 Sound Interface Device (SID) is a single-chip, 3-voice electronic music synthesiser/sound effects generator designed by Bob Yannes at Commodore. It is a true analogue synthesiser on a chip with programmable oscillators, envelope generators, filters, and modulation capabilities.

---

## Quick Start: Minimal Sound Program

To produce sound on the SID chip, you **must** initialize these registers in this order:

```basic
20 POKE 54277,9:POKE 54278,0   REM ADSR envelope (REQUIRED!)
30 POKE 54296,15                REM Volume to maximum
40 POKE 54273,25:POKE 54272,177 REM Frequency (both bytes)
50 POKE 54276,17                REM Gate ON + Triangle wave
60 FOR I=1 TO 1000:NEXT I       REM Delay
70 POKE 54276,16                REM Gate OFF
```

**Critical:** Without ADSR initialization (registers 54277/54278), the envelope generator has undefined behavior and may produce no sound.

---

## Memory Map

**Base Address:** 54272 ($D400)
**Address Range:** 54272 - 54296 ($D400 - $D418)

### Voice 1 Registers (54272-54278 / $D400-$D406)

| Register | Decimal | Hex | Name | Bits | Description |
|----------|---------|-----|------|------|-------------|
| 0 | 54272 | $D400 | FREQ LO | 0-7 | Frequency Low Byte |
| 1 | 54273 | $D401 | FREQ HI | 0-7 | Frequency High Byte |
| 2 | 54274 | $D402 | PW LO | 0-7 | Pulse Width Low Byte |
| 3 | 54275 | $D403 | PW HI | 0-3 | Pulse Width High Byte (bits 4-7 unused) |
| 4 | 54276 | $D404 | CONTROL | 0-7 | Waveform + Gate control |
| 5 | 54277 | $D405 | ATTACK/DECAY | 4-7 Attack, 0-3 Decay |
| 6 | 54278 | $D406 | SUSTAIN/RELEASE | 4-7 Sustain, 0-3 Release |

### Voice 2 Registers (54279-54285 / $D407-$D40D)

Identical to Voice 1, offset by +7 addresses.

### Voice 3 Registers (54286-54292 / $D40E-$D414)

Identical to Voice 1, offset by +14 addresses.

### Filter & Global Registers

| Register | Decimal | Hex | Name | Description |
|----------|---------|-----|------|-------------|
| 21 | 54293 | $D415 | FC LO | Filter Cutoff Frequency Low (bits 0-2) |
| 22 | 54294 | $D416 | FC HI | Filter Cutoff Frequency High (bits 0-7) |
| 23 | 54295 | $D417 | RES/FILT | Resonance (4-7), Voice Filter enable (0-3) |
| 24 | 54296 | $D418 | MODE/VOL | Filter Mode (4-7), Volume (0-3) |

### Read-Only Registers

| Register | Decimal | Hex | Name | Description |
|----------|---------|-----|------|-------------|
| 25 | 54297 | $D419 | POTX | Paddle X position (0-255) |
| 26 | 54298 | $D41A | POTY | Paddle Y position (0-255) |
| 27 | 54299 | $D41B | OSC 3 | Oscillator 3 output (for modulation/random) |
| 28 | 54300 | $D41C | ENV 3 | Envelope 3 output (for modulation) |

---

## Control Register (Reg 4, 11, 18)

**Register 54276 (Voice 1), 54283 (Voice 2), 54290 (Voice 3)**

| Bit | Name | Value | Description |
|-----|------|-------|-------------|
| 0 | GATE | 1 | Start note (triggers ATTACK) |
|   |      | 0 | Release note (triggers RELEASE) |
| 1 | SYNC | 1 | Synchronize with previous oscillator |
| 2 | RING MOD | 1 | Ring modulation with previous oscillator |
| 3 | TEST | 1 | Reset and lock oscillator (testing only) |
| 4 | TRIANGLE | 16 | Triangle waveform (mellow, flute-like) |
| 5 | SAWTOOTH | 32 | Sawtooth waveform (bright, brassy) |
| 6 | PULSE | 64 | Pulse waveform (variable timbre) |
| 7 | NOISE | 128 | Noise waveform (random, for effects) |

**Common Values:**
- `17` = Triangle + Gate ON (16 + 1)
- `33` = Sawtooth + Gate ON (32 + 1)
- `65` = Pulse + Gate ON (64 + 1)
- `129` = Noise + Gate ON (128 + 1)
- `16` = Triangle, Gate OFF
- `32` = Sawtooth, Gate OFF
- `64` = Pulse, Gate OFF

**Important:** Only select ONE waveform at a time (bits 4-7). Selecting multiple waveforms simultaneously produces a logical AND of the waveforms, which can cause unpredictable behavior.

---

## Frequency Calculation

### Formula

The 16-bit frequency value (FREQ HI × 256 + FREQ LO) determines pitch:

```
Fout = (Fn × FCLOCK / 16777216) Hz
```

For standard 1.02 MHz clock (NTSC):
```
Fout = (Fn × 0.059604645) Hz
```

For 0.985 MHz clock (PAL):
```
Fout = (Fn × 0.057595396) Hz
```

### Example Frequencies (NTSC)

| Note | Frequency | FREQ HI | FREQ LO | Combined Value |
|------|-----------|---------|---------|----------------|
| C-2 | 130.81 Hz | 4 | 48 | 1072 |
| A-2 | 220 Hz | 7 | 12 | 1804 |
| C-3 | 261.6 Hz | 8 | 148 | 2196 |
| A-4 | 440 Hz | 28 | 49 | 7217 |

**See Appendix E of C64 Programmer's Reference Guide for complete note table.**

---

## ADSR Envelope Generator

The envelope controls how volume changes over time when a note is played.

### Register Format

**Attack/Decay (Reg 5, 12, 19):**
- Bits 4-7: Attack rate (0-15)
- Bits 0-3: Decay rate (0-15)

**Sustain/Release (Reg 6, 13, 20):**
- Bits 4-7: Sustain level (0-15)
- Bits 0-3: Release rate (0-15)

### Envelope Rates

| Value | Attack Time | Decay/Release Time |
|-------|-------------|-------------------|
| 0 | 2 ms | 6 ms |
| 1 | 8 ms | 24 ms |
| 2 | 16 ms | 48 ms |
| 3 | 24 ms | 72 ms |
| 4 | 38 ms | 114 ms |
| 5 | 56 ms | 168 ms |
| 6 | 68 ms | 204 ms |
| 7 | 80 ms | 240 ms |
| 8 | 100 ms | 300 ms |
| 9 | 250 ms | 750 ms |
| 10 | 500 ms | 1.5 s |
| 11 | 800 ms | 2.4 s |
| 12 | 1 s | 3 s |
| 13 | 3 s | 9 s |
| 14 | 5 s | 15 s |
| 15 | 8 s | 24 s |

### Envelope Phases

1. **ATTACK:** Gate ON → Volume rises from 0 to peak
2. **DECAY:** Peak → Volume falls to Sustain level
3. **SUSTAIN:** Sustain level maintained while Gate = 1
4. **RELEASE:** Gate OFF → Volume falls to 0

### Common Envelope Settings

**Organ (instant on/off):**
- Attack: 0, Decay: 0, Sustain: 15, Release: 0
- `POKE 54277,0:POKE 54278,240`

**Piano (percussive decay):**
- Attack: 0, Decay: 9, Sustain: 0, Release: 0
- `POKE 54277,9:POKE 54278,0`

**Violin (sustained):**
- Attack: 10, Decay: 8, Sustain: 10, Release: 9
- `POKE 54277,168:POKE 54278,169`

**Simple beep (from official docs):**
- Attack: 0, Decay: 9, Sustain: 0, Release: 0
- `POKE 54277,9:POKE 54278,0`

---

## Filter

### Cutoff Frequency (Reg 21-22)

11-bit value controls filter frequency (30 Hz - 12 kHz range):
- Register 21: Bits 0-2 (low 3 bits)
- Register 22: Bits 0-7 (high 8 bits)

### Resonance & Voice Routing (Reg 23)

- Bits 4-7: Resonance (0-15) - emphasizes cutoff frequency
- Bit 0: Filter Voice 1 (1 = filtered, 0 = bypass)
- Bit 1: Filter Voice 2
- Bit 2: Filter Voice 3
- Bit 3: Filter External input

### Mode & Volume (Reg 24)

**Filter Modes (bits 4-7):**
- Bit 4: Low-Pass (full-bodied sounds)
- Bit 5: Band-Pass (thin, open sounds)
- Bit 6: High-Pass (tinny, buzzy sounds)
- Bit 7: Voice 3 OFF (mute voice 3 audio output)

**Volume (bits 0-3):**
- 0-15: Master volume (0 = silent, 15 = maximum)

**Note:** Filter modes are additive - you can combine Low-Pass + High-Pass to create Notch (Band Reject) filter.

---

## Waveform Characteristics

### Triangle (Bit 4 = 16)
- **Harmonics:** Odd only (1st, 3rd, 5th...)
- **Sound:** Mellow, flute-like, pure
- **Use:** Sustained melodic lines, bass

### Sawtooth (Bit 5 = 32)
- **Harmonics:** All (even and odd)
- **Sound:** Bright, brassy, rich
- **Use:** Brass instruments, leads, strings

### Pulse (Bit 6 = 64)
- **Harmonics:** Varies with pulse width
- **Sound:** Hollow (square) to nasal (narrow pulse)
- **Use:** Reeds, clarinets, special effects
- **Note:** Requires Pulse Width registers (2-3) to be set

### Noise (Bit 7 = 128)
- **Harmonics:** Random frequencies
- **Sound:** White noise to rumbling (depends on frequency)
- **Use:** Explosions, wind, drums, cymbals, sound effects

---

## Pulse Width

**Registers 2-3 (Voice 1), 9-10 (Voice 2), 16-17 (Voice 3)**

12-bit value controls duty cycle of Pulse waveform:

```
PWout = (PWn / 40.95) %
```

**Common Values:**
- `0` or `4095` ($FFF): DC output (constant level)
- `2048` ($800): Square wave (50% duty cycle)
- `512` ($200): Narrow pulse (12.5% duty cycle)

**Register Setup:**
```basic
REM For 50% square wave (2048):
POKE 54274,0:POKE 54275,8  REM $800 = 8*256 + 0
```

---

## Synchronization & Ring Modulation

### Oscillator Sync (Bit 1)

Synchronizes current oscillator with the previous one:
- Voice 1 syncs with Voice 3
- Voice 2 syncs with Voice 1
- Voice 3 syncs with Voice 2

Creates complex harmonic structures at the frequency of the syncing oscillator.

### Ring Modulation (Bit 2)

Replaces Triangle waveform with ring-modulated combination:
- Voice 1 ring mod with Voice 3
- Voice 2 ring mod with Voice 1
- Voice 3 ring mod with Voice 2

Produces non-harmonic overtones for bell/gong sounds.

---

## Modulation with Voice 3

### OSC 3 Output (Register 27 / $D41B)

Read-only register providing upper 8 bits of Oscillator 3 output:
- **Sawtooth:** Counts 0→255 repeatedly
- **Triangle:** Counts 0→255→0 repeatedly
- **Pulse:** Jumps between 0 and 255
- **Noise:** Random values (use as random number generator!)

**Modulation Uses:**
- Add to frequency registers → Vibrato, sirens
- Add to filter cutoff → Sample-and-hold effects
- Add to pulse width → Dynamic timbre changes

**Typical:** Set bit 7 of register 24 to mute Voice 3 audio when using for modulation.

### ENV 3 Output (Register 28 / $D41C)

Read-only register providing Envelope Generator 3 output (0-255).

**Modulation Uses:**
- Add to filter frequency → WAH-WAH, harmonic envelopes
- Add to other oscillator frequencies → Phaser effects

**Note:** Voice 3 must be gated for envelope output to change.

---

## Initialization Sequence

**Correct order for producing sound:**

1. **Clear all SID registers** (optional but recommended):
   ```basic
   FOR I=54272 TO 54296:POKE I,0:NEXT
   ```

2. **Set ADSR envelope** (REQUIRED):
   ```basic
   POKE 54277,9:POKE 54278,0  REM Attack=0, Decay=9, Sustain=0, Release=0
   ```

3. **Set master volume**:
   ```basic
   POKE 54296,15  REM Maximum volume
   ```

4. **Set frequency** (both bytes):
   ```basic
   POKE 54273,25:POKE 54272,177  REM Frequency value 6577
   ```

5. **Select waveform and gate ON**:
   ```basic
   POKE 54276,17  REM Triangle + Gate
   ```

6. **Wait** (note duration)

7. **Gate OFF**:
   ```basic
   POKE 54276,16  REM Triangle, no gate
   ```

**Why this order matters:**
- ADSR must be set before gating, or envelope behavior is undefined
- Frequency should be set before gate ON to avoid frequency glitches
- Volume must be non-zero to hear anything

---

## Common Pitfalls

### No Sound Output

1. **Missing ADSR:** Registers 54277/54278 not initialized
2. **Zero volume:** Register 54296 = 0
3. **Gate never set:** Bit 0 of control register never set to 1
4. **No waveform selected:** Bits 4-7 of control register all zero
5. **Only high frequency byte set:** Both 54272 AND 54273 must be set

### Wrong Pitch

1. **Only one frequency byte set:** Need both low and high bytes
2. **Bytes reversed:** Low byte is register N, high byte is register N+1

### Clicks/Pops

1. **Frequency changed while gate is ON:** Set frequency before gating
2. **Waveform changed during note:** Can cause phase discontinuities

---

## Hardware Specifications

- **Chip:** MOS 6581 (NTSC) / 8580 (later PAL)
- **Voices:** 3 independent
- **Frequency Range:** 0 - 4 kHz
- **Volume Range:** 48 dB (16 linear steps)
- **Filter:** 12 dB/octave rolloff, 30 Hz - 12 kHz
- **Power:** +5V (VCC), +12V (VDD)
- **Clock:** 1.02 MHz (NTSC), 0.985 MHz (PAL)

---

## Designer: Bob Yannes

The SID chip was designed by Bob Yannes, who was frustrated by the primitive sound capabilities of other computers in the early 1980s. He designed a true analogue synthesiser on a chip that rivaled dedicated music synthesizers of the era.

After leaving Commodore, Yannes co-founded **Ensoniq**, which became one of the leading professional synthesizer manufacturers in the 1980s and 1990s.

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Appendix O
- **Music Note Table:** Appendix E (8 octaves, concert A = 440 Hz)
- **Designer:** Bob Yannes
- **Additional Reading:** "Making Music on Your Commodore Computer" (Commodore)

---

## Quick Reference Card

```
Base: 54272 ($D400)

Voice 1: 54272-54278
Voice 2: 54279-54285
Voice 3: 54286-54292
Filter:  54293-54295
Volume:  54296 (bits 0-3)

Minimal Sound:
  POKE 54277,9:POKE 54278,0   ' ADSR
  POKE 54296,15                ' Volume
  POKE 54273,25:POKE 54272,177 ' Frequency
  POKE 54276,17                ' Triangle + Gate
  [wait]
  POKE 54276,16                ' Gate OFF

Control Register Values:
  17  = Triangle + Gate
  33  = Sawtooth + Gate
  65  = Pulse + Gate
  129 = Noise + Gate
  16  = Gate OFF (Triangle still selected)
```

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications
