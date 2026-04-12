# Amiga Paula Audio Model

Extracted from vAmiga (Dirk W. Hoffmann) and WinUAE (Toni Wilen / Antti S. Lankila).
Both emulators implement the full Paula audio subsystem: four DMA-driven state machines,
a sample-scaling DAC, and an analog filter chain. This document captures the definitive
implementation details from their source code.


---


## 1. Filter Chain Overview

The Amiga's audio output passes through an analog filter chain between the DAC and the
RCA jacks. The chain differs by hardware revision.

```
  DAC output
      |
      v
 +-----------+     +-----------+     +-----------+
 | Stage 1   |---->| Stage 2   |---->| Stage 3   |----> Line out
 | Static LP |     | LED filter|     | Static HP |
 +-----------+     +-----------+     +-----------+
   (A500/A1000       (switchable       (always on)
    only)             via CIA-A)
```

### Filter configurations by model

| Model | Stage 1 (static LP) | Stage 2 (LED) | Stage 3 (static HP) |
|-------|---------------------|---------------|---------------------|
| A500  | ON                  | ON when LED bright, OFF when LED dim | ON |
| A1000 | ON                  | Always ON (no bypass) | ON |
| A1200 | OFF (removed)       | ON when LED bright, OFF when LED dim | ON (different cutoff) |

(vAmiga AudioFilter.h:15-43, AudioFilter.cpp:268-304)


---


## 2. Filter Stage Details


### 2.1. Stage 1 -- Static Low-Pass Filter (A500/A1000 only)

**Type:** 1st-order RC low-pass (one-pole IIR)

**Component values:**

```
R = 360 ohms
C = 100 nF  (1e-7)
```

**Cutoff frequency (computed):**

```
f_c = 1 / (2 * pi * R * C)
    = 1 / (2 * pi * 360 * 1e-7)
    = 4420.97 Hz
```

(vAmiga AudioFilter.cpp:245-249)

```cpp
// vAmiga AudioFilter.cpp:245-249
void AudioFilter::setupLoFilter(double sampleRate)
{
    loFilter.clear();
    loFilter.setup(sampleRate, 360.0, 1e-7);
}
```

**IIR coefficient computation** (from cutoff frequency and sample rate):

```cpp
// vAmiga AudioFilter.cpp:30-41
void OnePoleFilter::setup(double sampleRate, double cutoff)
{
    if (cutoff >= sampleRate / 2.0) cutoff = (sampleRate / 2.0) - 1e-4;

    this->cutoff = cutoff;

    const double a = 2.0 - std::cos((2.0 * pi * cutoff) / sampleRate);
    const double b = a - std::sqrt((a * a) - 1.0);

    a1 = 1.0 - b;
    a2 = b;
}
```

This is a one-pole filter using the matched Z-transform approximation. The transfer
function is:

```
y[n] = a1 * x[n] + a2 * y[n-1]
```

Where `a2 = a - sqrt(a^2 - 1)` with `a = 2 - cos(2*pi*f_c/f_s)`, and `a1 = 1 - a2`.

**Example coefficients at 44100 Hz sample rate:**

```
a  = 2 - cos(2 * pi * 4420.97 / 44100)
   = 2 - cos(0.6296)
   = 2 - 0.8044
   = 1.1956

b  = 1.1956 - sqrt(1.1956^2 - 1)
   = 1.1956 - sqrt(0.4295)
   = 1.1956 - 0.6554
   = 0.5402

a1 = 1 - 0.5402 = 0.4598
a2 = 0.5402
```

**WinUAE cross-reference:**

WinUAE uses a different filter topology for this stage. It models two cascaded
first-order RC filters:

```cpp
// WinUAE audio.cpp:2263-2264
a500e_filter1_a0 = rc_calculate_a0(currprefs.sound_freq, 6200);
a500e_filter2_a0 = rc_calculate_a0(currprefs.sound_freq, 20000);
```

Filter 1 cutoff: **6200 Hz** (the primary static low-pass)
Filter 2 cutoff: **20000 Hz** (a gentle anti-aliasing filter)

WinUAE's coefficient formula uses bilinear transform with pre-warping:

```cpp
// WinUAE audio.cpp:2130-2143
static float rc_calculate_a0(int sample_rate, int cutoff_freq)
{
    float omega;
    if (cutoff_freq >= sample_rate / 2)
        return 1.0;

    omega = 2.0f * M_PI * cutoff_freq / sample_rate;
    omega = (float)softfloat_tan(omega / 2.0f) * 2.0f;
    float out = 1.0f / (1.0f + 1.0f / omega);
    return out;
}
```

Applied as:

```cpp
// WinUAE audio.cpp:469-471
fs->rc1 = a500e_filter1_a0 * input + (1.0f - a500e_filter1_a0) * fs->rc1;
fs->rc2 = a500e_filter2_a0 * fs->rc1 + (1.0f - a500e_filter2_a0) * fs->rc2;
normal_output = fs->rc2;
```

**Disagreement:** vAmiga uses a single pole at 4421 Hz derived from schematic values
(R=360, C=100nF). WinUAE uses two cascaded poles at 6200 Hz and 20000 Hz based on
measured frequency response. The combined effect of WinUAE's two poles gives a steeper
rolloff. The single-pole at 4421 Hz from vAmiga is a more literal reading of the
schematic, while WinUAE's 6200 Hz + 20000 Hz is a best-fit to measured hardware.


### 2.2. Stage 2 -- LED Filter (Switchable)

**Type:** 2nd-order low-pass (two-pole Butterworth-style biquad IIR)

**Component values:**

```
R1 = 10000 ohms (10 kohm)
R2 = 10000 ohms (10 kohm)
C1 = 6.8 nF     (6.8e-9)
C2 = 3.9 nF     (3.9e-9)
```

**Cutoff frequency (computed):**

```
f_c = 1 / (2 * pi * sqrt(R1 * R2 * C1 * C2))
    = 1 / (2 * pi * sqrt(10000 * 10000 * 6.8e-9 * 3.9e-9))
    = 1 / (2 * pi * sqrt(2.652e-6))
    = 1 / (2 * pi * 1.6285e-3)
    = 1 / (0.01023)
    = 9776.5 Hz
```

**Q factor (quality factor, computed):**

```
Q = sqrt(R1 * R2 * C1 * C2) / (C2 * (R1 + R2))
  = sqrt(2.652e-6) / (3.9e-9 * 20000)
  = 1.6285e-3 / 7.8e-5
  = 20.878
```

A Q factor of ~20.9 is very high -- this creates a sharp resonant peak at the cutoff
frequency before the rolloff. This is a significant feature of the Amiga's LED filter:
it does not simply cut high frequencies but produces a "ringing" resonance just before
the cutoff, giving the filter its characteristic warm sound.

(vAmiga AudioFilter.cpp:253-256)

```cpp
// vAmiga AudioFilter.cpp:253-256
void AudioFilter::setupLedFilter(double sampleRate)
{
    ledFilter.clear();
    ledFilter.setup(sampleRate, 10000.0, 10000.0, 6.8e-9, 3.9e-9);
}
```

**Coefficient computation** (second-order IIR / biquad):

```cpp
// vAmiga AudioFilter.cpp:83-98
void TwoPoleFilter::setup(double sampleRate, double cutoff, double qFactor)
{
    if (cutoff >= sampleRate / 2.0) cutoff = (sampleRate / 2.0) - 1e-4;

    this->cutoff = cutoff;
    this->qFactor = qFactor;

    const double a = 1.0 / std::tan((2.0 * pi * cutoff) / sampleRate);
    const double b = 1.0 / qFactor;

    a1 = 1.0 / (1.0 + b * a + a * a);
    a2 = 2.0 * a1;
    b1 = 2.0 * (1.0 - a*a) * a1;
    b2 = (1.0 - b * a + a * a) * a1;
}
```

This is a standard second-order IIR (biquad) low-pass filter designed via the bilinear
transform. The difference equation is:

```
y[n] = a1*x[n] + a2*x[n-1] + a1*x[n-2] - b1*y[n-1] - b2*y[n-2]
```

Note: `a2 = 2*a1`, so the numerator coefficients are `[a1, 2*a1, a1]`, which is the
standard form for a second-order low-pass (unity DC gain).

**Example coefficients at 44100 Hz sample rate:**

```
a  = 1 / tan(2 * pi * 9776.5 / 44100)
   = 1 / tan(1.3924)
   = 1 / 5.653
   = 0.1769

b  = 1 / 20.878
   = 0.04790

a1 = 1 / (1 + 0.04790 * 0.1769 + 0.1769^2)
   = 1 / (1 + 0.008474 + 0.03129)
   = 1 / 1.03977
   = 0.9617

a2 = 2 * 0.9617 = 1.9235

b1 = 2 * (1 - 0.1769^2) * 0.9617
   = 2 * (1 - 0.03129) * 0.9617
   = 2 * 0.9687 * 0.9617
   = 1.8635

b2 = (1 - 0.04790 * 0.1769 + 0.1769^2) * 0.9617
   = (1 - 0.008474 + 0.03129) * 0.9617
   = 1.02282 * 0.9617
   = 0.9837
```

**Application (Direct Form I):**

```cpp
// vAmiga AudioFilter.cpp:108-125
void TwoPoleFilter::applyLP(double &l, double &r)
{
    auto inl = l;
    auto inr = r;

    l = (a1 * inl) + (a2 * tmpL[0]) + (a1 * tmpL[1])
      - (b1 * tmpL[2]) - (b2 * tmpL[3]);
    r = (a1 * inr) + (a2 * tmpR[0]) + (a1 * tmpR[1])
      - (b1 * tmpR[2]) - (b2 * tmpR[3]);

    tmpL[1] = tmpL[0]; tmpL[0] = inl;   // input history
    tmpL[3] = tmpL[2]; tmpL[2] = l;      // output history

    tmpR[1] = tmpR[0]; tmpR[0] = inr;
    tmpR[3] = tmpR[2]; tmpR[2] = r;
}
```

State layout: `tmp[0]` = x[n-1], `tmp[1]` = x[n-2], `tmp[2]` = y[n-1], `tmp[3]` = y[n-2].

**WinUAE cross-reference:**

WinUAE models the LED filter as three cascaded first-order RC low-pass filters, all at
**7000 Hz**:

```cpp
// WinUAE audio.cpp:2265
filter_a0 = rc_calculate_a0(currprefs.sound_freq, 7000);
```

Applied as three stages in series:

```cpp
// WinUAE audio.cpp:473-475
fs->rc3 = filter_a0 * normal_output + (1 - filter_a0) * fs->rc3;
fs->rc4 = filter_a0 * fs->rc3       + (1 - filter_a0) * fs->rc4;
fs->rc5 = filter_a0 * fs->rc4       + (1 - filter_a0) * fs->rc5;
led_output = fs->rc5;
```

Three cascaded first-order filters at 7 kHz produce an 18 dB/octave rolloff (third
order). This contrasts with vAmiga's second-order Butterworth at ~9.8 kHz with Q=20.9.

WinUAE's approach gives a gentler, wider rolloff with no resonant peak. vAmiga's
approach models the actual circuit topology (Sallen-Key with specific R/C values) and
produces a resonant peak. The comments in WinUAE acknowledge the model is approximate:

```
// WinUAE audio.cpp:451-457
// The LED filter is complicated, and we are modelling it with a pair of
// RC filters, the other providing a highboost. The LED starts to cut
// into signal somewhere around 5-6 kHz, and there's some kind of highboost
// in effect above 12 kHz. Better measurements are required.
//
// The current filtering should be accurate to 2 dB with the filter on,
// and to 1 dB with the filter off.
```

**Disagreement:** vAmiga derives cutoff ~9.8 kHz from Sallen-Key component values with
a very high Q factor (~20.9), producing a resonant peak. WinUAE uses three cascaded
first-order poles at 7 kHz with no resonance. Both approximate the same hardware; vAmiga
is closer to the schematic, WinUAE is closer to measured response. The A1200 LED filter
uses the same coefficients as the A500 in both emulators -- only the static low-pass
stage is removed.


### 2.3. Stage 3 -- Static High-Pass Filter (Always On)

**Type:** 1st-order RC high-pass (one-pole IIR, applied as HP)

**Component values:**

| Model | R (ohms) | C (farads) | Cutoff (Hz) |
|-------|----------|------------|-------------|
| A500/A1000 | 1390 | 22.33 uF (2.233e-5) | 5.13 Hz |
| A1200 | 1360 | 22 uF (2.2e-5) | 5.32 Hz |

(vAmiga AudioFilter.cpp:258-266)

```cpp
// vAmiga AudioFilter.cpp:258-266
void AudioFilter::setupHiFilter(double sampleRate)
{
    hiFilter.clear();
    if (config.filterType == FilterType::A1200) {
        hiFilter.setup(sampleRate, 1360.0, 2.2e-5);
    } else {
        hiFilter.setup(sampleRate, 1390.0, 2.233e-5);
    }
}
```

The cutoff is approximately **5 Hz** -- this is a DC-blocking filter, removing any DC
offset from the audio signal. It has no audible effect on music or sound effects.

**Application:**

```cpp
// vAmiga AudioFilter.cpp:60-67
void OnePoleFilter::applyHP(double &l, double &r)
{
    tmpL = (a1 * l) + (a2 * tmpL);
    l = l - tmpL;

    tmpR = (a1 * r) + (a2 * tmpR);
    r = r - tmpR;
}
```

The high-pass is implemented by running the same one-pole low-pass and then subtracting
the result from the input: `HP(x) = x - LP(x)`.

**WinUAE cross-reference:** WinUAE does not implement a separate high-pass DC-blocking
filter in its Paula audio path. DC removal is handled elsewhere in the audio pipeline.


---


## 3. Complete Filter Coefficient Tables

### At 44100 Hz sample rate (standard)

**Stage 1 -- Static Low-Pass (vAmiga):**

| Parameter | Value |
|-----------|-------|
| Type | One-pole IIR |
| R | 360 ohms |
| C | 100 nF |
| Cutoff | 4420.97 Hz |
| a1 | ~0.460 |
| a2 | ~0.540 |

**Stage 1 -- Static Low-Pass (WinUAE):**

| Parameter | Filter 1 | Filter 2 |
|-----------|----------|----------|
| Type | One-pole IIR | One-pole IIR |
| Cutoff | 6200 Hz | 20000 Hz |
| a0 | ~0.663 | ~0.976 |

**Stage 2 -- LED Filter (vAmiga):**

| Parameter | Value |
|-----------|-------|
| Type | Two-pole IIR (biquad) |
| R1 | 10 kohm |
| R2 | 10 kohm |
| C1 | 6.8 nF |
| C2 | 3.9 nF |
| Cutoff | 9776.5 Hz |
| Q Factor | 20.878 |
| a1 | ~0.962 |
| a2 | ~1.924 |
| b1 | ~1.864 |
| b2 | ~0.984 |

**Stage 2 -- LED Filter (WinUAE):**

| Parameter | Value |
|-----------|-------|
| Type | Three cascaded one-pole IIR |
| Cutoff (each) | 7000 Hz |
| a0 | ~0.727 |

**Stage 3 -- High-Pass (vAmiga):**

| Parameter | A500/A1000 | A1200 |
|-----------|------------|-------|
| Type | One-pole HP | One-pole HP |
| R | 1390 ohms | 1360 ohms |
| C | 22.33 uF | 22 uF |
| Cutoff | 5.13 Hz | 5.32 Hz |
| a1 | ~0.000731 | ~0.000758 |
| a2 | ~0.999269 | ~0.999242 |


### At 48000 Hz sample rate

The same R/C values apply; only the digital coefficients change. The cutoff frequencies
remain identical (they are analog properties). Coefficients shift slightly because the
same analog frequency maps to a different position relative to Nyquist.


---


## 4. Audio Channel State Machine

The Paula audio subsystem has four identical channel state machines (channels 0-3). Each
implements the state diagram from the Amiga Hardware Reference Manual with states encoded
as 3-bit values.

vAmiga uses 5 states: `000`, `001`, `010`, `011`, `101`. The HRM's state `100` is not
used; vAmiga maps the "wait for DMA data" phase to state `101` instead.

(vAmiga StateMachine.cpp, StateMachineEvents.cpp, StateMachineRegs.cpp)


### 4.1. State Diagram

```
                         DMA enabled
                    +-----------------+
                    |                 |
                    v                 |
              +----------+           |
              |   000    |           |     000 = Idle
  AUDxDAT    |  (idle)  |           |     001 = Wait for DMA word 1
  written &   +----+-----+           |     101 = Wait for DMA word 2
  !AUDxIP &        |                 |     010 = Output high byte
  !DMA mode        | DMA enabled     |     011 = Output low byte
       |           v                 |
       |     +----------+           |
       |     |   001    |           |
       |     | (wait 1) |           |
       |     +----+-----+           |
       |          |                  |
       |          | AUDxDAT arrives  |
       |          | (DMA fetch)      |
       |          v                  |
       |     +----------+           |
       |     |   101    +-----------+
       |     | (wait 2) |  DMA disabled
       |     +----+-----+
       |          |
       |          | AUDxDAT arrives
       |          | (DMA fetch)
       |          v
       |     +----------+  period    +----------+
       +---->|   010    +----------->|   011    |
             | (hi byte)|            | (lo byte)|
             +----+-----+<----------+----+-----+
                  ^      period done      |
                  |                       |
                  +-----------------------+
                    period done &
                    (DMA on OR !AUDxIP)

                  If period done &
                  !DMA & AUDxIP:
                    011 -> 000 (stop)
```


### 4.2. State Transitions -- Detail

Each transition is a named function in vAmiga. The following table shows every
transition, its trigger condition, and what actions it performs.

#### 000 -> 001: DMA enabled while idle

```cpp
// vAmiga StateMachine.cpp:225-237
void StateMachine<nr>::move_000_001()
{
    assert(AUDxON());       // DMA mode only
    lencntrld();            // Reload length counter from AUDxLEN latch
    AUDxDR();               // Request DMA fetch from Agnus
    state = 0b001;
}
```

Actions: reload length counter, request first DMA word.

#### 000 -> 010: AUDxDAT written in IRQ mode

```cpp
// vAmiga StateMachine.cpp:208-223
void StateMachine<nr>::move_000_010()
{
    assert(!AUDxON());      // IRQ mode only
    assert(!AUDxIP());      // No pending interrupt
    volcntrld();            // Reload volume
    percntrld();            // Reload period counter
    pbufld1();              // Load buffer (handles attach-volume)
    AUDxIR();               // Trigger interrupt
    state = 0b010;
    penhi();                // Output high byte to DAC
}
```

This is the "software-driven" (non-DMA) audio path. The CPU writes AUDxDAT directly
and the state machine starts playing without DMA.

#### 001 -> 000: DMA disabled while waiting for first word

```cpp
// vAmiga StateMachine.cpp:239-248
void StateMachine<nr>::move_001_000()
{
    assert(!AUDxON());      // DMA turned off
    state = 0b000;
}
```

Simply returns to idle.

#### 001 -> 101: First DMA word arrives

```cpp
// vAmiga StateMachine.cpp:250-264
void StateMachine<nr>::move_001_101()
{
    assert(AUDxON());
    AUDxIR();               // Trigger interrupt (signals: "I have the address")
    AUDxDR();               // Request second DMA word
    AUDxDSR();              // Reset DMA pointer to start
    if (!lenfin()) lencount();  // Decrement length counter (unless finished)
    state = 0b101;
}
```

**This is where "the first word is discarded."** The first DMA fetch triggers an
interrupt and resets the DMA pointer, but the fetched data is not loaded into the output
buffer -- it is consumed by `AUDxDSR()` which reloads the location pointer. The actual
sample data starts with the second DMA fetch.

#### 101 -> 000: DMA disabled while waiting for second word

```cpp
// vAmiga StateMachine.cpp:266-275
void StateMachine<nr>::move_101_000()
{
    assert(!AUDxON());
    state = 0b000;
}
```

#### 101 -> 010: Second DMA word arrives

```cpp
// vAmiga StateMachine.cpp:277-292
void StateMachine<nr>::move_101_010()
{
    assert(AUDxON());
    percntrld();            // Load period counter
    volcntrld();            // Load volume
    pbufld1();              // Load output buffer (or modulate next channel)
    if (napnav()) AUDxDR(); // Request next DMA word (unless attach-period-only mode)
    state = 0b010;
    penhi();                // Output high byte
}
```

Now the channel is actively producing audio. The period counter starts running.

#### 010 -> 011: Period counter expires (high byte done)

```cpp
// vAmiga StateMachine.cpp:295-321
void StateMachine<nr>::move_010_011()
{
    percntrld();            // Reload period counter

    if (AUDxAP()) {         // Attach period mode?
        pbufld2();          // Feed data to next channel's period register
        if (AUDxON()) {
            AUDxDR();       // Request DMA
            if (intreq2) { AUDxIR(); intreq2 = false; }
        } else {
            AUDxIR();       // Trigger interrupt
        }
    }

    state = 0b011;
    penlo();                // Output low byte
}
```

After the period counter expires in state 010, the low byte of the sample word is
output. The period counter is reloaded for the low byte's duration.

#### 011 -> 010: Period counter expires (low byte done), continue playing

```cpp
// vAmiga StateMachine.cpp:348-375
void StateMachine<nr>::move_011_010()
{
    percntrld();            // Reload period counter
    pbufld1();              // Load next sample word into buffer
    volcntrld();            // Reload volume

    if (napnav()) {         // Not in attach-period-only mode?
        if (AUDxON()) {
            AUDxDR();       // Request next DMA word
            if (intreq2) { AUDxIR(); intreq2 = false; }
        } else {
            AUDxIR();       // Trigger interrupt (CPU must provide next data)
        }
    }

    state = 0b010;
    penhi();                // Output high byte of new word
}
```

This is the main playback loop: 010 -> 011 -> 010 -> 011 ...

#### 011 -> 000: Period counter expires, stop playback

Triggered when DMA is off AND the interrupt is pending (meaning the CPU has not
acknowledged the previous interrupt, so no new data is available):

```cpp
// vAmiga StateMachineEvents.cpp:24-37
void StateMachine<nr>::serviceEvent()
{
    switch (state) {
        case 0b010:
            move_010_011();
            return;

        case 0b011:
            (AUDxON() || !AUDxIP()) ? move_011_010() : move_011_000();
            return;
    }
}
```

The condition: if DMA is off AND AUDxIP is set (interrupt still pending), the channel
stops. Otherwise it loops back to 010.

```cpp
// vAmiga StateMachine.cpp:335-345
void StateMachine<nr>::move_011_000()
{
    constexpr EventSlot slot = (EventSlot)(SLOT_CH0 + nr);
    agnus.cancel<slot>();   // Cancel the period timer event
    intreq2 = false;
    state = 0b000;          // Back to idle
}
```


### 4.3. Period Counter Mechanics

The period counter determines the playback rate. It counts in DMA cycles (color clocks).

```cpp
// vAmiga StateMachine.cpp:117-125
void StateMachine<nr>::percntrld()
{
    u64 delay = DMA_CYCLES(audperLatch == 0 ? 0x10000 : audperLatch);

    if constexpr (nr == 0) agnus.scheduleRel<SLOT_CH0>(delay, CHX_PERFIN);
    if constexpr (nr == 1) agnus.scheduleRel<SLOT_CH1>(delay, CHX_PERFIN);
    if constexpr (nr == 2) agnus.scheduleRel<SLOT_CH2>(delay, CHX_PERFIN);
    if constexpr (nr == 3) agnus.scheduleRel<SLOT_CH3>(delay, CHX_PERFIN);
}
```

- Period value of 0 is treated as 0x10000 (65536) -- maximum period, lowest frequency.
- The period is specified in DMA cycles (color clocks = CPU cycles / 2 on PAL, or
  ~279.365 ns per color clock for PAL).
- Minimum documented period is 124 color clocks. Values below 124 work but produce
  frequencies above the Nyquist limit of the DAC, causing aliasing.
- Each 16-bit sample word produces two output samples (high byte then low byte), each
  held for one period duration.

**Sample rate formula:**

```
sample_rate = master_clock / (2 * period)
```

Where master_clock is 3,546,895 Hz (PAL) or 3,579,545 Hz (NTSC).

At period = 124:

```
PAL:  3546895 / (2 * 124) = 14,302.0 Hz (per byte)
NTSC: 3579545 / (2 * 124) = 14,433.6 Hz (per byte)
```

Note: each sample word produces two bytes, so the word fetch rate is half this. The
maximum byte output rate (~14.3 kHz for PAL) is well below the filter cutoffs, which is
why the Amiga's audio quality is limited.

The HRM states "maximum sample frequency of 28.86 kHz" which refers to the DMA word
fetch rate at period=124: `3546895 / 124 = 28,604 Hz` (close to 28.86 kHz with NTSC
clock).

**Anti-flood protection:**

```cpp
// vAmiga StateMachine.h:92-107
// Many games initialize AUDxPER with a value of 1 (e.g., James Pond 2 and
// Ghosts'n Goblins). As a result, the sample buffer is flooded with
// identical samples. To prevent this, these two variables hinder penlo()
// and penhi() to write into the sample buffer. The locks are released
// whenever a new sample is written into the AUDxDAT register.
bool enablePenlo = false;
bool enablePenhi = false;
```

Both flags are set to `true` when AUDxDAT is written, and cleared after penhi()/penlo()
execute once. This prevents the same sample from being written repeatedly when the
period is set to an absurdly low value.


### 4.4. AUDxDAT Write Handling

The response to writing AUDxDAT depends on whether DMA is active and the current state:

```cpp
// vAmiga StateMachineRegs.cpp:41-93
void StateMachine<nr>::pokeAUDxDAT(u16 value)
{
    auddat = value;
    enablePenlo = enablePenhi = true;   // Unlock sample output

    if (AUDxON()) {
        // DMA mode
        switch(state) {
            case 0b000: move_000_001(); break;
            case 0b001: move_001_101(); break;
            case 0b101: move_101_010(); break;
            case 0b010:
            case 0b011:
                if (!lenfin()) {
                    lencount();          // Decrement length counter
                } else {
                    lencntrld();         // Reload length counter
                    AUDxDSR();           // Reset DMA pointer
                    intreq2 = true;      // Flag: fire interrupt at next transition
                }
                break;
        }
    } else {
        // IRQ mode
        switch(state) {
            case 0b000:
                if (!AUDxIP()) move_000_010();  // Start playing if no IRQ pending
                break;
        }
    }
}
```

In DMA mode, states 010/011 handle the "length counter" logic: when the length counter
reaches 1 (`lenfin()`), the DMA pointer is reset to the start of the sample data and
an interrupt is flagged for the next state transition. This implements looping playback.

**Length counter:**

```cpp
// vAmiga StateMachine.h:235-241
void lencntrld() { audlen = audlenLatch; }  // Reload from AUDxLEN
void lencount()  { U16_DEC(audlen, 1); }    // Decrement by 1
bool lenfin()    { return audlen == 1; }     // Finished when == 1
```

The length counter counts *words* of sample data. When it reaches 1, the DMA pointer
resets. Setting AUDxLEN=1 means one word is fetched before looping; AUDxLEN=0 is treated
as 65536 words (the counter wraps).


### 4.5. WinUAE State Machine Cross-Reference

WinUAE uses the same state numbering (0, 1, 2, 3, 5) and the same basic flow:

```
0 -> 1 (DMA enabled)
0 -> 2 (AUDxDAT written, IRQ mode)
1 -> 5 (first DMA word arrives)
5 -> 2 (second DMA word arrives, start playing)
2 -> 3 (period expires, output low byte)
3 -> 2 (period expires, output high byte, fetch next word)
3 -> 0 (period expires, DMA off and IRQ pending: stop)
```

(WinUAE audio.cpp:1806-2023)

WinUAE adds several hack/compatibility features not in vAmiga:

1. **DMA wait hack** (audio.cpp:1774-1797): When DMA is rapidly toggled off and on (a
   common tracker trick), WinUAE detects this pattern and forces the channel to state 0
   to prevent note-swallowing.

2. **State 3+0x10** (audio.cpp:1948-1959): A special sub-state for period=1 handling
   that is not present in vAmiga.

3. **Delayed AUDxDAT processing** (audio.cpp:2608): AUDxDAT is processed after a 1 CCK
   delay for cycle-accurate timing.


---


## 5. DAC Model

### 5.1. Sample Format

- **Input:** Signed 8-bit samples (-128 to +127), stored as the high and low bytes of
  the 16-bit AUDxDAT register.
- **Volume:** 6-bit register (AUDxVOL), range 0-64. Not 0-63.

```cpp
// vAmiga StateMachineRegs.cpp:35-39
void StateMachine<nr>::pokeAUDxVOL(u16 value)
{
    // 1. Only the lowest 7 bits are evaluated
    // 2. All values greater than 64 are treated as 64 (max volume)
    audvolLatch = (u16)std::min(value & 0x7F, 64);
}
```

WinUAE confirms the same clamping:

```cpp
// WinUAE audio.cpp:1537-1545
static void update_volume(int nr, uae_u16 v)
{
    // 7 bit register in Paula.
    v &= 127;
    if (v > 64)
        v = 64;
    cdp->data.audvol = v;
}
```

### 5.2. Sample Scaling

The DAC output for each channel is:

```
output = (signed_8bit_sample) * volume
```

This produces a signed 14-bit result (range: -128 * 64 = -8192 to +127 * 64 = +8128).

```cpp
// vAmiga StateMachine.cpp:167-181 (penhi)
i8 sample = (i8)HI_BYTE(buffer);
i16 scaled = (i16)(sample * audvol);

// vAmiga StateMachine.cpp:191-205 (penlo)
i8 sample = (i8)LO_BYTE(buffer);
i16 scaled = (i16)(sample * audvol);
```

### 5.3. Stereo Channel Assignment

The Amiga has four audio channels with fixed stereo assignment:

| Channel | Speaker |
|---------|---------|
| 0       | Right   |
| 1       | Left    |
| 2       | Left    |
| 3       | Right   |

In vAmiga, this is implemented through configurable pan values with defaults that
reproduce the hardware assignment:

```
// vAmiga Defaults.cpp:198-201
AUD_PAN0 = 50    -> pan_value = 0.854 -> mostly right
AUD_PAN1 = 350   -> pan_value = 0.146 -> mostly left
AUD_PAN2 = 350   -> pan_value = 0.146 -> mostly left
AUD_PAN3 = 50    -> pan_value = 0.854 -> mostly right
```

The pan formula converts the integer setting to a float:

```cpp
// vAmiga AudioPort.cpp:250
pan[channel] = float(0.5 * (sin(double(config.pan[channel]) * M_PI / 200.0) + 1));
```

The mixing formula:

```cpp
// vAmiga AudioPort.cpp:472-473
double l = ch0 * (1 - pan0) + ch1 * (1 - pan1) + ch2 * (1 - pan2) + ch3 * (1 - pan3);
double r = ch0 * pan0 + ch1 * pan1 + ch2 * pan2 + ch3 * pan3;
```

WinUAE confirms the same channel mapping in its stereo handler:

```cpp
// WinUAE audio.cpp:1243-1244 (sample16s_handler)
data0 += data3;   // channels 0+3 -> right
data1 += data2;   // channels 1+2 -> left
```

(WinUAE audio.cpp:1256-1257)
```cpp
put_sound_word_right(data2);  // data2 = ch0+ch3
put_sound_word_left(data3);   // data3 = ch1+ch2
```

Note: The variable names are reused after summing (data2/data3 are overwritten with the
L/R sums). The original channel 0+3 goes right, channel 1+2 goes left.

### 5.4. Sample Interpolation

vAmiga supports three interpolation methods when resampling from the Paula's irregular
output timing to the host's fixed sample rate:

```cpp
// vAmiga Sampler.cpp:64-81
if constexpr (method == SamplingMethod::NONE) {
    return elements[r1];                        // Sample-and-hold (nearest prior)
}

if constexpr (method == SamplingMethod::NEAREST) {
    return ((clock - keys[r1]) < (keys[r2] - clock))
        ? elements[r1] : elements[r2];          // Nearest neighbor
}

if constexpr (method == SamplingMethod::LINEAR) {
    double dx = (double)(keys[r2] - keys[r1]);
    double dy = (double)(elements[r2] - elements[r1]);
    double weight = (double)(clock - keys[r1]) / dx;
    return (i16)(elements[r1] + weight * dy);    // Linear interpolation
}
```

Samples are stored in a ring buffer with timestamps (cycle counts). The interpolation
finds the two samples bracketing the target time and blends between them.


---


## 6. LED Filter Switch (CIA-A PRA Bit 1)

The LED filter is controlled by bit 1 of CIA-A's Peripheral Register A (PRA). This is
the same bit that controls the power LED brightness.

**Polarity: active low.**

When PRA bit 1 = 0: LED is bright AND filter is ON.
When PRA bit 1 = 1: LED is dim AND filter is OFF.

```cpp
// vAmiga CIA.h:696
bool powerLED() const { return (pa & 0x2) == 0; }
```

The `powerLED()` function returns `true` when bit 1 is clear (LED bright = filter on).

The filter checks this live on every sample:

```cpp
// vAmiga AudioFilter.cpp:281-291
bool AudioFilter::ledFilterEnabled() const
{
    switch (config.filterType) {
        case FilterType::A500:
        case FilterType::A1200: return ciaa.powerLED();  // Dynamic: follows LED state
        case FilterType::A1000:
        case FilterType::LED:   return true;              // Always on
        default:                return false;
    }
}
```

For A500 and A1200, the filter state tracks the LED in real time. For A1000, the filter
is always on regardless of the LED pin (the A1000 has no bypass circuit).

**WinUAE cross-reference:**

```cpp
// WinUAE audio.cpp:2784-2789
void led_filter_audio(void)
{
    led_filter_on = 0;
    if (led_filter_forced > 0 || (gui_data.powerled && led_filter_forced >= 0))
        led_filter_on = 1;
}
```

WinUAE supports three modes via `led_filter_forced`:
- `led_filter_forced = 1`: filter always on (FILTER_SOUND_ON)
- `led_filter_forced = 0`: filter follows hardware LED state (FILTER_SOUND_EMUL)
- `led_filter_forced = -1`: filter always off

The filter state is applied at sample generation time:

```cpp
// WinUAE audio.cpp:503-506
if (led_filter_on)
    o = (int)led_output;
else
    o = (int)normal_output;
```

**No settling behavior:** Neither emulator models any transition/settling time when the
filter switches on or off. The filter state changes instantaneously between samples.
On real hardware, the analog circuit would have a brief transient when the switch
engages, but this is not modeled.


---


## 7. Audio Modulation (ADKCON)

The ADKCON register controls two modulation modes that chain audio channels together:

### 7.1. Attach Volume (AUDxAV)

When enabled, the output of channel N is used to modulate the volume of channel N+1
instead of being sent to the DAC.

```cpp
// vAmiga StateMachine.cpp:147-152
bool StateMachine<nr>::AUDxAV() const
{
    return (paula.adkcon >> nr) & 0x01;
}
```

ADKCON bits 0-3 control attach-volume for channels 0-3 respectively.

When AUDxAV is set, `pbufld1()` writes the data to the next channel's volume register
instead of the output buffer:

```cpp
// vAmiga StateMachine.cpp:128-135
void StateMachine<nr>::pbufld1()
{
    if (!AUDxAV()) { buffer = auddat; return; }

    // Modulate next channel's volume
    if constexpr (nr == 0) paula.channel1.pokeAUDxVOL(auddat);
    if constexpr (nr == 1) paula.channel2.pokeAUDxVOL(auddat);
    if constexpr (nr == 2) paula.channel3.pokeAUDxVOL(auddat);
}
```

Channel 3 cannot modulate any other channel (there is no channel 4).

### 7.2. Attach Period (AUDxAP)

When enabled, the output of channel N modulates the period of channel N+1.

```cpp
// vAmiga StateMachine.cpp:153-157
bool StateMachine<nr>::AUDxAP() const
{
    return (paula.adkcon >> nr) & 0x10;
}
```

ADKCON bits 4-7 control attach-period for channels 0-3 respectively.

During the 010->011 transition, `pbufld2()` feeds data to the next channel's period:

```cpp
// vAmiga StateMachine.cpp:137-145
void StateMachine<nr>::pbufld2()
{
    assert(AUDxAP());

    if constexpr (nr == 0) paula.channel1.pokeAUDxPER(auddat);
    if constexpr (nr == 1) paula.channel2.pokeAUDxPER(auddat);
    if constexpr (nr == 2) paula.channel3.pokeAUDxPER(auddat);
}
```

### 7.3. The napnav Condition

The `napnav()` function determines whether normal DMA requests and interrupts should
fire. It returns true when the channel is NOT in attach-period-only mode:

```cpp
// vAmiga StateMachine.h:259
bool napnav() const { return !AUDxAP() || AUDxAV(); }
```

This means: "not attach-period, or attach-volume (or both)". When only attach-period is
set (AUDxAP=1, AUDxAV=0), DMA requests are suppressed because the modulating channel
does not need to fetch its own sample data.

WinUAE implements the same logic:

```cpp
// WinUAE audio.cpp:1753
int napnav = (!audav && !audap) || audav;
```


---


## 8. DMA Interaction

### 8.1. DMA Request Flow

Audio DMA requests are not handled immediately. The state machine sets a flag, and Agnus
services it during the next available DMA slot:

```cpp
// vAmiga StateMachine.h:226-268
void AUDxDR() { audDR = true; }                     // Set DMA request flag
void AUDxDSR() { agnus.reloadAUDxPT<nr>(); }        // Reset pointer to start
void requestDMA() { if (audDR) { agnus.setAudxDR<nr>(); audDR = 0; } }
```

The `requestDMA()` method is called by Agnus during the first refresh cycle of each
scan line, transferring the pending request.

### 8.2. Interrupt Timing

Audio interrupts are not fired instantly but scheduled with a 1 DMA cycle delay:

```cpp
// vAmiga StateMachine.cpp:105-114
void StateMachine<nr>::AUDxIR() const
{
    if constexpr (nr == 0) { paula.scheduleIrqRel(IrqSource::AUD0, DMA_CYCLES(1)); }
    if constexpr (nr == 1) { paula.scheduleIrqRel(IrqSource::AUD1, DMA_CYCLES(1)); }
    if constexpr (nr == 2) { paula.scheduleIrqRel(IrqSource::AUD2, DMA_CYCLES(1)); }
    if constexpr (nr == 3) { paula.scheduleIrqRel(IrqSource::AUD3, DMA_CYCLES(1)); }
}
```


---


## 9. Filter Pipeline Application

The filter pipeline is applied in the synthesize() method, which runs at the host
sample rate (typically 44100 or 48000 Hz), not at the Paula's native rate:

```cpp
// vAmiga AudioPort.cpp:448-498
template <SamplingMethod method> void
AudioPort::synthesize(Cycle clock, long count, double cyclesPerSample)
{
    // ... per-channel interpolation and volume ...

    for (isize i = 0; i < count; i++) {

        float ch0 = sampler[0].interpolate<method>((Cycle)cycle) * vol0;
        float ch1 = sampler[1].interpolate<method>((Cycle)cycle) * vol1;
        float ch2 = sampler[2].interpolate<method>((Cycle)cycle) * vol2;
        float ch3 = sampler[3].interpolate<method>((Cycle)cycle) * vol3;

        // Mix to stereo
        double l = ch0*(1-pan0) + ch1*(1-pan1) + ch2*(1-pan2) + ch3*(1-pan3);
        double r = ch0*pan0 + ch1*pan1 + ch2*pan2 + ch3*pan3;

        // Filter pipeline (in series)
        if (loEnabled)  filter.loFilter.applyLP(l, r);    // Stage 1: static LP
        if (ledEnabled) filter.ledFilter.applyLP(l, r);   // Stage 2: LED filter
        if (hiEnabled)  filter.hiFilter.applyHP(l, r);    // Stage 3: static HP

        // Master volume
        if (fading) { volL.shift(); volR.shift(); }
        l *= volL;
        r *= volR;

        stream.put(SamplePair { float(l), float(r) });
        cycle += cyclesPerSample;
    }
}
```

The filter coefficients are computed once when the sample rate or filter type changes,
not per-sample. The LED filter enable/disable is checked once at the start of each
synthesis batch, not per-sample (so the LED state is sampled at the batch boundary).


---


## 10. Cross-Reference Summary: vAmiga vs WinUAE

| Feature | vAmiga | WinUAE |
|---------|--------|--------|
| **Static LP model** | 1-pole, R=360/C=100nF, fc=4421 Hz | 2 cascaded 1-pole, fc=6200+20000 Hz |
| **LED filter model** | 2-pole biquad, fc=9777 Hz, Q=20.9 | 3 cascaded 1-pole, fc=7000 Hz each |
| **LED filter order** | 2nd order (12 dB/oct) | 3rd order (18 dB/oct) |
| **LED filter resonance** | Yes (Q=20.9, sharp peak) | No (monotonic rolloff) |
| **High-pass filter** | 1-pole HP, fc=5.1 Hz | Not explicitly modeled |
| **State count** | 5 states (000,001,010,011,101) | 5 states (0,1,2,3,5) + hack state 3+0x10 |
| **Volume clamping** | 7-bit, clamp to 64 | 7-bit, clamp to 64 |
| **Period=0 handling** | Treated as 65536 | Treated as 65536 |
| **First word discard** | Yes (001->101 transition) | Yes (state 1->5 transition) |
| **IRQ delay** | 1 DMA cycle | 1+ cycles (configurable) |
| **Sample flood protection** | enablePenhi/enablePenlo flags | Various hacks |
| **DMA wait hack** | Not implemented | Implemented (forced state 0 on rapid DMA toggle) |
| **Filter settling** | Instantaneous | Instantaneous |
| **Channel mapping** | Configurable pan (default: 0+3 right, 1+2 left) | Hard-coded: 0+3 right, 1+2 left |
| **Coefficient method** | Matched Z-transform (1-pole), bilinear (2-pole) | Bilinear transform with pre-warping |
| **Filter source** | pt2-clone by 8bitbubsy | Antti S. Lankila |


---


## 11. Source Map

### vAmiga sources

| File | Contents |
|------|----------|
| `AudioFilter.h` | Filter class declarations, pipeline structure (OnePoleFilter, TwoPoleFilter, AudioFilter) |
| `AudioFilter.cpp` | Filter coefficient computation, R/C values, filter enable logic, filter application code |
| `AudioFilterTypes.h` | FilterType enum (NONE, A500, A1000, A1200, LOW, LED, HIGH) |
| `StateMachine.h` | State machine class with all action methods, register definitions |
| `StateMachine.cpp` | State transition implementations (move_NNN_NNN functions) |
| `StateMachineEvents.cpp` | Period expiry event handler (serviceEvent) |
| `StateMachineRegs.cpp` | Register write handlers (pokeAUDxLEN/PER/VOL/DAT) |
| `StateMachineTypes.h` | StateMachineInfo struct |
| `Sampler.cpp` | Sample interpolation (NONE/NEAREST/LINEAR) |
| `AudioPort.cpp` (in Core/Ports/) | Synthesize loop, filter pipeline application, stereo mixing |
| `CIA.h` | powerLED() function (PRA bit 1 check) |
| `Defaults.cpp` | Default pan values (0+3 right, 1+2 left) |

### WinUAE sources

| File | Contents |
|------|----------|
| `audio.cpp:425-514` | Filter state, coefficient storage, filter() function |
| `audio.cpp:1537-1545` | Volume register handling (7-bit, clamp to 64) |
| `audio.cpp:1577-1596` | newsample() -- sample output |
| `audio.cpp:1746-2024` | audio_state_channel2() -- complete state machine |
| `audio.cpp:2125-2143` | rc_calculate_a0() -- coefficient computation |
| `audio.cpp:2249-2266` | Filter setup (cutoff frequencies: 6200, 20000, 7000 Hz) |
| `audio.cpp:2784-2789` | led_filter_audio() -- LED filter switch logic |


---


## 12. Key Frequencies Reference

| Description | Frequency | Source |
|-------------|-----------|--------|
| vAmiga static LP cutoff | 4421 Hz | Computed from R=360, C=100nF |
| WinUAE static LP cutoff (primary) | 6200 Hz | Measured/fitted |
| WinUAE static LP cutoff (secondary) | 20000 Hz | Anti-aliasing |
| vAmiga LED filter cutoff | 9777 Hz | Computed from R1=R2=10k, C1=6.8nF, C2=3.9nF |
| WinUAE LED filter cutoff | 7000 Hz | Measured/fitted |
| Guru Book "~7 kHz" | ~7000 Hz | Rule of thumb |
| vAmiga HP cutoff (A500) | 5.13 Hz | DC blocker, R=1390, C=22.33uF |
| vAmiga HP cutoff (A1200) | 5.32 Hz | DC blocker, R=1360, C=22uF |
| Max sample rate (PAL, period=124) | 14302 Hz (byte rate) | 3546895 / (2*124) |
| Max sample rate (NTSC, period=124) | 14434 Hz (byte rate) | 3579545 / (2*124) |
| Max DMA fetch rate (PAL, period=124) | 28604 Hz (word rate) | 3546895 / 124 |


---


## 13. Implementation Notes for Emulator Authors

### Filter implementation checklist

1. The static low-pass is a first-order IIR. Use the R/C values (R=360, C=100nF) or
   a cutoff around 4.4-6.2 kHz depending on which hardware measurements you trust.

2. The LED filter is a second-order filter. vAmiga's Sallen-Key model (Q=20.9) matches
   the circuit schematic; WinUAE's three cascaded first-order poles at 7 kHz match
   measured frequency response better. Choose based on your accuracy goals.

3. The high-pass is a DC blocker at ~5 Hz. It prevents DC offset from reaching the
   output but has no audible effect on audio content. Simple to implement but easy
   to forget.

4. The A1200 removes the static low-pass entirely. Only the LED filter and HP remain.

5. The A1000 always has the LED filter active; it cannot be bypassed. The A500 and A1200
   allow the filter to be switched via CIA-A PRA bit 1.

### State machine implementation checklist

1. Five states: idle (000), wait-DMA-1 (001), wait-DMA-2 (101), output-high (010),
   output-low (011).

2. The first DMA word is discarded (001->101 transition). Audio actually starts on the
   second word (101->010).

3. Period counter of 0 means 65536. Period counter is reloaded from the latch at each
   010->011 and 011->010 transition.

4. Volume is 7 bits wide, clamped to 64. Values 65-127 all become 64.

5. The stop condition (011->000) requires both DMA-off AND interrupt-pending. If the
   CPU acknowledges the interrupt fast enough, playback continues in IRQ mode.

6. Modulation chains: channel N modulates channel N+1. Only channels 0-2 can modulate.
   Channel 3 has no target.
