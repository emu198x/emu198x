# Amiga I/O, Audio, and Expansion Reference

**Target audience:** Authors of hardware-accurate Amiga emulators.

**Scope:** Everything the other Amiga research docs don't cover:

- Audio (Paula DMA audio + `audio.device`)
- Serial (Paula UART + `serial.device`)
- Parallel (CIA PRB + `parallel.device`)
- Keyboard (CIA-A shift register + keyboard MPU protocol + `keyboard.device`)
- Mouse and gameport (Agnus quadrature + Paula POT + `gameport.device`)
- Input event stream (`input.device`)
- Timers (`timer.device`, CIA timer A/B, TOD, E-clock, VBlank)
- Resources (cia, potgo, misc, disk, battclock, battmem, card, keyboard)
- AutoConfig and `expansion.library`
- Zorro II electrical / bus notes

**Companion documents — do not duplicate, cross-reference:**

- `amiga-boot-process.md` — early boot, ROM, OVL, KickStart bring-up.
- `amiga-hardware-reference.md` — authoritative custom-chip register reference (ADKCON, INTENA/INTREQ, DMACON, POTGO, JOYxDAT, AUDxLCH, SERPER, SERDAT, SERDATR and the rest). This document uses those registers but does **not** re-document the bit layouts — grep them in `amiga-hardware-reference.md` when you need the address map.
- `amiga-graphics-display.md` — everything Denise/Copper/Blitter.
- `amiga-exec-kernel.md` — generic Exec device model, tasks, messages, interrupts, `Forbid`/`Permit`, IORequest, library base vectors.
- `amiga-dos-filesystem-disk.md` — AmigaDOS and `trackdisk.device`.

## How to read this document

Each subsystem has two layers:

1. **Hardware layer** — what Paula / CIA / Agnus actually do, which registers, which signals, which interrupts. Cross-ref `amiga-hardware-reference.md` for bit tables.
2. **Software layer** — how the ROM device drivers and Exec libraries arbitrate the hardware. This is what games/apps actually talk to when they are behaving, and what your emulator's ROM must *look like* to software that bypasses the drivers and hits the hardware directly.

An emulator must implement both. The hardware layer determines compatibility with games (which hit Paula/CIA directly). The software layer determines compatibility with applications (which talk through `exec.library` to `audio.device`, `serial.device`, etc.), and is also the "contract" Workbench, Preferences, and CLI programs are written against. Most 1980s games scribble on Paula; most 1990s productivity code uses devices; demos mix both, often badly.

Citations: `(HRM, §Audio)`, `(A500/A2000 TRM, §Expansion)`, `(RKM L&D, ch. 5 Audio Device)`, `(Autodocs, expansion.library/ConfigBoard)`, etc. HRM = *Amiga Hardware Reference Manual 3rd ed*. TRM = *A500/A2000 Technical Reference Manual, 1987*. RKM L&D = *ROM Kernel Reference Manual: Libraries and Devices*. Autodocs = *ROM Kernel Reference Manual: Includes and Autodocs*. Mapping = *Mapping the Amiga, 2nd ed.* SPG = Abacus *System Programmer's Guide*.

---

## Table of contents

1. [Audio — Paula DMA hardware](#1-audio--paula-dma-hardware)
2. [audio.device — software arbitration](#2-audiodevice--software-arbitration)
3. [Serial port — Paula UART (SERDAT / SERDATR / SERPER)](#3-serial-port--paula-uart)
4. [serial.device](#4-serialdevice)
5. [Parallel port — CIA-A PRB + CIA-B handshake](#5-parallel-port--cia-a-prb--cia-b-handshake)
6. [parallel.device](#6-paralleldevice)
7. [Keyboard — CIA-A SP/CNT and the MPU protocol](#7-keyboard--cia-a-spcnt-and-the-mpu-protocol)
8. [keyboard.device](#8-keyboarddevice)
9. [Mouse and gameport — JOYxDAT, POTGO, POTxDAT, CIA-A PRA](#9-mouse-and-gameport)
10. [gameport.device](#10-gameportdevice)
11. [input.device — the merged event stream](#11-inputdevice)
12. [timer.device, CIA timers, and the E-clock](#12-timerdevice-cia-timers-and-the-e-clock)
13. [Resources: cia, potgo, misc, disk, keyboard, battclock, battmem, card](#13-resources)
14. [battclock / battmem — the real-time clock](#14-battclock--battmem)
15. [misc.resource — serial/parallel port arbitration](#15-miscresource)
16. [AutoConfig, `expansion.library`, DiagArea, BootNode](#16-autoconfig-expansionlibrary-diagarea-bootnode)
17. [Zorro II electrical / bus / A500 side slot](#17-zorro-ii-electrical--bus)
18. [Chip revisions relevant to these subsystems](#18-chip-revisions)
19. [Appendix A — `ConfigDev`, `ExpansionRom`, `ExpansionControl`, `DiagArea`](#appendix-a--configdev-expansionrom-expansioncontrol-diagarea)
20. [Appendix B — Resource / device summary table](#appendix-b--resource--device-summary-table)
21. [Appendix C — ADKCON and INTENA audio-relevant bit tables](#appendix-c--adkcon-and-intena-audio-relevant-bit-tables)
22. [Appendix D — Gaps in corpus](#appendix-d--gaps-in-corpus)
23. [Appendix E — Source map](#appendix-e--source-map)

---

# 1. Audio — Paula DMA hardware

## 1.1 Overview

Paula contains four independent audio channels (HRM §Audio). Each channel has:

- An 8-bit signed DAC.
- A DMA engine that fetches 16-bit words from Chip RAM and outputs them as two sequential 8-bit signed samples (the high byte first, then the low byte — see §1.7).
- A period counter (`AUDxPER`) that controls the sample rate.
- A volume register (`AUDxVOL`) — 6-bit linear, range 0–64.
- A length counter (`AUDxLEN`) counted in **words**, not bytes.
- An interrupt request line back to the Paula interrupt logic.

Channels are hard-wired to the stereo outputs:

| Channel | Output jack |
|---------|-------------|
| 0 | Right |
| 1 | Left |
| 2 | Left |
| 3 | Right |

(HRM §Audio: "Channels 1 and 2 are connected to the left-side stereo output jack. Channels 0 and 3 are connected to the right-side output jack.")

Sample format is **signed 8-bit two's complement**, range -128 to +127. The HRM specifies `AUDxDAT` "contains 2 bytes of data that are each 2's complement and are outputted sequentially (with digital-to-analog conversion) to the audio output pins. (LSB = 3 mV)".

## 1.2 Registers — which ones matter for emulation

This document does not repeat the bit-by-bit `hardware/custom.h` map; see `amiga-hardware-reference.md`. The registers an audio emulator must implement, per channel x ∈ {0,1,2,3}:

| Register | Offset from $DFF000 | W/R | Purpose |
|----------|---------------------|-----|---------|
| `AUDxLCH` | $0A0 + x*$10 | W | High 3 bits (5 bits in ECS Agnus) of Chip RAM address of sample data. |
| `AUDxLCL` | $0A2 + x*$10 | W | Low 15 bits of sample address. Together with LCH this is an 18-bit chip RAM pointer (20-bit in ECS). |
| `AUDxLEN` | $0A4 + x*$10 | W | Length in **words**. |
| `AUDxPER` | $0A6 + x*$10 | W | Period in color-clock ticks per sample output. Minimum 124 NTSC / 123 PAL. |
| `AUDxVOL` | $0A8 + x*$10 | W | Volume 0–64. Bit 6 forces max. Bits 5–0 are the linear volume. |
| `AUDxDAT` | $0AA + x*$10 | W | "Manual" data register. Writing it is how non-DMA sound is produced. |

Plus the shared:

- `DMACON` / `DMACONR` ($096 / $002) — bits AUD0EN…AUD3EN (0..3), DMAEN (9), SET/CLR (15).
- `ADKCON` / `ADKCONR` ($09E / $010) — attach period / attach volume bits (the ATPER / ATVOL family). See §1.6 and Appendix C.
- `INTENA` / `INTREQ` / `INTENAR` / `INTREQR` ($09A / $09C / $01C / $01E) — bits AUD0..AUD3 (bits 7..10).

HRM names the interrupt priorities (HRM §System Control Hardware, figure 7-4):

| Exec pri | HW level | Description | Bit |
|---------:|---------:|-------------|----:|
| 8  | 4 | audio channel 2 (AUD2) | 8 |
| 9  | 4 | audio channel 0 (AUD0) | 7 |
| 10 | 4 | audio channel 3 (AUD3) | 10 |
| 11 | 4 | audio channel 1 (AUD1) | 9 |

Note: all four audio channels share interrupt level 4 on the 68000; the "Exec software priority" column in HRM figure 7-4 is how Exec orders handlers internally. The hardware bits are INTENA 7..10 for AUD0..AUD3, but HRM lists them in priority order not bit order, which is a recurring source of confusion.

## 1.3 Period value / sample rate math

Paula is clocked at the **color clock** — not the CPU clock. HRM §Audio gives these as authoritative:

| | NTSC | PAL |
|---|---:|---:|
| Clock constant (ticks/sec) | 3,579,545 | 3,546,895 |
| Clock interval (µs)        | 0.279365  | 0.281937  |

The period register counts down once per color-clock tick. When the counter reaches zero, the channel outputs the next sample and the period latch is reloaded. Therefore:

```
sample_rate = clock_constant / period
period      = clock_constant / sample_rate
```

For example (HRM): `period = 447` at NTSC → 3,579,545 / 447 ≈ 8 kHz. For 9600 samples/second on NTSC → 372; 28,867 samples/second → 124. The minimum period value is 124 (NTSC) or 123 (PAL), yielding the maximum theoretical sample rate of 28,867 Hz.

Why 124 not 1? Because each audio channel is allocated exactly **one DMA slot per horizontal scan line**. At 262.5 lines/frame × 59.94 fps × 2 samples/word = 31,469 samples/sec is the theoretical ceiling, but "to save buffers, the hardware is designed to handle 28,867 samples/second" (HRM §Audio, "Limitations on Selection of Sampling Period"). Pushing `AUDxPER` below 124 causes the DMA engine to *repeat the previous two bytes in the buffer register* because the Agnus DMA slot fetches can't keep up. This is the documented basis for the "audio blit" / period-sweep carrier trick described in §1.8.

PAL is the same 28,867 Hz ceiling, but because PAL's color clock is slightly slower (3,546,895 ticks/sec vs 3,579,545), the minimum period drops from 124 to 123.

## 1.4 Volume

Six-bit linear, 0..64. Bit 6 (value $40) forces max (64 ones, no zeros in the PWM). Bits 5..0 are the level (HRM Appendix A, `AUDxVOL`). The attenuation is linear in amplitude, logarithmic in dB — HRM Table 5-9 is the official mapping (0 → -∞ dB, 1 → -36.1 dB, 8 → -18.1 dB, 32 → -6.0 dB, 64 → 0 dB). For an emulator, multiply each sample by `volume / 64.0f`.

## 1.5 The audio state machine — frame-by-frame

This is the most important thing to get right for emulator accuracy. HRM §Audio, "The Audio State Machine", describes eight states per channel. The accurate version:

- Each channel is clocked at the 3.58 MHz color-clock rate (HRM).
- Three of the eight states are unused transitional states that fall back to idle (state 000).
- One path out of idle is for interrupt-driven (CPU writes `AUDxDAT`) operation; the other is for DMA-driven operation.
- **DMA-driven operation** (the common case):
  1. When `AUDxEN` + `DMAEN` first go true, state 001 is entered and Paula sends a DMA request to Agnus for the first word.
  2. "Because of pipelining in Agnus, the first data word must be thrown away." This is the famous "first-word discard" that causes pops on the first enable; it's intentional, not a bug.
  3. State 101 is entered when the first (discarded) word arrives. Paula has already requested the next word.
  4. State 010 is entered on the next word arrival. Main loop begins.
  5. Main loop: states 010 → 011 → 010 → 011 … Each transition lasts until the period counter reaches 1.
  6. In state 010, the high byte is output. In state 011, the low byte is output. **High byte first** — this matters for sample ordering. (HRM §Audio: "In the 010 state the upper byte is output, and in the 011 state the lower byte is output.")
  7. Period counter reloads at each 010→011 or 011→010 transition.
  8. Length counter decrements once per word fetched. When it reaches 1, Paula sends a "DMA restart request" to Agnus along with the next fetch. Agnus responds by reloading its pointer from `AUDxLCH`/`AUDxLCL` and the length latch reloads `AUDxLEN` into the length counter. An interrupt request goes out "just as the last word of the waveform starts its output" (HRM).
- **Interrupt-driven ("manual") operation**:
  1. DMA for that channel is disabled (`AUDxEN` = 0).
  2. CPU writes a word to `AUDxDAT`. The state machine transitions to 010, outputs the upper byte, then on period-count → 011, outputs the lower byte, and generates an interrupt.
  3. If the CPU has written a new `AUDxDAT` before the period runs out, the state machine stays in the loop. Otherwise, it falls to idle and the DAC holds the last value.

Key implication for emulators: **the interrupt fires when Agnus latches the pointer+length into Paula's "back-up" registers** — not when the last sample plays. That means after starting DMA the first interrupt arrives almost immediately (as soon as the first period elapses and the next word is fetched from chip RAM), not after the waveform has finished playing. This is the "keep one step ahead of the DMA" pattern used by all classic Amiga music drivers.

HRM §Audio "Audio DMA Example":
> As soon as DMA starts:
> a. Copy to "back-up" length register from `AUDxLEN`.
> b. Copy to "back-up" location register from `AUDxLCL` (will be used as a pointer showing current data word to fetch).
> c. Create an interrupt for the 680x0 saying that it has completed retrieving working copies of length and location registers.
> d. Start retrieving audio data each allocated DMA time slot.

So the interrupt at (c) fires right after the back-up registers are loaded. Interrupt handlers respond by writing the **next** `AUDxLCH` / `AUDxLEN`, which load into the back-up registers the next time around. This is the basis of glitch-free sample joining and DMA-based looping.

If you do not rewrite the location/length registers, "the current waveform will be repeated. Each time the length counter reaches zero, both the location and length registers are reloaded with the same values to continue the audio output." (HRM). That's how one-shot becomes looped — the repeat is automatic unless the CPU interrupt handler interrupts it.

## 1.6 Modulation — attaching channels via ADKCON

`ADKCON` bits 7..0 control audio attachment (see Appendix C for the full bit layout). Emulator-relevant subset:

- `ATPER` (attach period) on a channel sends that channel's *data words* to the **period latch** of the next-higher channel, instead of the DAC.
- `ATVOL` (attach volume) similarly sends the words to the volume latch.
- Attaching suppresses normal audio output of the modulator channel — it becomes silent and acts as a modulator.
- Channels cascade: channel 0 modulates 1, channel 1 modulates 2, channel 2 modulates 3. Channel 3 has nowhere to go, so ATPER3 and ATVOL3 exist but effectively just disable channel 3's audio output.

HRM lays it out by bit:

| Bit | Name | Effect |
|----:|------|--------|
| 7 | ATPER3 | disables channel 3 audio (no target) |
| 6 | ATPER2 | chan 2 modulates period of channel 3 |
| 5 | ATPER1 | chan 1 modulates period of channel 2 |
| 4 | ATPER0 | chan 0 modulates period of channel 1 |
| 3 | ATVOL3 | disables channel 3 audio (no target) |
| 2 | ATVOL2 | chan 2 modulates volume of channel 3 |
| 1 | ATVOL1 | chan 1 modulates volume of channel 2 |
| 0 | ATVOL0 | chan 0 modulates volume of channel 1 |

The format of the data differs in attach mode. Normally each 16-bit word is two 8-bit sample bytes. In attach-volume mode, each word is a 16-bit "volume word" where bits 6..0 are a V6..V0 volume value. In attach-period mode, each word is a 16-bit period value (bits 15..0). In mode "attach both," the modulator alternates — first word is a volume word for the modulated channel, second word is a period word, third word volume, fourth word period (HRM Table 5-4).

For emulator writers: the modulator channel's period still drives the *rate* at which modulation words are consumed. "In attach volume, requests occur as they do in normal operation (on the 011→010 transition). In attach period, a set of requests occurs on the 010→011 transition. When both attach period and attach volume are high, requests occur on both transitions." (HRM). So if you attach both, you pull modulation data at **twice** the period rate.

This modulation mechanism is how `ProTracker` and friends implement vibrato and tremolo in hardware — cheaper than reprogramming `AUDxPER` every VBlank.

## 1.7 Low-pass filter

Paula's analog output is followed by a low-pass filter with cutoff around 4–5 kHz, attenuating sharply past 7 kHz (HRM figure 5-5). This filter is enabled by default. On "A2000's with 2 layer motherboards and later A500 models" the filter can be bypassed by the CIA-A output bit that controls the brightness of the red power LED — `CIA-A $BFE001 bit 1 /LED`, same bit in `ciapra` (HRM §Audio, "Low-Pass Filter", and HRM figure for Appendix F CIA-A PRA):

- `/LED` = 0 → LED bright → filter **on** (default).
- `/LED` = 1 → LED dim → filter **off** (bypassed).

An accurate emulator should model both states. This bit is inverted from the obvious intuition (0 bright, 1 dim).

Note: A1000 and early A2000s (one-layer motherboards) do not have the filter-bypass feature — the filter is always on (HRM). Some very early A500s too.

## 1.8 The "audio blit" trick — sub-period modulation

HRM §Audio mentions this almost offhand:

> If the sampling rate is set much higher than the normal maximum sampling rate (approximately 29 KHz), the two samples in the buffer register will be repeated. If the filter on the Amiga is bypassed and the volume is set to the maximum ($40), this feature can be used to make modulated carriers up to 1.79 MHz. The modulation is placed in the memory map, with plus values in the even bytes and minus values in the odd bytes.

The mechanism: when period < 124, the DMA can't refill `AUDxDAT` fast enough, so Paula keeps re-outputting the latched high/low byte pair at up to the 3.58 MHz color-clock rate. Alternating +N / -N / +N / -N in memory produces a square-wave carrier at half the color-clock rate (1.79 MHz). Modulate the amplitude (volume) or the period and you can drive ultrasonic / RF work. This is curious trivia for emulator authors (most software doesn't do this) but modelling the buffer-repeat behavior when period < minimum *is* required for "14-bit" audio hacks that rely on using one channel at a sub-Nyquist rate to bump another channel's output.

## 1.9 Aliasing and sample-rate / length relationships

HRM §Audio is emphatic: "Aliasing distortion is eliminated when the sampling rate exceeds the output frequency by at least 7 KHz" because the analog filter's knee is around 4–5 kHz and attenuates anything above 7 kHz to the point it's masked. For music tracker-style playback, the rule of thumb is:

- Sample period 124 (28,867 Hz) down to 256 (14,000 Hz) gives full frequency response up to 7 kHz.
- Period 256..320 (14 kHz..11 kHz) starts losing the top end.
- Period > 320 begins to audibly alias on transient content.

A 14-bit audio trick: use two channels at maximum volume to cover the MSBs, plus two channels at very low volume for the LSBs, summed on the same output. Requires filter-off (CIA-A bit 1) to not smear the LSB detail. Emulator: implement the channel summing per side (ch 1+2 to left, ch 0+3 to right) before applying the filter, so the technique works.

## 1.10 ECS Agnus and `AUDxLCH` width

The original (OCS) Agnus gives `AUDxLCH` only 3 bits wide, limiting the sample pointer to 18 bits → 256 KB = standard 512 KB chip RAM addressable via the low-bit-trick. ECS Agnus ("Fatter Agnus") widens it to 5 bits, so `AUDxLCH` can address up to 20 bits → 1 MB chip RAM, or 2 MB in the "Super Agnus" A500+/A600. HRM Appendix A marks this register as "(E)" for ECS-specific extension. Emulators for ECS-and-up must mask `AUDxLCH` to 5 bits (0x1F), not 3 (0x07).

---

# 2. audio.device — software arbitration

`audio.device` (RKM L&D chapter 5) is an Exec device on top of the Paula audio hardware whose job is **channel arbitration**, not mixing. It does not do sample-rate conversion, it does not do software mixing, and it does not do voicing. When it "plays" a sound, it programs the same four Paula registers you would have programmed yourself.

Why bother? Because the Amiga is multitasking. If task A is playing music on channels 0+1 and task B wants to play a sound effect, they need to negotiate: whose music is more important? Can task B wait for the channel? The answer is `audio.device`'s precedence-based allocation protocol.

## 2.1 Why games bypass `audio.device`

Games usually take over the machine (Forbid, disable multitasking, overwrite interrupt vectors) and poke Paula directly. `audio.device` only earns its keep when multiple running tasks want audio simultaneously — Workbench bell, music player, sound effects, speech synthesis. For standalone games that aren't willing to yield a CPU slice, `audio.device`'s allocation latency is unacceptable, and its cooperation with the task scheduler is redundant. This is why emulating `audio.device` correctly is usually less important than emulating Paula correctly — but if you run Workbench or productivity software (e.g. the classic narrator.device-based speech demos, SoundEdit, tracker programs that cooperate with the system), `audio.device` behaviors do matter.

## 2.2 Allocation and arbitration

Request a subset of channels by sending `ADCMD_ALLOCATE`. You pass in an **allocation mask array**, which is a list of preferred channel combinations in priority order (RKM L&D table 5-2). For example, the array `[3, 5, 10, 12]` says: "my first choice is channels 0+1 (mask 0b0011), second choice is 0+2 (0b0101), third is 1+3 (0b1010), fourth is 2+3 (0b1100)." The device walks the array top-to-bottom and picks the first combination that can be satisfied.

The allocation command carries a **precedence** in the `ln_Pri` field of the IORequest's `io_Message.mn_Node.ln_Pri`. Range -128 (silence) to +127 (unstoppable). RKM L&D table 5-1 gives suggested values:

| Precedence | Use |
|-----------:|-----|
| 127 | Unstoppable. |
| 90–100 | Emergency alerts. |
| 80–90  | Bells, annunciators. |
| 75     | Speech (`narrator.device`). |
| 50–70  | Sonic cues tied to graphics. |
| -50..50 | Music program. |
| -70..0 | Sound effects. |
| -100..-80 | Background theme. |
| -128 | Silence / release. |

Arbitration rule: if the requested channels are free → allocate immediately and return. If they are taken, compare the requesting task's precedence to the current holders'. If the requester is strictly higher, the current holder is "stolen from" — its next command on that channel will return `AUDIO_NOALLOCATION`, and the corresponding bit in `io_Unit` is cleared so the stolen task can see which channel left. If the requester is equal or lower and the `ADIOF_NOWAIT` flag is clear, the request blocks (on a message reply) until the channels free up. If `ADIOF_NOWAIT` is set, it returns `IOERR_ALLOCFAILED` immediately.

`ADCMD_LOCK` lets a task **detect** that it's being stolen from. It's posted right after `ADCMD_ALLOCATE` and does not reply until someone with higher precedence steals the channel (or until the owner itself frees it). The typical pattern:

1. `ADCMD_ALLOCATE` with precedence P.
2. `ADCMD_LOCK`. Does not reply.
3. When another task with precedence > P allocates, the original `ADCMD_LOCK` replies with `ADIOERR_CHANNELSTOLEN`.
4. Owner has a short grace period to clean up (ramp down volume, etc.), then must issue `ADCMD_FREE`.
5. The stealing task's `ADCMD_ALLOCATE` now completes.

This is why setting precedence to 127 prevents stealing. Tasks that "must not be stolen from" (a decaying chime, a critical alert) should `ADCMD_SETPREC` to 127 at the attack, then lower it after the transient.

**Allocation key**: on successful allocate, the device fills `ioa_AllocKey` in the IOAudio block with a unique non-zero value. All subsequent commands on these channels must carry this key. When a channel is stolen, the key in the hardware's bookkeeping changes, so the old owner's key mismatches and commands error out cleanly. If you allocate multiple channels over multiple calls, pass the existing `ioa_AllocKey` to reuse it; otherwise the allocate returns a new key. Multi-channel commands (like attach-modulation setups) require all affected channels to carry the same key.

## 2.3 The command set

(`RKM L&D, ch. 5`)

**System functions:** `OpenDevice`, `CloseDevice`, `BeginIO`, `AbortIO`.

`OpenDevice` special-cases nonzero `ioa_Length`: it performs an implicit `ADCMD_ALLOCATE` with the allocation mask in `ioa_Data`, `ADIOF_NOWAIT` set. If you're not ready to allocate at open time, set `ioa_Length = 0`. `CloseDevice` auto-frees any channels still held under the `io_Unit` mask.

`BeginIO` must be used instead of `DoIO` / `SendIO` when you need to preserve the device-specific flags in `io_Flags` (bits 4..7). `DoIO` and `SendIO` zero those bits, wiping options like `ADIOF_PERVOL`, `ADIOF_SYNCCYCLE`, `ADIOF_WRITEMESSAGE`, `ADIOF_NOWAIT`.

**Allocation commands:**

- `ADCMD_ALLOCATE` — as described above.
- `ADCMD_FREE` — release channels. Performs an implicit `CMD_RESET` on them.
- `ADCMD_SETPREC` — change the current precedence on held channels.
- `ADCMD_LOCK` — as described above.

**Hardware control commands** (all take an IOAudio, `io_Unit` selects channels as a mask of bits 0..3):

- `CMD_WRITE` — play a waveform. `io_Data` = pointer to waveform (must be in chip memory, even address). `io_Length` = length in bytes, must be even. `ioa_Cycles` = number of repeats; 0 = infinite. Writing to the same channel while a write is in progress queues the new request (classic double-buffering). If the `ADIOF_PERVOL` flag is set, `ioa_Period` / `ioa_Volume` are applied at start; otherwise the last values are used. `ADIOF_WRITEMESSAGE` + `ioa_WriteMsg` gives you a second message reply at the moment the CMD_WRITE actually starts playing (distinct from when it completes).
- `ADCMD_FINISH` — abort the in-progress write on the selected channels. With `ADIOF_SYNCCYCLE`, the current waveform cycle completes before the abort, avoiding clicks at non-zero-crossings.
- `ADCMD_PERVOL` — change period and/or volume while a write is in progress. Supports `ADIOF_SYNCCYCLE` to delay until end of cycle. Used for vibrato, tremolo, envelopes.
- `CMD_FLUSH` — abort all queued `CMD_WRITE` and `ADCMD_WAITCYCLE` requests. Does not affect `ADCMD_LOCK`.
- `CMD_RESET` — restore audio hardware (attach bits, interrupt vectors) and flush. Does not unlock.
- `ADCMD_WAITCYCLE` — reply when the current cycle completes.
- `CMD_STOP` — stop current playback immediately and queue future writes.
- `CMD_START` — resume a stopped channel. Sync point for multi-channel playback.
- `CMD_READ` — return the current `CMD_WRITE` on a channel (introspection).

## 2.4 `IOAudio` structure (from RKM L&D / `devices/audio.h`)

```c
struct IOAudio {
    struct IORequest ioa_Request;     /* standard IORequest */
    struct Message  *ioa_WriteMsg;    /* second reply on write-start */
    UBYTE           *ioa_Data;        /* waveform pointer / alloc mask */
    ULONG            ioa_Length;      /* waveform length / alloc mask length */
    UWORD            ioa_Period;      /* period value 124..65535 */
    UWORD            ioa_Volume;      /* 0..64 */
    UWORD            ioa_Cycles;      /* repeat count, 0 = infinite */
    struct Message   ioa_WriteMsg_unused_union;
    UBYTE            ioa_AllocKey;    /* unique non-zero after allocate */
};
```

Note `ioa_AllocKey` is usually referenced from the IORequest in assembly. The important field order for emulator/ROM interop is: the first `sizeof(IORequest)` bytes are the standard `IORequest` embedded struct, with `io_Message`, `io_Device`, `io_Unit`, `io_Command`, `io_Flags`, `io_Error`. After that, the audio-specific extension.

---

# 3. Serial port — Paula UART

Paula contains a UART accessed through three registers: `SERPER` (write-only, sets the baud divisor and receive mode), `SERDAT` (write-only, transmit holding register), and `SERDATR` (read-only, receive data + status). (HRM §Interface Hardware, "Serial I/O Interface").

Cross-ref: full bit layouts are in `amiga-hardware-reference.md` / `hardware/custom.h`. This section describes behavior, not bit positions.

## 3.1 Baud rate

`SERPER` bits 14..0 are the divisor. Bit 15 is the receive-mode bit:

- Bit 15 = 0 → receive 8 data bits before firing RBF interrupt.
- Bit 15 = 1 → receive 9 data bits (8 data + parity, or 9 raw) before RBF.

The divisor N produces one-bit time every N+1 color clocks (NTSC 0.279365 µs, PAL 0.281937 µs each). So:

```
SERPER_bits14_0 = (clock_constant / baud) - 1
```

HRM gives the NTSC example: 9600 baud → (3,579,545 / 9600) − 1 = 371. PAL uses 3,546,895.

The minimum useful divisor is not explicitly stated in HRM, but per HRM §Interface Hardware the "maximum reliable rate is on the order of 150,000–250,000 bits per second" with "At these high rate[s] it is not possible to handle the overhead of interrupts. The receiving end will need to be in a tight read loop." The MIDI standard 31,250 baud is well within reliable range.

## 3.2 Transmit path

`SERDAT` is a 16-bit register. The data bits to transmit are packed with stop bits:

- Bit 0 is the first data bit transmitted (after the auto-generated start bit).
- 1..8 or 1..9 contain the rest of the data bits.
- *Above* the data bits, the program inserts a single 1-bit as a stop bit (or two 1-bits for two stop bits).
- All higher bits should be 0.

The hardware shifts right, outputting bit 0 first, and **stops shifting when it sees a 1 in the MSB position with all lower bits zero** (HRM): "The register stops shifting and signals 'shift register empty' (TSRE) when there is a 1 bit present in the bit-shifted-out position and the rest of the contents of the shift register are 0s."

This stop-detection mechanism means *the stop bit is effectively part of the data you write to SERDAT*, not something the hardware inserts for you (except for the start bit, which is always auto-inserted). So for 8N1:

```
bits 15..9 = 0
bit  8     = 1      (stop bit)
bits 7..0  = data
```

For 8N2:

```
bits 15..10 = 0
bit   9     = 1    (stop bit marker)
bit   8     = 1    (extra stop bit)
bits  7..0  = data
```

…because the 1 in bit 9 is the stop marker, and bit 8 being 1 adds an extra stop-bit time before shift-empty fires. Writing all zeros to `SERDAT` does nothing (no start bit generated).

**Transmit buffer empty (TBE)** interrupt fires when the data moves from `SERDAT` into the internal shift register, meaning `SERDAT` is now free for the next byte. This happens *before* the data is actually on the wire.

**Transmit shift register empty (TSRE)** is a status bit (bit 12 of `SERDATR`), not an interrupt. It indicates the wire has finished outputting. Used for half-duplex (you must wait for TSRE before driving the line-turnaround).

## 3.3 Receive path

Received bits enter a serial-to-parallel shift register. When 8 or 9 data bits have arrived (after start bit), the shifted value transfers to `SERDATR` and the **receive buffer full (RBF)** interrupt fires. The shift register can begin receiving the next frame immediately.

Software has **one character time** to read `SERDATR` and clear the RBF bit in INTREQ. If it doesn't, the next incoming character overwrites and the **OVRUN** bit (SERDATR bit 15 / INTREQ) is set.

`SERDATR` layout (HRM Appendix A, Table 8-9 in the original):

| Bit | Name | Meaning |
|----:|------|---------|
| 15 | OVRUN | Mirror of overrun INTREQ bit |
| 14 | RBF | Mirror of receive-buffer-full INTREQ bit |
| 13 | TBE | Transmit buffer empty (not a mirror) |
| 12 | TSRE | Transmit shift register empty |
| 11 | RXD | Direct read of the RXD pin |
| 10 | — | Unused |
| 9 | STP | Stop bit if 9-bit receive mode |
| 8 | STP / DB8 | Stop bit (8-bit mode) or 9th data bit (9-bit mode) |
| 7..0 | DB7..DB0 | Data |

The RBF mirror in `SERDATR` is read-only; to *clear* RBF you must write to INTREQ ($DFF09C) with the RBF bit set to clear it (bit 11, INTF_RBF). This is the classic Amiga interrupt-clear pattern.

**Break detect** — there is no dedicated break-detect interrupt. You detect break by noticing that RXD (bit 11 of SERDATR) has been low for longer than a character time. Most serial drivers watch this during RBF handling.

**Force break** on transmit: `ADKCON` bit 11 `UARTBRK` forces TXD low while set. Pulling it low then high produces a break condition.

## 3.4 DB25 pinout / modem control

The serial connector carries the standard RS-232 subset (TXD, RXD, RTS, CTS, DSR, DCD, DTR, GND) — but **none of the modem-control signals are handled by Paula**. They are routed through **CIA-B PRA** (`$BFD000`):

| CIA-B PRA bit | RS-232 | Direction |
|--------------:|--------|-----------|
| 7 | /DTR | output (driven) |
| 6 | /RTS | output (driven) |
| 5 | /CD  | input |
| 4 | /CTS | input |
| 3 | /DSR | input |

(Appendix F, HRM: "PA7..com line DTR*, driven output; PA6..com line RTS*, driven output; PA5..com line carrier detect*; PA4..com line CTS*; PA3..com line DSR*".)

RI (ring indicator) on A500/A2000 is shared with the parallel port's /SEL signal — CIA-B PRA bit 2. A jumper / wire inside the machine decides whether that pin is RI or SEL; on standard A500/A2000 builds it's SEL on the parallel side and RI on the serial side, and they cross-talk in hardware because they're the same I/O pin.

Because the modem-control lines are driven from CIA-B PRA and not from Paula, **the UART does not do any hardware flow control**. All CTS/RTS handshaking is implemented in software by `serial.device` reading CIA-B PRA on each byte. This is why serial throughput under `serial.device` tops out at around 19.2 kbaud reliably, even though the UART can push 250 kbaud; each byte requires CPU + software interrupt work to check the modem control bits. Paula's 31,250 baud MIDI mode is fine because MIDI doesn't use hardware flow control.

The audio output jacks AUD0 (left) and AUDI (right input, mixed into the analog output) are also on pins of the DB25 — this is how some modems with built-in audio / early voice modems were wired. It's useless for digital serial work but note that the pins exist. **Emulator writers: do not model audio on the serial port** except as a note in the compatibility list; no production software uses it.

## 3.5 "BREAK" handling

`ADKCON` bit 11 UARTBRK = forces TXD to 0. To send a break, set UARTBRK, wait for the desired break time (typically 250 ms), clear UARTBRK. `serial.device` exposes this via `SDCMD_BREAK` and `io_BrkTime`.

---

# 4. serial.device

(RKM L&D chapter 13)

`serial.device` is a shared-access device (optionally exclusive) that sits on top of Paula's UART + CIA-B PRA modem control. It provides:

- Baud-rate selection.
- 7 or 8 data bits, 1 or 2 stop bits.
- Parity (even/odd/off).
- XON/XOFF software flow control.
- Optional "seven-wire" (DTR/DSR/RTS/CTS) hardware flow control.
- An input ring buffer (minimum 512 bytes, typically 4096).
- `CMD_READ` / `CMD_WRITE` with optional EOF-character termination.
- Break send (`SDCMD_BREAK`).
- `SDCMD_QUERY` to inspect line status.
- `SDCMD_SETPARAMS` to change parameters (but only when no I/O is pending).
- A "RAD_BOOGIE" high-speed mode that disables parity, break detection, and XON/XOFF in exchange for higher throughput.

## 4.1 `IOExtSer` structure

(`devices/serial.h`, via RKM L&D ch.13)

```c
struct IOExtSer {
    struct IORequest IOSer;          /* standard IORequest */
    ULONG  io_CtlChar;               /* XON / XOFF / INQ / ACK packed in 4 bytes */
    ULONG  io_RBufLen;               /* input buffer size, min 512 */
    ULONG  io_ExtFlags;              /* reserved */
    ULONG  io_Baud;                  /* real baud rate 110..292000 */
    ULONG  io_BrkTime;               /* break duration in microseconds */
    struct IOTArray io_TermArray;    /* 8 EOF chars, descending */
    UBYTE  io_ReadLen;               /* bits per read char, 7 or 8 */
    UBYTE  io_WriteLen;              /* bits per write char, 7 or 8 */
    UBYTE  io_StopBits;              /* 1 or 2 (2 only if io_WriteLen == 7) */
    UBYTE  io_SerFlags;              /* see flag bits below */
    UWORD  io_Status;                /* modem control status bits */
};
```

`io_Status` bit assignments (RKM L&D, §"Setting Serial Parameters"):

| Bit | Active | Meaning |
|----:|--------|---------|
| 0 | low | Busy (PA/printer style) |
| 1 | low | Paper out |
| 2 | low | Select |
| 3 | low | Data set ready (DSR) |
| 4 | low | Clear to send (CTS) |
| 5 | low | Carrier detect (CD) |
| 6 | low | Request to send (RTS) |
| 7 | low | Data terminal ready (DTR) |
| 8 | high | Read overrun |
| 9 | high | Break sent |
| 10 | high | Break received |
| 11 | high | Transmit XOFFed |
| 12 | high | Receive XOFFed |
| 13..15 | — | Reserved |

`io_SerFlags` bits (SERB/SERF):

- `SERB_XDISABLED` — disable XON/XOFF.
- `SERB_EOFMODE` — enable `io_TermArray` EOF matching on reads.
- `SERB_SHARED` — shared access (default exclusive).
- `SERB_RAD_BOOGIE` — high-speed mode. Disables parity checking, XON/XOFF, break detection. Forces 8-bit. Sets `SERB_XDISABLED`.
- `SERB_QUEUEDBRK` — break commands queue behind pending writes (default: break preempts).
- `SERB_7WIRE` — RTS/CTS/DSR/DTR handshaking enabled (must be set at OpenDevice).
- `SERB_PARTY_ODD` — 1 = odd parity, 0 = even.
- `SERB_PARTY_ON` — enable parity.

## 4.2 Command set

- `CMD_READ` — standard read. With `SERB_EOFMODE` and a valid `io_TermArray`, the read terminates early when any byte in `io_TermArray` is encountered. `io_TermArray` must be in **descending byte order** (RKM L&D: "the array of characters be in descending order"). With `io_Length = -1`, read until a NUL (0x00) is encountered (null-terminated string mode).
- `CMD_WRITE` — standard write. `io_Length = -1` writes until NUL inclusive (so the NUL goes out the wire too).
- `SDCMD_QUERY` — fill `io_Status` with current modem bits and error flags.
- `SDCMD_SETPARAMS` — apply parameter changes (only when no I/O is pending). Reallocates `io_RBufLen` buffer if the size changed, discarding buffered data.
- `SDCMD_BREAK` — send a break for `io_BrkTime` microseconds. Either preempts pending writes or queues, depending on `SERB_QUEUEDBRK`.

## 4.3 Why ~31.25 kbaud is the practical ceiling

The hardware can push 250 kbaud (per HRM). But `serial.device`'s interrupt-driven byte-by-byte RBF handler, combined with software flow control checks and the ring buffer copy, chews CPU. At 19.2 kbaud on an unloaded 7 MHz 68000 you still have plenty of headroom. At 31.25 kbaud (MIDI), `serial.device` in `RAD_BOOGIE` mode is fine — MIDI is sparse and short messages. For general terminal / modem use above 19.2 kbaud, you need either:

- `RAD_BOOGIE` mode to disable per-byte software overhead.
- Or to bypass `serial.device` and do your own RBF ISR that shovels bytes into a private buffer.

Emulators running at real Amiga speed should reproduce this behavior by throttling `serial.device` throughput to roughly 19.2 k effective, unless the user has forced high speed. Emulators running at ≫7 MHz effective speed (cycle-exact but running on a modern host with headroom) can generally skip this nicety as long as they deliver the bytes to software correctly.

## 4.4 Exclusive vs shared

- Exclusive (default): only one task can OpenDevice it. All others get "device busy".
- Shared (`SERB_SHARED` set at open): multiple tasks can share the device. Each issues reads and the incoming bytes are routed to whichever task has an outstanding read. **This is unusual** — most apps use exclusive.

## 4.5 Opening, closing

On open, `serial.device` opens `timer.device` internally (for break timing and timeout tracking) and allocates the input buffer from the size in `io_RBufLen` (minimum 512). If zero is passed, it uses the last-used size or the default 512.

On close (the last close for shared mode), it deallocates the input buffer, closes `timer.device`, and saves the parameter settings for the next open.

## 4.6 Arbitration via `misc.resource`

Because `serial.device` and `parallel.device` share the CIA-B PRA bits (DTR/RTS/DSR/CTS/CD and POUT/BUSY/SEL respectively), they need to negotiate. `serial.device` uses `misc.resource` at OpenDevice to claim `MR_SERIALPORT` and the associated CIA-B bits (`MR_SERIALBITS`). If another driver already holds them, `OpenDevice` fails. See §15.

---

# 5. Parallel port — CIA-A PRB + CIA-B handshake

The parallel port is a true general-purpose 8-bit bidirectional I/O port routed through the CIAs. It is **not** a dedicated peripheral controller — it's just a wired-out CIA-A PRB with a couple of CIA-B handshake bits and a PC pin.

## 5.1 The data bus

The 8 data pins of the parallel connector map to **CIA-A PRB** (bits PB0..PB7, at address `$BFE101`). Data direction is set via CIA-A DDRB (`$BFE301`). Setting DDRB = $FF makes all 8 bits outputs; DDRB = $00 makes them all inputs; any mix is legal (HRM Appendix F): "PB7..P7 data 7 ... PB0..P0 data 0" — "Centronics parallel interface data".

The CIA uses the standard PR/DDR model: *reading* a PR register returns the actual pin state (whether it's driven out or sampled as input), *writing* a PR register sets the driven output value (only affects pins where DDR bit is 1).

## 5.2 Handshake lines

- **/STROBE (DRDY)** — data-ready out on the A2000/A500 is generated **automatically** by the CIA-A PC pin. HRM Appendix F: "PC...drdy* centronics control". The PC pin on the CIA "will go low on the third cycle after a port B access" (HRM Appendix F, §Handshaking). So **any write to CIA-A PRB automatically strobes PC low**, generating a /STROBE pulse on the parallel connector. This is free hardware handshake — no software intervention needed.
- **/ACK** — acknowledge in from the peripheral is routed to the CIA-A **FLAG pin**. FLAG is a negative-edge-triggered input that sets an IRQ bit. HRM Appendix F: "F....ack*". So every /ACK pulse from the printer fires a CIA-A FLAG interrupt, which Paula routes through INT2 (level 2). This is how `parallel.device` knows a byte has been accepted.

Control/status bits on the parallel connector map to **CIA-B PRA** bits PA0..PA2:

| CIA-B PRA bit | Parallel signal | Centronics name | Direction |
|--------------:|-----------------|-----------------|-----------|
| 0 | BUSY | BUSY input from printer | I/O |
| 1 | POUT | Paper out input | I/O |
| 2 | SEL  | Select input | I/O |

These are **bidirectional general-purpose pins**; their direction is set via CIA-B DDRA. By default (`parallel.device` open for output) they're inputs. Samplers and other parallel peripherals that use the port as an 8-bit I/O port with extra handshaking will reconfigure them as outputs or re-use them as extra data lines.

There is also a cross-talk: CIA-B PRA bit 0 (BUSY) is shared with the CIA-B SP pin (serial shift register pin for CIA-B, which is unused in the stock Amiga), and CIA-B PRA bit 1 (POUT) is shared with the CIA-B CNT pin. HRM Appendix F, CIA-B block:

```
PA2..SEL          centronics control
PA1..POUT         paper out
PA0..BUSY         busy
SP...BUSY
CNT..POUT
```

This is used by some creative peripherals to get a hardware serial shift register on the parallel port.

## 5.3 Output timing

Output cycle (HRM §Interface Hardware, "Parallel Connector Interface Timing, Output Cycle"):

1. CPU writes data to CIA-A PRB.
2. Within ~5.3 µs the DRDY (PC) pin goes low.
3. Peripheral sees data ready, latches data, pulls /ACK low.
4. /ACK edge fires CIA-A FLAG interrupt.
5. Software clears interrupt, writes next byte → PC pin goes low again.

Software interaction is essentially "write byte, wait for FLAG interrupt, repeat." `parallel.device` does the interrupt handling.

Input cycle is symmetric — the external device writes data onto the bus (DDR must be set to input), pulses /ACK, CIA fires FLAG, software reads PRB. In this mode /STROBE is unused or repurposed (the CIA PC pin still gets pulsed on PRB access, but since the CPU is reading not writing, it's a harmless by-product — unless the peripheral interprets /STROBE on read as a "data accepted" signal, which many Centronics-style printers do not but some samplers do).

## 5.4 Bidirectional use

`parallel.device` can work in either output or input mode, switching DDRB as needed. A single byte-write or byte-read cycle does not auto-swap DDRB — the driver has to do it. That makes byte-at-a-time bidirectional use slow. Most "bidirectional parallel" peripherals work by convention (software knows the direction), not by tight ping-pong.

## 5.5 /RESET

The parallel connector exposes the system /RESET line. A peripheral can reset the Amiga by pulling this low, and a peripheral can also be reset by the Amiga resetting.

## 5.6 Why the parallel port is a magnet for peripherals

Because it's just a CIA with automatic /STROBE and a FLAG-interrupt-driven /ACK, any device that can speak bytes-with-handshake can plug into it. Famous examples:

- **Printers** (Centronics, the designed use).
- **Audio samplers** (Perfect Sound, AMAS, etc.). Typically bit-bang: set DDRB to input, sample the 8 bits at a fast rate from a CIA-A PRB read loop. Used `CIA-A timer` for rate control.
- **Digitisers and video grabbers**.
- **Parallel-to-SCSI adapters**.

For emulators: nothing on the parallel port is cycle-critical in the way disk is, but the tight-loop samplers will be sensitive to CIA-A PRB read timing if the emulator doesn't model CIA read latency correctly.

---

# 6. parallel.device

(RKM L&D chapter 14)

`parallel.device` wraps the bit-level CIA mess in an Exec device. It's much simpler than `serial.device`: a handful of flags, buffered read/write with EOF termination, no baud rate (rate is determined by the peripheral's /ACK handshake speed).

## 6.1 `IOExtPar` structure

```c
struct IOExtPar {
    struct IORequest IOPar;
    ULONG  io_PExtFlags;              /* reserved */
    UWORD  io_Status;                 /* status bits */
    UBYTE  io_ParFlags;               /* see PARB_* */
    struct IOTArray io_PTermArray;    /* 8 EOF bytes, descending */
};
```

`io_ParFlags` (PARB_):

- `PARB_EOFMODE` — enable EOF terminator matching on reads via `io_PTermArray`.
- `PARB_SHARED` — shared open (default is exclusive).

## 6.2 Command set

- `CMD_READ` — read bytes. `io_Length = -1` for NUL-terminated. Optional EOF termination.
- `CMD_WRITE` — write bytes. `io_Length = -1` to write until NUL (inclusive).
- `PDCMD_QUERY` — fill `io_Status`.
- `PDCMD_SETPARAMS` — change EOF array / flags.

Error codes (`ParErr_*`): DevBusy, BufToBig, InvParam, LineErr, NotOpen, PortReset, InitErr.

On open, `parallel.device` opens `timer.device` internally and claims the parallel bits via `misc.resource` (MR_PARALLELPORT / MR_PARALLELBITS). If exclusive and someone else owns the port, open fails.

## 6.3 What `parallel.device` does **not** do

- It does not implement the Centronics-level status polling of BUSY / POUT / SEL beyond returning them in `io_Status`. Most printer drivers (including the Workbench `printer.device`) check these bits themselves.
- It does not manage DDR direction for bidirectional use; `CMD_WRITE` assumes output.
- It does not handle paper-out / offline / error cases intelligently — that's the job of the higher-level `printer.device`.

---

# 7. Keyboard — CIA-A SP/CNT and the MPU protocol

The Amiga keyboard is a **separate microcontroller** (6500/1 or 6570-036 on various models, running firmware) connected to the main unit by a four-wire cable: +5V, GND, KDAT, KCLK (HRM Appendix G). The keyboard MPU does:

- Matrix scanning.
- Debouncing.
- Phantom key detection (A500 and later).
- N-key rollover (limited, with phantom suppression).
- Buffering up to 10 keys.
- Generating raw key codes.
- Handshake with the host.
- LED control (caps lock only; power LED is on the motherboard).
- Self-test and error indication (via blinking caps-lock LED).
- Special codes for sync loss, type-ahead buffer full, etc.
- Reset protocol (Ctrl-LAmiga-RAmiga).

The host side receives keycodes via **CIA-A's serial shift register** (SDR at `$BFEC01`). CIA-A's SP pin is connected to KDAT and CNT to KCLK (HRM Appendix F). The keyboard drives KCLK; the host never drives KCLK (KDAT is bidirectional, but only for the handshake pulse).

## 7.1 Electrical / protocol

- Both KCLK and KDAT idle high (+5V). "The KDAT line is active low; that is, a high level (+5V) is interpreted as 0, and a low level (0V) is interpreted as 1." (HRM Appendix G).
- **Keyboard transmits a byte** (8 bits) by:
  1. Putting the first data bit on KDAT.
  2. Pulsing KCLK low (for ~20 µs), then high.
  3. Between bits, keyboard waits about 20 µs with KDAT stable before pulsing KCLK again.
  4. Continues for all 8 bits.
  5. After the last bit, keyboard releases KDAT high.
- **Bit order**: transmitted order is bits 6-5-4-3-2-1-0-**7** (HRM Appendix G). So the byte is **rotated left by 1** before transmission. The reason: bit 7 is the up/down flag, and they wanted it sent **last** so that if sync is lost mid-byte, the missing bit is the up/down flag; a garbage byte then appears as a key release (safer than as a key press).
- **CIA-A serial register**: CIA-A's SP is configured as an **input** and CNT is the shift clock. Each KCLK pulse shifts one bit into the CIA SDR. After 8 CNT pulses the CIA generates an SP-full interrupt, which is visible on CIA-A ICR bit 3. The SDR value is the "rotated" byte, so software must rotate right by 1 to recover the actual keycode.
- **Handshake**: after the host sees the CIA SDR interrupt, it must pulse KDAT low "for at least 1 (one) microsecond" to acknowledge receipt (HRM Appendix G). In practice, software **must pulse it low for 85 µs** to ensure compatibility with all keyboard revisions. This is done by briefly switching CIA-A SP from input to output, writing a 0, waiting, then switching back to input.
- **Timeout**: if the keyboard doesn't get the handshake within 143 ms, it assumes sync has been lost and enters **resync mode** (see below).
- Bit rate during transmission is ~60 µs per bit → ~17 kbit/s.

## 7.2 Raw keycodes

(HRM §Interface Hardware and Appendix G)

- 00–3F: positional keys on the main area. The legend on top of the keys varies by country; the code is strictly positional.
- 40–5F: common keys (Space, Backspace, Tab, Numeric pad Enter, Return, Escape, Delete, arrow keys, Function keys F1–F10, keypad parens/slash/asterisk/plus, Help).
- 60–67: qualifier keys (LShift, RShift, CapsLock, Control, LAlt, RAlt, LAmiga, RAmiga).
- 78: "Reset warning" special code — indicates Ctrl-LAmiga-RAmiga has been pressed, reset imminent.
- F9: Last key was bad, retransmission follows.
- FA: Keyboard output buffer overflow.
- FC: Self-test failed (also caps-lock LED blinks).
- FD: Initiate power-up key stream (for keys held at power-on).
- FE: Terminate power-up key stream.

Key-up encoding: the up/down bit is bit 7. Key-down transmits the raw code; key-up transmits `raw_code | 0x80`. **Exception**: CapsLock only transmits on down, never on up. The up/down bit indicates current state of the LED (0 = LED on / caps active, 1 = LED off / caps inactive).

## 7.3 Lost-sync resync protocol

If the keyboard transmits and never gets a handshake (e.g. host locked up and missed the CIA interrupt), after 143 ms:

1. Keyboard clocks out a single 1 bit, waits for handshake.
2. If 143 ms passes again, clocks out another 1 bit.
3. Repeat until handshake arrives.
4. Once handshake arrives, the keyboard transmits a "lost sync" code (F9), then retransmits the garbled byte.

The garbage character from the resync appears as a key-up (because the resync clocks out 1s, and 1s in the up/down flag = up). That's the reason for the bit rotation in §7.1 — it makes garbage-from-resync look like a release, not a press.

## 7.4 Power-up sequence

1. Keyboard runs self-test (ROM checksum, RAM test, watchdog test).
2. If failed, blink caps-lock LED: 1 blink = ROM, 2 = RAM, 3 = watchdog, 4 = matrix short (per HRM Appendix G).
3. Syncs up with host via slow 1-bit clock-out until handshake arrives.
4. Transmits FC if self-test failed (no handshake expected).
5. Transmits FD, then all currently-pressed keycodes (as down), then FE. This is how key-held-at-power-on is reported.
6. Normal operation begins.

The 500, 2000, 3000, 1200, 4000 keyboards all implement this. The original A1000 keyboard is similar but with slight protocol differences.

## 7.5 Ctrl-Amiga-Amiga reset

Keyboard-initiated reset:

1. User presses Ctrl + LAmiga + RAmiga.
2. Keyboard transmits "reset warning" code (78) — **twice**, because the host must handshake the first one normally, else hard reset proceeds immediately.
3. On the second 78, the host has 250 ms to drive KDAT low — else hard reset proceeds.
4. Once the host drives KDAT low, it has **10 seconds** of "emergency processing" time. During this time the keyboard is waiting.
5. When the host releases KDAT high, the keyboard asserts hard reset by pulling KCLK low for 500 ms, which the motherboard circuit detects and turns into a real /RESET pulse.

The 10-second window is the one `KBD_ADDRESETHANDLER` handlers run in — see §8. Software uses this to flush disk caches, save state, etc. before reset.

Note: reset warning is only available on **some** A1000 and A2000 keyboards. The A500 doesn't implement it; reset is immediate. Emulators should expose reset-warning as a configurable option.

---

# 8. keyboard.device

(RKM L&D chapter 10)

`keyboard.device` provides higher-level access to the keyboard than directly banging CIA-A SDR. It produces **`InputEvent` chains**, de-rotates raw keycodes, tracks qualifier state, and manages the reset-handler chain.

Under normal operation, the `input.device` task eats all the keyboard events before applications see them. So applications that want keyboard input use the `console.device` or Intuition's IDCMP, not `keyboard.device` directly. The direct interface matters for:

- Writing a reset handler.
- Reading the full key matrix (games that need N-key rollover beyond what the hardware's event stream gives).
- Bypassing Intuition (games that take over the machine).

## 8.1 Commands

Standard: `OpenDevice`, `CloseDevice`, `DoIO`, `SendIO`, `AbortIO`.

Specific:

- `KBD_READMATRIX` — read the current 6×16 bit matrix state into a 16-byte buffer. Each bit = current physical state of one key (1 = down, 0 = up). Lets you see simultaneously-held keys directly from the matrix, bypassing the ghost-suppression that the keyboard MPU applies to the event stream.
- `KBD_READEVENT` — read one (or more) keyboard `InputEvent`s from the queue. Not normally used by applications — use `input.device` instead.
- `KBD_ADDRESETHANDLER` — add a reset handler to the prioritized chain.
- `KBD_REMRESETHANDLER` — remove a reset handler.
- `KBD_RESETHANDLERDONE` — signal that this handler is finished with its cleanup work, the next handler in the chain can run.

## 8.2 Reset handler chain

When Ctrl-LAmiga-RAmiga is pressed, `keyboard.device` calls all registered reset handlers in priority order (high priority first). Each handler is an `Interrupt` structure:

```c
struct Interrupt {
    struct Node is_Node;
    APTR       is_Data;   /* passed to handler in A1 */
    VOID     (*is_Code)();/* called */
};
```

The handler is called with `is_Data` in A1. It has a limited time (fraction of the 10-second grace window divided across handlers) to do its work, then it **must** issue a `KBD_RESETHANDLERDONE` to let the next handler run. If it doesn't, the whole chain blocks.

Reset handlers commonly run on **software interrupt context**, so they may not call `Wait()` — use `SendIO` not `DoIO` when issuing `KBD_RESETHANDLERDONE`.

## 8.3 InputEvent as keyboard output

The `KBD_READEVENT` command fills an `InputEvent` with:

- `ie_Class = IECLASS_RAWKEY` — raw, non-translated keyboard event.
- `ie_Code` = raw key code (0x00..0x67), or `raw_code | IECODE_UP_PREFIX` (0x80) for a key up.
- `ie_Qualifier` = current state of qualifier keys:
  - `IEQUALIFIER_LSHIFT` (0x0001)
  - `IEQUALIFIER_RSHIFT` (0x0002)
  - `IEQUALIFIER_CAPSLOCK` (0x0004)
  - `IEQUALIFIER_CONTROL` (0x0008)
  - `IEQUALIFIER_LALT` (0x0010)
  - `IEQUALIFIER_RALT` (0x0020)
  - `IEQUALIFIER_LCOMMAND` (0x0040) — left Amiga key
  - `IEQUALIFIER_RCOMMAND` (0x0080) — right Amiga key
  - `IEQUALIFIER_NUMERICPAD` (0x0100) — the key comes from numeric pad
  - `IEQUALIFIER_REPEAT` (0x0200) — this is a repeat
  - Plus gameport / mouse qualifiers (button state).
- `ie_TimeStamp` = timeval when the event was produced.
- `ie_NextEvent` = link to next event in chain (for event chains).

`IECLASS_RAWKEY` means "unprocessed raw key with qualifiers" — no character translation has been done. `console.device` is responsible for translating raw key + qualifier → character according to the keymap (`keymap.library` in later versions). An emulator that boots the ROM and runs Workbench must provide both layers.

## 8.4 Dead keys / compose keys

Dead keys (accent keys like ` ´ ~ ^ ¨) are handled at the `console.device` level via the keymap, not at `keyboard.device`. `console.device` maintains a compose-state machine. The raw events that come through `keyboard.device` are just sequential key events; the "dead" behavior is imposed above.

---

# 9. Mouse and gameport

## 9.1 Registers

(HRM §Interface Hardware, "Controller Port Interface")

- `JOY0DAT` ($DFF00A) — port 1 (left port) counters in mouse mode, or switch states in joystick mode.
- `JOY1DAT` ($DFF00C) — port 2 (right port) counters/switches.
- `POT0DAT` ($DFF012) — port 1 proportional (paddle/X-Y joystick) counters.
- `POT1DAT` ($DFF014) — port 2 proportional counters.
- `POTGO` ($DFF034, write-only) — potentiometer start + GPIO control.
- `POTGOR` / `POTINP` ($DFF016, read-only) — pot state + GPIO read.
- `CIAAPRA` ($BFE001) — fire buttons on bits 6, 7 (port 1 / port 2 respectively).
- `BPLCON0` bit 3 (LPEN) — enable light pen latch of beam counter.

## 9.2 Mouse (quadrature) mode

When a quadrature input (mouse, trackball, quadrature-driving controller) is plugged into a port, its two directions each produce two pulse trains 90° out of phase. Agnus contains 8-bit up/down counters, one per axis per port, that decode the quadrature automatically. `JOY0DAT`/`JOY1DAT` exposes them (HRM):

- Bits 15..8 = vertical count.
- Bits 7..0 = horizontal count.

Counters wrap modulo 256. They're **hardware counters that tick every quadrature edge** — movement right or down increments, left or up decrements.

Software reads the counter at a fixed interval (usually VBlank, once per 1/60 s NTSC or 1/50 s PAL) and subtracts the previous reading to get a delta. Because the counter wraps at 256 and a fast mouse move can cross the wrap point, software interprets the difference as a signed 8-bit value (-128..+127). If you move the mouse fast enough to generate more than 127 counts per sample interval, you lose direction — the HRM gives 38 in/sec as the upper bound at VBlank sampling (200 counts/inch × 1/60 s), and recommends reading twice per frame for fast games.

**Mouse buttons**:

- **Left** button is routed to CIA-A PRA: bit 6 for port 1, bit 7 for port 2. Active low (0 = pressed).
- **Right** button is routed to pin 9 of the controller connector, which is one of the POT inputs. Software reads it through POTGOR.
- **Middle** button (when present — three-button mice) is on pin 5, the other POT input.

## 9.3 Digital joystick mode

Digital joysticks use the same connectors and put switch states into `JOYxDAT`. Encoding is deliberately chosen so that it looks like a mouse quadrature pattern for some classes of useful input. HRM Table 8-3:

| Bit | Meaning |
|-----|---------|
| 1 | Right (switch closed = 1) |
| 9 | Left (switch closed = 1) |
| 1 XOR 0 | Back (switch closed = 1) |
| 9 XOR 8 | Forward (switch closed = 1) |

Fire buttons: left fire on pin 6 → CIA-A PRA bit 6 (port 1) or bit 7 (port 2). Second fire button on pin 9 (right mouse button style) via POTGOR. Some joysticks have both.

## 9.4 Proportional (paddle / X-Y pot) mode

For variable-resistance controllers (pots, paddles, light pen), Paula contains 8-bit integrating A/D converters. POT0DAT and POT1DAT are counters in 2×8-bit format (bits 15..8 = Y pot, bits 7..0 = X pot for each port).

Reading cycle (HRM §Interface Hardware, "Reading Proportional Controllers"):

1. During vertical blanking, CPU writes `POTGO` with the START bit set and OUTxY / OUTxX output-enable bits cleared for the pot channels you want to measure. For a standard X-Y joystick, write $0001 to POTGO.
2. For the first 7 (NTSC) or 8 (PAL) horizontal scan lines, the capacitors are discharged and counters held reset.
3. After the reset window, the caps begin charging through the external resistor. Once the cap voltage crosses a preset threshold, the counter stops. Until then it increments once per horizontal line.
4. Counter values are valid for read at the next VBlank.
5. To start another measurement, write POTGO again.

Max pot resistance is 470 kΩ ± 10% (528 kΩ absolute max per HRM), with 0.047 µF cap. Full-scale charge time is 16.6 ms (one video frame). This is why you read pots at VBlank.

The four pot pins (two per port) are also usable as a **4-bit bidirectional GPIO** via the same POTGO register — bits OUTRY/DATRY, OUTRX/DATRX, OUTLY/DATLY, OUTLX/DATLX (HRM Table 8-4). Setting an OUTxy bit to 1 disconnects the pot circuit from that pin and turns it into a GPIO output; the state of DATxy is the driven logic level.

Mouse right and middle buttons use this GPIO function: set the DAT bit to 1 (so that pull-up on the pin tells the pin to read back as 1 when not pressed) and read POTGOR to see if the peripheral is pulling the pin to ground.

**Light pen**: pin 6 is the "beam trigger". When BPLCON0 bit 3 (LPEN) is set and the light pen is pointing at the screen, the raster beam passing under the pen triggers a latch in Agnus that freezes VPOSR / VHPOSR (vertical/horizontal beam counter). Software reads them to get the XY position of the pen. On A1000 the light pen is wired to port 1; on A500 and later it's port 2 by default, port 1 by internal jumper.

## 9.5 Mouse velocity calculation (standard pattern)

```
delta_x = (current_x - previous_x) & 0xFF
if delta_x > 127: delta_x -= 256
similarly for y
previous_x = current_x
```

Emulator note: the counter is 8-bit and wraps, so treating the read as unsigned and doing signed subtract mod 256 is the correct interpretation. Reading twice per frame halves the max-velocity-before-ambiguity.

## 9.6 Pot resistor timing — implementation note

The pot charging curve is exponential. HRM approximates it as linear for a short window and gets 8-bit resolution across the vertical scan. An emulator should model the counter as:

```
ticks_needed = scale * (resistance / max_resistance)
```

(roughly linear over the 0..255 range), with the value clamping at 255 at end-of-frame. The scale is calibrated so that 470 kΩ ≈ full scale. For synthetic paddles, you can just linearly map 0..255 without modelling the RC curve.

---

# 10. gameport.device

(RKM L&D chapter 11)

`gameport.device` is an **exclusive-access** device with two units: unit 0 = front/left gameport (connector 1, the one Intuition dedicates to the mouse), unit 1 = rear/right gameport (connector 2). OpenDevice fails if another task already has the unit.

## 10.1 Commands

- `GPD_SETCTYPE` — declare the type of controller connected. Types: `GPCT_NOCONTROLLER` (0), `GPCT_MOUSE` (1), `GPCT_ABSJOYSTICK` (2), `GPCT_RELJOYSTICK` (3). The device won't report events until you set a type other than `GPCT_NOCONTROLLER`.
- `GPD_ASKCTYPE` — read current type setting.
- `GPD_SETTRIGGER` — set conditions under which the device generates events.
- `GPD_ASKTRIGGER` — read current trigger settings.
- `GPD_READEVENT` — read one or more events (blocking until a trigger fires).

Standard: `OpenDevice`, `CloseDevice`, `DoIO`, `SendIO`, `AbortIO`.

## 10.2 Controller types

- `GPCT_MOUSE` — quadrature-decoded position events, two or three buttons, X/Y deltas per event. The Amiga mouse, trackball, and most quadrature driving controllers.
- `GPCT_ABSJOYSTICK` — digital joystick that reports one event per position change (e.g. "moved to forward-left"). Click-style, not autorepeating.
- `GPCT_RELJOYSTICK` — same but with autorepeat — as long as the user holds the stick, events keep coming at the configured period.
- `GPCT_NOCONTROLLER` — disable.

## 10.3 `GamePortTrigger`

```c
struct GamePortTrigger {
    UWORD gpt_Keys;       /* GPTF_DOWNKEYS / GPTF_UPKEYS */
    UWORD gpt_Timeout;    /* max interval between reports in vblank ticks */
    UWORD gpt_XDelta;     /* X movement threshold */
    UWORD gpt_YDelta;     /* Y movement threshold */
};
```

`gpt_Keys` = `GPTF_UPKEYS | GPTF_DOWNKEYS` means report both press and release button transitions. `gpt_Timeout` = number of VBlank ticks (1/60 s each NTSC, 1/50 s PAL) to wait before generating a timeout event even if nothing else happens. `gpt_XDelta` / `gpt_YDelta` = minimum mouse movement in either axis to trigger a report; smaller movements are accumulated until the next trigger.

Events produced:

- `IECLASS_RAWMOUSE` for mouse movement/button events.
- `IECLASS_POINTERPOS` — in later ROMs, absolute pointer position events (used by tablets).
- Within RAWMOUSE, `ie_Code` specifies which button:
  - `IECODE_LBUTTON` (0x68) — left button
  - `IECODE_RBUTTON` (0x69) — right button
  - `IECODE_MBUTTON` (0x6A) — middle button
  - ORed with `IECODE_UP_PREFIX` (0x80) for release events.
  - `IECODE_NOBUTTON` (0xFF) for pure movement or timeout events.
- `ie_X`, `ie_Y` contain relative deltas since last event.

## 10.4 Mutual exclusion with the mouse

Because `input.device` normally claims unit 0 for the system pointer, tasks wanting unit 0 will fail OpenDevice. Workaround: either use unit 1, or call `input.device` and have it remap the pointer unit with `IND_SETMPORT`. Games that take over the machine shut down `input.device` first, then grab the mouse directly.

Example (RKM L&D §GAMEPORT): the sample program uses unit 1 specifically because unit 0 is held by `input.device`.

---

# 11. input.device

(RKM L&D chapter 9)

`input.device` is the task that merges:

- `keyboard.device` raw events.
- `gameport.device` mouse/joystick events.
- `timer.device` timer-tick events.
- Injected "disk inserted" / "disk removed" events from AmigaDOS.
- Events from any handler in the chain.

…into a **single prioritized event stream**. Handlers subscribe to the stream with a priority; events flow from high priority to low. Intuition registers itself at priority 50. If you add a handler at priority 51 or higher, you see the raw event stream *before* Intuition.

A handler can:

- Pass the event through unchanged (return the pointer it received).
- Modify the event in place (update fields, change class).
- Unlink the event from the chain (shorten the chain).
- Add new events to the chain (used to inject synthetic input).
- Terminate the chain by returning NULL (no handler downstream sees anything).

## 11.1 InputEvent structure

```c
struct InputEvent {
    struct InputEvent *ie_NextEvent;  /* link to next event */
    UBYTE ie_Class;                   /* IECLASS_* */
    UBYTE ie_SubClass;                /* subclass */
    UWORD ie_Code;                    /* event-specific code */
    UWORD ie_Qualifier;               /* IEQUALIFIER_* */
    union {
        struct { WORD ie_x, ie_y; } ie_xy;
        APTR  ie_addr;
    } ie_position;
    struct timeval ie_TimeStamp;
};
```

`ie_position` is overloaded: for mouse/pointer events it's `ie_X`/`ie_Y` (relative deltas); for some events it's a pointer to extra data.

## 11.2 IECLASS list

- `IECLASS_NULL` (0) — no event, placeholder.
- `IECLASS_RAWKEY` (1) — raw keyboard event (code = raw keycode ± 0x80).
- `IECLASS_RAWMOUSE` (2) — mouse/joystick event from gameport.
- `IECLASS_EVENT` (3) — generic event.
- `IECLASS_POINTERPOS` (4) — absolute pointer position (tablets).
- `IECLASS_TIMER` (6) — timer tick from `timer.device`.
- `IECLASS_GADGETDOWN` (7), `IECLASS_GADGETUP` (8) — Intuition gadget events.
- `IECLASS_REQUESTER` (9) — Intuition requester events.
- `IECLASS_MENULIST` (10) — menu events.
- `IECLASS_CLOSEWINDOW` (11), `IECLASS_RAWPORT` (12), etc.
- `IECLASS_NEWPOINTERPOS` (13) — V36+ absolute pointer.
- `IECLASS_DISKREMOVED` (14), `IECLASS_DISKINSERTED` (15) — from AmigaDOS.

Intuition consumes RAWKEY, RAWMOUSE, TIMER (for double-click detection), and DISKREMOVED/INSERTED. It produces GADGETDOWN, GADGETUP, REQUESTER, MENULIST, CLOSEWINDOW etc.

## 11.3 Handler interface

A handler is passed in R28 (A0) a pointer to the first event in a chain and in A1 a pointer to its private data. It must preserve all registers except scratch. It returns the (possibly modified) chain pointer in D0.

The `IND_ADDHANDLER` command takes an `Interrupt` structure pointer:

```c
struct Interrupt {
    struct Node is_Node;       /* ln_Pri controls priority */
    APTR       is_Data;        /* passed in A1 */
    VOID     (*is_Code)();     /* called */
};
```

Handlers run in software-interrupt context (actually a task running at a high internal priority), so they can't `Wait()`, can't do disk I/O, must be fast.

## 11.4 Commands

- `IND_ADDHANDLER` — insert handler at the priority position in `is_Node.ln_Pri`.
- `IND_REMHANDLER` — remove.
- `IND_WRITEEVENT` — inject an event chain into the stream.
- `IND_SETTHRESH` — set the key-repeat hold-time (how long before repeat starts).
- `IND_SETPERIOD` — set the repeat rate (time between repeats).
- `IND_SETMPORT` — set which gameport is the mouse.
- `IND_SETMTRIG` — set the minimum mouse move thresholds for generating events.
- `IND_SETMTYPE` — set the mouse device type (mouse / joystick / no controller).

Set*Thresh and Set*Period are normally called by Preferences / Intuition based on Workbench settings.

## 11.5 Memory caveat

"When a task finally responds to the message, the allocated memory is not returned to the system until the window is closed. Therefore, a task that chooses not to respond to its incoming messages for a long period of time can potentially remove a great deal of memory from the system free-memory list." (RKM L&D §Input Device)

## 11.6 V36+ commodities layer

Kickstart 2.0 (V36) added `commodities.library` which sits on top of `input.device`. It provides a higher-level event router (CxObject / InputXpression pattern), letting multiple "commodities" register matching patterns against the event stream. This is the basis for hotkey apps, autosaver, screen blanker, etc. in the 2.0+ Workbench environment. Emulators should implement `commodities.library` if they target 2.x and higher.

---

# 12. timer.device, CIA timers, and the E-clock

## 12.1 Device overview

(RKM L&D chapter 6)

`timer.device` has **two units** on all Amiga ROMs:

- `UNIT_VBLANK` — ticks on vertical blank interrupts. Precision ±16.67 ms (NTSC) or ±20 ms (PAL). Very low overhead. Use for anything ≥ 1 second.
- `UNIT_MICROHZ` — uses CIA-B Timer A, microsecond precision. Higher overhead. Use for short intervals.

Kickstart 2.0+ adds:

- `UNIT_ECLOCK` — ticks at the 68000 E-clock rate (~715 kHz, = CPU clock / 10).
- `UNIT_WAITUNTILECLOCK` — wait until a specific absolute E-clock value.

## 12.2 Command set

- `TR_ADDREQUEST` — main command. Fill `tr_time` with the duration (tv_secs, tv_micro), send it, get reply when the time expires.
- `TR_GETSYSTIME` — get current system time (seconds + micros since boot or wall clock).
- `TR_SETSYSTIME` — set system time. Takes a timeval.
- Standard: `CMD_FLUSH`, `CMD_CLEAR`.

Library functions (in `amiga.lib`, dispatched through the device base):

- `AddTime(dest, src)` — dest += src.
- `SubTime(dest, src)` — dest -= src.
- `CmpTime(dest, src)` — compare. Returns +1, 0, -1.
- `GetSysTime(tv)` — convenience.
- `SetSysTime(tv)` — convenience.

## 12.3 `timeval` struct

```c
struct timeval {
    ULONG tv_secs;
    ULONG tv_micro;    /* 0..999999 */
};
```

Must be normalized (tv_micro < 1,000,000). Requests use the duration-from-now interpretation: "signal me in 30 seconds" not "signal me at time T."

Multiple queued requests are sorted by the driver and served in order of expiry.

The timer driver uses the base `IORequest` (not an extended struct) followed by a `timeval`, bundled as `struct timeRequest`:

```c
struct timerequest {
    struct IORequest tr_node;
    struct timeval   tr_time;
};
```

The driver destroys `tr_time` on each call, so software must re-initialize before re-submitting.

## 12.4 How UNIT_VBLANK gets its tick

It uses the **VBlank interrupt** (INT3, Paula-bit 5, priority level 3). Each VBlank, the driver walks its request queue and replies to any requests whose time has come. Because VBlank occurs every 16.67 ms (NTSC) or 20 ms (PAL), the effective resolution is one frame time, and the accuracy over long periods is very good (the VBlank rate is locked to the 60/50 Hz refresh).

Actually: the "system time" maintained by `timer.device` is driven by `ciaa.tod`, which is a **24-bit counter clocked by the external 50/60 Hz power-supply sync signal**, not VBlank. See §12.5 below and §13 (cia.resource).

## 12.5 The TOD counters

Both CIAs have a 24-bit "Time Of Day" counter (three bytes: todlow, todmid, todhi) clocked by an external tick signal.

- **CIA-A TOD** is clocked by a **50/60 Hz line tick** derived from the power supply (or VBlank on NTSC). This is the "system clock tick" — Intuition uses it for wall-clock time, `timer.device` uses it to drive `UNIT_VBLANK`. Very stable over long time periods.
- **CIA-B TOD** is clocked by a **horizontal sync** signal. Ticks once per scan line. Useful for timing events synchronous to the raster.

HRM Appendix F TOD register block:

```
BFE801   todlo     50/60 Hz event counter bits 7-0 (VSync or line tick)
BFE901   todmid    50/60 Hz event counter bits 15-8
BFEA01   todhi     50/60 Hz event counter bits 23-16
```

TOD supports a programmable alarm register (at the same addresses, CRB7=1 selects alarm-write mode). When the TOD value equals the alarm value, a TOD interrupt fires.

Reading TOD: read MSB first — this latches all three bytes. Read LSB last — this unlatches. Reading out of order yields carry artifacts. This is documented in HRM Appendix F.

Writing TOD: writes go to the real counter by default, or the alarm if CRB7=1. **Writing stops the TOD counter until you write LSB** — this ensures you don't start counting from a half-updated value.

## 12.6 UNIT_MICROHZ and CIA-B Timer A

`UNIT_MICROHZ` uses CIA-B Timer A as a countdown timer running at the E-clock rate. E-clock is CPU clock / 10 = 0.715909 MHz NTSC, 0.709379 MHz PAL, or ~1.3968 µs per tick NTSC, ~1.4097 µs per tick PAL. For an N-microsecond delay:

```
TA = N / 1.3968   (NTSC)
```

The CIA Timer A is a 16-bit counter, so the max single-shot delay is 65535 × 1.3968 µs ≈ 91.5 ms NTSC. For longer delays, the driver chains multiple underflows or uses the timer in continuous mode and counts underflows.

`cia.resource` (§13) is responsible for arbitrating who owns CIA-B Timer A. `timer.device` claims it at open time. Other drivers that want CIA-B Timer A (rare, but possible) must go through `cia.resource/AddICRVector`.

## 12.7 UNIT_ECLOCK (Kickstart 2.0+)

`UNIT_ECLOCK` reports absolute 64-bit E-clock ticks as an `EClockVal` (two ULONGs). It's used for high-precision timing that's immune to the wall-clock time being set backwards. `ReadEClock()` returns the current E-clock value and the library base frequency.

## 12.8 System time vs wall-clock time

`timer.device`'s system time starts at zero on boot. AmigaDOS then sets it (if unset and the boot disk has a datestamp) to the last-modified time of the boot floppy. Later, the battery-backed RTC (see §14) if present is consulted to set absolute time.

"System time is stable to within a few seconds a day" (RKM L&D ch.6). In addition, "system time is changed every time someone asks what time it is using TR_GETSYSTIME. This way the return value of the system time is unique and unrepeating. This allows system time to be used as a unique identifier."

That last sentence is curious — it means calling `GetSysTime` in a tight loop returns monotonically increasing values, even within the same microsecond. Useful for generating unique IDs.

## 12.9 Exec `WaitTOF`

`graphics.library/WaitTOF()` waits for the next top-of-frame (VBlank). It's functionally equivalent to `timer.device` UNIT_VBLANK with `tr_time = { 0, 1 }`, but it's faster because it doesn't go through the device chain. Use `WaitTOF` for frame-synced loops.

---

# 13. Resources

A **resource** is a lightweight library-like object (opened with `OpenResource`, not `OpenLibrary`) that wraps shared hardware state below the device level. The key ones:

## 13.1 cia.resource

Two instances: `ciaa.resource` and `ciab.resource`. Each exposes the base address of its respective CIA chip and arbitrates access to the ICR (interrupt control register) so that drivers needing specific CIA interrupts can coexist.

Functions (Autodocs):

- `AddICRVector(icrBit, interrupt)` — add an interrupt server to a CIA interrupt source. `icrBit` is 0..4 (Timer A underflow, Timer B underflow, TOD alarm, SP/SDR full, FLAG pin). `interrupt` is an `Interrupt` struct.
- `RemICRVector(icrBit, interrupt)` — remove.
- `AbleICR(mask)` — mask/unmask ICR bits. Like Exec's `Disable`/`Enable` but at the CIA level.
- `SetICR(mask)` — set or clear ICR bits on a specific chip.

The point of `cia.resource`: there's a single CIA per resource, and multiple drivers may need to react to different interrupt sources on it. Directly poking the CIA ICR from a driver would race with other drivers. `AddICRVector` serializes access and chains multiple drivers on a single interrupt source.

Default owners at boot:

- **CIA-A**: `keyboard.device` owns SP (SDR full) for keyboard. `audio.device` and others may use FLAG. The ICR interrupts into INT2 (level 2).
- **CIA-B**: `trackdisk.device` owns FLAG (disk index). `timer.device` owns Timer A (MICROHZ). `serial.device` and various drivers tap the TOD. ICR interrupts into INT6 (level 6).

## 13.2 potgo.resource

Arbitrates access to the POTGO register's four GPIO bits (OUTLX/DATLX, OUTLY/DATLY, OUTRX/DATRX, OUTRY/DATRY). Functions:

- `AllocPotBits(mask)` — request a subset of the 4-bit direction/data fields.
- `FreePotBits(mask)`.
- `WritePotgo(value, mask)` — atomic write-through-mask to POTGO.
- `ReadPotgo()` — read POTINP.

Because the pot pins are shared between multiple possible functions (mouse buttons, GPIO, pot A/D), multiple drivers need to reach into POTGO. `potgo.resource` prevents races.

## 13.3 misc.resource

See §15 below. Arbitrates ownership of the **serial port**, **parallel port**, **serial bits** (CIA-B DTR/CTS/DSR/etc. bits), and **parallel bits** (CIA-B BUSY/POUT/SEL bits).

## 13.4 disk.resource

Arbitrates CIA-B PRB bits used for drive select / motor / step. Multiple devices can access the disk hardware (`trackdisk.device`, third-party drivers) via `disk.resource` which serializes access. See `amiga-dos-filesystem-disk.md` for the detail.

## 13.5 keyboard.resource

Wraps the raw CIA-A shift register interface for the keyboard. `keyboard.device` sits on top of `keyboard.resource`. Third-party keyboard drivers (e.g. a plug-in that wants to remap at the raw level) can use `keyboard.resource` to intercept the input before `keyboard.device` sees it.

## 13.6 battclock.resource and battmem.resource

See §14.

## 13.7 card.resource (PCMCIA)

Introduced in Kickstart 2.05 (A600 and A1200). PCMCIA slot management:

- `OwnCard` / `ReleaseCard` — claim/release the slot for a specific driver.
- `GetCardMap` — get a pointer to the PCMCIA attribute memory.
- `BeginCardAccess` / `EndCardAccess` — bracket accesses for power management.
- `ReadCardStatus` — read card state.

Emulators targeting A600/A1200 need to implement PCMCIA signaling and `card.resource` for cards like the Squirrel SCSI or network adapters.

---

# 14. battclock / battmem

## 14.1 Chip

The real-time clock on the A500 (when fitted via a trapdoor expansion), A2000, A3000, A4000, A1200 is typically **Oki MSM6242B** (or pin-compatible Ricoh RP5C01A on some boards).

The chip has:

- BCD-encoded seconds, minutes, hours (12- or 24-hour), day-of-week, day, month, year.
- Alarm registers.
- A "hold" mode for coherent reads.
- Small amount of battery-backed RAM (varies by chip — MSM6242B has none, RP5C01A has 13 nibbles of user RAM).

Mapped into CPU address space at $DC0000–$DCFFFF (A2000 and later). A500 without a clock has nothing at this range.

## 14.2 battclock.resource

Functions (from Autodocs):

- `ResetBattClock()` — zero the clock (generally don't).
- `ReadBattClock()` — returns a ULONG seconds count since midnight 1 Jan 1978.
- `WriteBattClock(seconds)` — set the clock.

These functions hide the BCD encoding and chip variant from callers. They also handle the "hold" / "coherent read" sequence (read all registers while held, then release).

## 14.3 battmem.resource

Functions:

- `ObtainBattSemaphore()` / `ReleaseBattSemaphore()` — exclusive access.
- `ReadBattMem(buffer, offset, length)` — read battery RAM.
- `WriteBattMem(buffer, offset, length)` — write.

Battery RAM is used to persist small Workbench preferences, system state, and similar. Contents are not fixed; the ROM may use it for boot-path hints, and utilities may use it for screen mode, serial port settings, etc.

## 14.4 BCD register layout

For Oki MSM6242B (the most common Amiga RTC):

| Register | Offset | Meaning |
|----------|--------|---------|
| S1 | 0 | Seconds units (BCD) |
| S10 | 1 | Seconds tens |
| MI1 | 2 | Minutes units |
| MI10 | 3 | Minutes tens |
| H1 | 4 | Hours units |
| H10 | 5 | Hours tens + 12/24 mode |
| D1 | 6 | Day units |
| D10 | 7 | Day tens |
| MO1 | 8 | Month units |
| MO10 | 9 | Month tens |
| Y1 | 10 | Year units |
| Y10 | 11 | Year tens |
| W | 12 | Day-of-week |
| CD | 13 | Control D — hold (bit 0 = 1 freezes counter for read) |
| CE | 14 | Control E — interrupts/mode |
| CF | 15 | Control F — 12/24 hour, stop, reset |

Each "nibble" register is 4 bits wide. The chip is memory-mapped as byte-wide registers but only the low 4 bits are significant. Address spacing on the Amiga is typically 2 or 4 bytes per register (depending on board layout) — the MSM6242B is at $DC0000 with registers at $DC0003, $DC0007, $DC000B, $DC000F, etc. (every 4 bytes, low 4 bits of byte).

## 14.5 Reading the clock (coherent)

1. Write `CD |= HOLD_BIT` (bit 0).
2. Read S1..Y10 and W in any order.
3. Write `CD &= ~HOLD_BIT`.

Without the hold, you may catch a carry mid-read (e.g. read seconds=59, then the minute ticks, then you read minutes=N+1; you now have seconds=59, minutes=N+1 which didn't exist).

## 14.6 Century

The chip stores 2-digit year (00–99). Amiga software assumes 1978 as the epoch and extends the year to 4 digits by adding 1900 if Y >= 78 else 2000. Y2K rolls over to 00, which software maps to 2000. Post-2077 is ambiguous.

---

# 15. misc.resource

Serial/parallel port arbitration. Units:

- `MR_SERIALPORT` — the Paula UART hardware.
- `MR_PARALLELPORT` — the CIA-A PRB parallel data lines.
- `MR_SERIALBITS` — the CIA-B PRA bits used by serial (DTR/RTS/CTS/DSR/CD).
- `MR_PARALLELBITS` — the CIA-B PRA bits used by parallel (BUSY/POUT/SEL).

Functions (called in assembly only — the resource has a nonstandard calling convention for historical reasons, per Autodocs):

- `AllocMiscResource(unitNum, name)` — claim the unit. Returns NULL on success, or a string identifying the current owner on failure.
- `FreeMiscResource(unitNum)` — release.

Rationale (from Autodocs):

> The misc.resource must be accessed using assembly language. The set of functions available are too small to justify the overhead of a real library interface.

Why it exists: `serial.device` and some other driver (say, a MIDI driver that wants raw UART access) both want to touch SERDAT/SERPER. Whoever gets there first claims `MR_SERIALPORT`; the second open fails.

On open, `serial.device` claims `MR_SERIALPORT` and `MR_SERIALBITS`. Similarly `parallel.device` claims `MR_PARALLELPORT` and `MR_PARALLELBITS`. Freed on close.

Emulators should implement `misc.resource` minimally (stub the calls to succeed) unless they want to be compatible with drivers that intentionally take over serial/parallel hardware from `serial.device`.

---

# 16. AutoConfig, `expansion.library`, DiagArea, BootNode

## 16.1 AutoConfig protocol

(TRM §Auto Configuration, and Autodocs expansion.library)

AutoConfig is the Amiga's mechanism for discovering and placing expansion cards into the 68000 address space without jumper switches. It was innovative for 1985 and is the direct ancestor of PCI plug-and-play in spirit.

### 16.1.1 The geographic window

All unconfigured expansion cards (PICs, "Plug-In Cards") appear at the **64 KB window starting at $E80000** when `CONFIGIN*` (CFGIN) is asserted to them. Only one PIC at a time can respond — the daisy-chain enforces this.

**Daisy chain**: on the Zorro bus, each slot has its own CFGINn input and CFGOUTn output. CFGOUT of slot 1 is wired to CFGIN of slot 2, and so on. The first slot's CFGIN comes from the motherboard and is asserted as soon as reset deasserts. Initially, CFGOUT of a PIC is **negated**, meaning the chain is broken — only the current PIC sees an asserted CFGIN, so only it responds at $E80000.

When the CPU finishes configuring a PIC (assigning it a base address or "shutting it up"), that PIC asserts its CFGOUT, which propagates to the next slot's CFGIN, so the next PIC now appears at $E80000. Repeat until all slots are done.

Empty slots don't break the chain: the backplane wires their CFGIN through to CFGOUT automatically. So you don't have to populate slots in order.

### 16.1.2 What the CPU reads from $E80000

The PIC's ROM area is a series of **nibble-encoded** bytes. Each byte is split into two nibbles, high nibble at offset K and low nibble at offset K+2 (byte offset), where the nibbles appear on D15–D12 of the bus (TRM §Auto-Config, "Address Specification Table"). So a 16-byte logical ROM occupies 64 bytes of address space.

All nibbles are **one's-complement-encoded** except the first two (offsets 0/2 which hold `er_Type`). Software reads, converts to bytes, and inverts where needed. `ReadExpansionByte(board, offset)` knows the layout.

The ROM layout corresponds to the `ExpansionRom` struct:

| Byte offset | Meaning |
|------------:|---------|
| 0 | `er_Type` — board type + memory size |
| 1 | `er_Product` — manufacturer's product number |
| 2 | `er_Flags` — flags |
| 3 | reserved |
| 4–5 | `er_Manufacturer` — 16-bit manufacturer ID assigned by Commodore |
| 6–9 | `er_SerialNumber` — 32-bit serial |
| 10–11 | `er_InitDiagVec` — if valid, offset from board base of DiagArea |
| 12–15 | reserved |

`er_Type` breakdown:

| Bit(s) | Name | Purpose |
|-------:|------|---------|
| 7..6 | TYPEMASK | 11 = current-style Zorro II board; 00/01/10 reserved |
| 5 | MEMLIST | 1 = add this board's memory to the free memory list |
| 4 | DIAGVALID | 1 = `er_InitDiagVec` is valid |
| 3 | CHAINEDCONFIG | 1 = next PIC is chained physically to this one |
| 2..0 | MEMMASK | encoded size: 000 = 8 MB, 001 = 64 KB, 010 = 128 KB, 011 = 256 KB, 100 = 512 KB, 101 = 1 MB, 110 = 2 MB, 111 = 4 MB |

Note the weird memory-size encoding: 000 means 8 MB (the largest), 001..111 are 64 KB through 4 MB in increasing order. This is because the designers wanted "000 = reserved" for old-style boards; the 8 MB case is a late addition.

`er_Flags` bit 7 = ERFF_MEMSPACE (wants to be placed in the 8 MB expansion memory space $200000..$9FFFFF). Bit 6 = ERFF_NOSHUTUP (board cannot be shut up — typically only bus backplanes).

### 16.1.3 Control writes

The ExpansionControl structure is at $40..$7F of the 64 KB window:

| Byte offset | Meaning |
|------------:|---------|
| $48, $4A | `ec_BaseAddress` — CPU writes here to assign a new base address. High byte at $48, low byte at $4A (nibble-encoded). |
| $4C | `ec_Shutup` — any write here shuts the board up (it never responds again until reset). |
| $40, $42 | `ec_Interrupt` — control status register for interrupts (read/write). |

`ec_Interrupt` bits:

- Write bit 1 (INTENA) — enable interrupts.
- Write bit 3 (RESET) — local reset.
- Read bit 4 (INT2PEND), read bit 5 (INT6PEND), read bit 6 (INT7PEND) — which interrupt the board is pulling.
- Read bit 7 (INTERRUPTING) — "I am pulling INT".

Write base address: the CPU writes two nibbles to $48 (high) and $4A (low) corresponding to A23..A16 (byte offset 4 in the ExpansionControl view). This maps the board to a new 64 KB-aligned (or larger, depending on size) address, at which point the board stops responding at $E80000 and starts responding at its new address. It also automatically asserts CFGOUT, passing the chain to the next slot.

### 16.1.4 Alignment rules

- A PIC that requests N × 64 KB must be placed on an N × 64 KB boundary (binary).
- 4 MB PICs: must be placeable on 4 MB boundaries OR at $200000, OR at $600000 (the "holes" in the 8 MB expansion range on odd 2 MB boundaries).
- 8 MB PICs: must be placeable on 8 MB boundaries OR at $200000.
- 6 MB is not supported (split 8 MB boards into two PICs of 4+2 MB).

### 16.1.5 Shut-up protocol

Writing any value to offset $4C of the unconfigured window causes the PIC to:

1. Stop responding to $E80000.
2. Assert CFGOUT so the next PIC can be configured.
3. Never respond again until hardware reset.

Used when the OS can't use this PIC (e.g., no driver available) or for test sequences.

### 16.1.6 Address space

```
$200000..$9FFFFF   8 MB Zorro II expansion memory/IO (per TRM)
$A00000..$BEFFFF   Reserved
$BF0000..$BFFFFF   8520 CIA registers
$C00000..$D7FFFF   Pseudo-fast/slow RAM (A500 trapdoor 512K lives here, $C00000..$C7FFFF)
$D80000..$DBFFFF   Reserved
$DC0000..$DCFFFF   Battery clock
$DD0000..$DDFFFF   Reserved
$DE0000..$DEFFFF   Gary (A3000) / reserved
$DF0000..$DFFFFF   Custom chip registers
$E00000..$E7FFFF   Reserved
$E80000..$E8FFFF   AutoConfig window
$E90000..$EFFFFF   Reserved (sometimes slave IO)
$F00000..$F7FFFF   Diagnostic / cartridge ROM
$F80000..$FFFFFF   Kickstart ROM
```

## 16.2 expansion.library

The ROM library that runs the AutoConfig protocol and maintains the list of configured boards.

Functions (Autodocs expansion.library):

- `AllocBoardMem(slotSpec)` — allocate expansion space for a board given the size field of `er_Type`. Knows about binary-boundary alignment rules.
- `FreeBoardMem(startSlot, slotSpec)` — inverse.
- `AllocExpansionMem(numSlots, slotOffset)` — low-level slot allocation with explicit alignment constraint (numSlots mod slotOffset == 0).
- `FreeExpansionMem(startSlot, numSlots)` — inverse.
- `AllocConfigDev()` — allocate a new `ConfigDev` struct.
- `FreeConfigDev(configDev)` — free it.
- `AddConfigDev(configDev)` — add to the system-wide list.
- `RemConfigDev(configDev)` — remove from the list.
- `FindConfigDev(oldConfigDev, manufacturer, product)` — search the list for a match. Pass NULL as `oldConfigDev` to start; pass the previous return to iterate. -1 for manufacturer or product is a wildcard.
- `ReadExpansionByte(board, offset)` — read one byte (two nibbles) from a board's config area.
- `WriteExpansionByte(board, offset, byte)` — write one byte.
- `ReadExpansionRom(board, configDev)` — read the entire `ExpansionRom` portion of the config area into `cd_Rom`. Knows about one's-complement encoding.
- `ConfigBoard(board, configDev)` — allocate expansion memory for the board and write the base address to its ExpansionControl area. Updates `configDev->cd_BoardAddr` etc. Called after `ReadExpansionRom`.
- `ConfigChain(baseAddr)` — the top-level "configure everything" call. Walks the entire chain at baseAddr, calling all the other functions as needed, and adds each configured board's `ConfigDev` to the system list.
- `MakeDosNode(parmPkt)` — build a DOS `DeviceNode` + `FileSysStartupMsg` + environment vector from a parameter packet. Used for mounting block devices.
- `AddDosNode(bootPri, flags, deviceNode)` — add a `DeviceNode` to AmigaDOS. If DOS is already running, the device goes in immediately; if not, it's queued for DOS to pick up at boot.
- `AddBootNode(bootPri, flags, deviceNode, configDev)` — like `AddDosNode` but also wires up the device for autoboot (ROM boot).
- `GetCurrentBinding(currentBinding, size)` / `SetCurrentBinding(currentBinding, size)` — pass extra arguments to a newly-bound driver (kludge to let `BindDriver` communicate).
- `ObtainConfigBinding()` / `ReleaseConfigBinding()` — serialize access to the `CurrentBinding` state (via a SignalSemaphore).

## 16.3 DiagArea

If `er_Type` has `DIAGVALID` (bit 4) set, `er_InitDiagVec` is a word offset from the board base pointing to a `DiagArea` structure:

```c
struct DiagArea {
    UBYTE da_Config;        /* see below */
    UBYTE da_Flags;
    UWORD da_Size;          /* total size in bytes */
    UWORD da_DiagPoint;     /* offset to diagnostic code, 0 if none */
    UWORD da_BootPoint;     /* offset to boot code */
    UWORD da_Name;          /* offset to null-terminated board name string */
    UWORD da_Reserved01;
    UWORD da_Reserved02;
};
```

`da_Config` bits:

- bits 7..6 (DAC_BUSWIDTH): 00 = 16-bit wide access (DAC_WORDWIDE), 01 = byte-wide, 10 = nibble-wide.
- bit 7 (DAC_NIBBLEWIDE, DAC_BYTEWIDE): how the diag area is physically encoded in the ROM.
- bits 5..4 (DAC_BOOTTIME): 00 = never boot, 01 = call `da_BootPoint` at config-time (as the board is being configured), 10 = call at bind-time (when drivers are bound to boards later).

The DiagArea is **copied from the board's ROM to fast RAM** by `expansion.library`, then "de-nibbleized" if the original was nibble-wide (combine pairs of nibbles into bytes). After copy, `da_DiagPoint` (if nonzero) is called first as a self-test routine. Then `da_BootPoint` is called.

Call environment (Autodocs):

- A7 = at least 2K of stack
- A6 = ExecBase
- A5 = ExpansionBase
- A3 = this board's ConfigDev
- A2 = base of (copied) diag/init area
- A0 = base of the board itself (in expansion space)

The routine returns a value in D0. If NULL, `expansion.library` returns the copied diag area to the free memory pool (one-shot code). If nonzero, the diag area stays resident — this is how a driver "installs itself" from a ROM. A typical hard-disk driver returns a pointer to its `Device` structure, which Exec then integrates into the library list.

**This is how autoboot works**. A hard-disk controller card's ROM contains a DiagArea. At boot, `ConfigChain` configures the card and then calls its DiagPoint. The DiagPoint code runs a self-test, sets up a `Device` struct, returns it. Exec sees a new device in its list. `ConfigBoard` then (with BootNode info) uses `AddBootNode` to tell expansion.library "here's a bootable disk." At the DOS bootstrap stage, expansion.library's boot nodes get tried in priority order for boot.

## 16.4 BootNode

Wraps information needed for autoboot. A `BootNode` is added to expansion.library's boot list:

```c
struct BootNode {
    struct Node     bn_Node;
    UWORD           bn_Flags;
    struct DeviceNode *bn_DeviceNode;
};
```

Priority in `bn_Node.ln_Pri` controls boot order. Floppy drive is +5, hard disks are 0, network disks −5. The bootstrap tries the highest-priority node first, falls through on failure.

## 16.5 `cd_Flags`

```c
#define CDB_SHUTUP    0    /* this board has been shut up */
#define CDB_CONFIGME  1    /* this board needs a driver to claim it */
#define CDF_SHUTUP    0x01
#define CDF_CONFIGME  0x02
```

CDB_CONFIGME is set after AutoConfig runs but before a driver has claimed the board. BindDrivers or equivalent walks the ConfigDev list, matches drivers to Manufacturer+Product codes, and clears CONFIGME when a driver successfully binds. You can see this in the classic AmigaDOS sequence where expansion hardware appears but drivers bind later.

## 16.6 AddDosNode vs AddBootNode vs MakeDosNode

- `MakeDosNode` — constructs the raw DOS data structures (DeviceNode, FileSysStartupMsg, environment vector) from a parameter packet. Does not enter them into DOS yet.
- `AddDosNode` — enters the DeviceNode into the DOS device list. DOS sees the new volume. Cannot autoboot from this — just a manually-mounted disk.
- `AddBootNode` — wraps `AddDosNode` plus adds the device to the expansion.library boot list. Used for disks that can be booted.

A driver in a DiagArea that wants its disk to be bootable:

1. Call `MakeDosNode` to build the DeviceNode.
2. Call `AddBootNode(bootPri, flags, deviceNode, configDev)`.

ROM bootstrap then sees the BootNode in the list.

---

# 17. Zorro II electrical / bus

(TRM §Expansion Bus)

## 17.1 100-pin connector

The A2000 Zorro II slot is a 100-pin edge connector. Five slots on A2000, plus a 86-pin coprocessor slot (for 68020/68030 accelerators and the Video slot equivalent). The A1000 and A500 have an 86-pin edge connector on the side (different physical form but similar signal set — the A500 "side slot").

### 17.1.1 Pin groups

**Power**:
- Pin 1..4: Ground.
- Pin 5, 6: +5V (up to 2 A per slot, 4 A on one slot for big RAM cards; 20 A max for the whole A2000 PSU).
- Pin 8: -5V (300 mA total budget).
- Pin 10: +12V (8 A total budget, most used by drives).
- Pin 20: -12V (300 mA total budget).

**Clocks**:
- Pin 14: /C3 — 3.58 MHz clock, rising-edge-synched to 7.16 MHz system clock (a.k.a. /CCKQ).
- Pin 15: CDAC — 7.16 MHz clock leading the system clock by 70 ns.
- Pin 16: /C1 — 3.58 MHz clock, falling-edge-synched (a.k.a. /CCK).
- Pin 50: E — 68000 E clock, 715 kHz (six 7M clocks high, four low).
- Pin 92: 7M — 7.16 MHz system clock.

**Address/control**:
- A1..A23 (23-bit address bus).
- /AS, /UDS, /LDS — strobes.
- /READ — R/W line.
- /VPA, /VMA — 6800-family handshake (used for CIAs and some 6502-style peripherals).
- /DTACK — data transfer acknowledge.
- FC0..FC2 — processor function codes.
- /BERR — bus error.
- /RST, /BUSRST — reset lines.
- /HLT — 68000 halt.

**Data**:
- D0..D15 (16-bit data bus).

**Interrupts**:
- Pin 19: /INT2 — interrupt level 2 input (shared with Paula).
- Pin 22: /INT6 — interrupt level 6 input (shared with Paula).
- Pins 40, 42, 44: on original Zorro were /IPL0, /IPL1, /IPL2 (multiplexed encoded interrupts). On A2000/B2000 these are decoded /EINT7 (pin 40), /EINT5 (pin 42), /EINT4 (pin 44).
- Pin 96: /EINT1 (A2000/B2000 addition).

So Zorro II expansion cards can source interrupt levels 2, 6, and in A2000/B2000 also 1, 4, 5, 7.

**Slot control**:
- Pin 7: /OWN — a PIC is bus-master (wired-OR, open-collector).
- Pin 9: /SLAVEn — this slot is responding as slave to the current cycle.
- Pin 11: /CFGOUTn — configuration chain output from this slot.
- Pin 12: /CFGINn — configuration chain input.

**Bus arbitration**:
- Pin 60: /BRn (slot-specific bus request).
- Pin 62: /BGACK (bus grant acknowledge, unbuffered 68000 BGACK).
- Pin 64: /BGn (slot-specific bus grant).
- Pin 95: /GBG (generic bus grant, for coprocessor slot).
- /BG arbiter logic gives priority: slot 1 highest, slot 5 lowest. Coprocessor slot on B2000 is priority 0 (highest), just above slot 1.

**Slot configuration**:
- /XRDY (pin 18) — external ready; pulled low to insert wait states into /DTACK generation.
- /OVR (pin 17) — override; tri-states internal /DTACK so a PIC can drive it. Also disables internal memory range decoding if asserted during $200000..$9FFFFF.
- DOE (pin 93) — data output enable.

## 17.2 /DTACK generation and wait states

Normal Amiga memory cycles are 4 clocks (280 ns at 7 MHz). A slow peripheral that needs more time pulls /XRDY low (within 60 ns of /AS going valid) to inhibit the motherboard's /DTACK generation. When ready, it releases /XRDY and the cycle completes. /XRDY is open-collector, so multiple devices can wired-OR it.

Alternative: /OVR tri-states /DTACK entirely, letting the PIC drive it itself — more work but more flexible.

Any access that can't be satisfied by a local device (i.e., no /SLAVEn asserted) within a timeout ends with /BERR (bus error), which the 68000 takes as a bus-error exception.

## 17.3 DMA (bus mastery)

A PIC can become bus master:

1. Assert /BRn (slot-specific bus request).
2. Wait for /BGn (bus grant).
3. Assert /BGACK to take control.
4. Drive /AS, R/W, data bus, etc. to do DMA cycles.
5. Deassert /BGACK to release.

Arbitration prioritizes slot 1 highest. A DMA-mastering card (e.g. SCSI with DMA) requests the bus, does its transfers, and releases. The 68000 is halted during this time.

**Collision detection**: multiple slaves must not respond to the same address. Each slot has its own /SLAVEn output, all OR-combined into a collision detect circuit which asserts /BERR if more than one slot (or one slot + processor memory) is responding.

## 17.4 A500 side slot vs A2000 Zorro II

The A500 has:

- **Side slot** (86-pin edge connector on the left side). Similar signal set to Zorro II but in an 86-pin form. Used for hard disk controllers (GVP A530, etc.), RAM expansions, accelerators. AutoConfig works here.
- **Trapdoor slot** (underneath, 50-pin). Not AutoConfig; used for the classic 512 KB "Slow RAM" upgrade and the clock card. Maps at $C00000..$C7FFFF.

The A1000 has:

- 86-pin edge connector (same as A500 side slot electrically). External Zorro I expansion box (A1000 "Zorro box") plugs in.

The A2000 has:

- Five 100-pin Zorro II slots.
- One 86-pin coprocessor slot (the "Video slot" equivalent is actually an additional 23-pin slot, not the 86-pin slot).
- Two ISA slots (for the Bridge Board).

The B2000/A2000B (later revisions) has Zorro II implemented via a gate array, identical in behavior to the PAL-based A2000.

## 17.5 Trapdoor slot (A500)

Address range $C00000..$C7FFFF is used by:

- The 512 KB "CBM A501" trapdoor RAM upgrade.
- Some third-party 512 KB/1 MB/2 MB upgrades.
- The classic CBM battery-backed clock card ($DC0000..$DCFFFF).

The trapdoor card is **not** AutoConfig — it maps at a fixed address and is enabled by a jumper or by Agnus configuration. The 512 KB "Slow RAM" is in fact seen by the custom chips as chip memory on Fat Agnus+ systems (the "ECS trick"); on original Agnus it's in an unused area and has some quirks (see Mapping the Amiga and `amiga-boot-process.md`).

---

# 18. Chip revisions

(HRM Appendix C, Mapping the Amiga chip appendix, corpus-general notes)

## 18.1 Agnus

- **8370 Agnus** (OCS): original. 512 KB chip RAM addressing. NTSC.
- **8371 Agnus** (OCS): PAL version.
- **8372A "Fatter Agnus"** (ECS): 1 MB chip RAM. NTSC or PAL (jumper-selectable).
- **8372B "Super Agnus"** (ECS): 2 MB chip RAM. A500+, A600.
- **8375 Agnus** (AGA): 2 MB chip RAM, AGA register extensions. A4000.

For audio, the relevant difference is `AUDxLCH` width: 3 bits (OCS) vs 5 bits (ECS/AGA). HRM notes AUDxLCH "(E)" for ECS extension.

## 18.2 Paula

- **8364 Paula** (OCS): original. Two known die revisions; no externally visible behavioral differences relevant to audio/UART/disk.
- **8364 R7** (ECS): same part number, slight analog refinements. Some "bugs" in very early Paula revisions (DMA timing glitches) were cleaned up.

Paula never got a major revision. Audio behavior is the same across all Amigas 1000/500/2000/3000/1200/4000 — only the Agnus-side pointer width changed. Filter bypass bit appeared on later A500 and two-layer A2000 motherboards as noted in §1.7.

## 18.3 Denise / Lisa

- **8362 Denise** (OCS): original.
- **8373 Denise** (ECS): ECS Denise adds `BEAMCON0`, superhires, productivity mode.
- **Lisa** (AGA): in AGA machines, replaces Denise. Huge change for graphics, irrelevant here.

## 18.4 CIAs

- **MOS 8520** (OCS CIA): original. Two instances (CIA-A, CIA-B).
- **CSG 8520A / 8520-R4** ("Fat CIA"): later revision, sometimes called "Fat CIA" in sales literature but the die is the same. No externally visible behavioral differences.

CIA timer A in output mode is used by some SPI-style peripherals (at 1/2 E-clock = ~358 kHz max shift rate). CIA serial shift register for keyboard is documented in §7.

## 18.5 Gary, Buster, Ramsey, Fat Agnus friends

Support chips on the A2000 / A3000 / A4000 motherboards:

- **Gary** (5719): address decoding, DMA routing, auto-config signal handling on A500 and A2000. Replaces the PAL logic that earlier A2000s used for bus control.
- **Buster** (5721): Zorro II bus controller / arbiter on A2000 (replaces separate PALs in the original A2000). On A3000 "Buster II" / "Super Buster" adds Zorro III support.
- **Ramsey**: memory controller on A3000/A4000 (handles fast RAM, DMA to/from it).
- **Fat Agnus** / **Fatter Agnus** / **Super Agnus**: Agnus revisions (see §18.1).

For an emulator, Gary and Buster are mostly invisible (they affect timing and DMA arbitration, not software state). An accuracy-focused emulator targeting Zorro III hardware needs Buster II behavior. Zorro II emulators can ignore them as long as bus cycles complete in the right number of cycles.

## 18.6 A500 vs A2000 gate array differences

The A500 uses Gary. The A2000 (original) uses a bank of PALs for the same functions; A2000B uses Gary-equivalent gate arrays. Functionally identical from software's view. TRM §Expansion Bus notes some minor timing differences that matter for slow expansion cards.

---

# Appendix A — `ConfigDev`, `ExpansionRom`, `ExpansionControl`, `DiagArea`

From `libraries/configregs.h` and `libraries/configvars.h` (Autodocs, SPG, and the `hardware/` include files).

## A.1 `ExpansionRom` (PIC-readable config area)

```c
struct ExpansionRom {
    UBYTE er_Type;            /* 0x00 nibble offset */
    UBYTE er_Product;         /* 0x04 */
    UBYTE er_Flags;           /* 0x08 */
    UBYTE er_Reserved03;      /* 0x0C */
    UWORD er_Manufacturer;    /* 0x10 (2 bytes = 4 nibbles) */
    ULONG er_SerialNumber;    /* 0x18 (4 bytes = 8 nibbles) */
    UWORD er_InitDiagVec;     /* 0x28 */
    UBYTE er_Reserved0c;
    UBYTE er_Reserved0d;
    UBYTE er_Reserved0e;
    UBYTE er_Reserved0f;
};
```

**Note**: each logical byte occupies **two addresses, 4 bytes apart** in the PIC's physical space because each address holds one nibble in bits 15..12. `EROFFSET(field)` macro converts a field offset in the struct to a board-space offset:

```c
#define EROFFSET(er)  ((int)&((struct ExpansionRom *)0)->er)
```

Used with `ReadExpansionByte(board, EROFFSET(er_Type))`, etc.

## A.2 `ExpansionControl` (PIC-writable configuration area)

```c
struct ExpansionControl {
    UBYTE ec_Interrupt;   /* 0x40 interrupt control register */
    UBYTE ec_Reserved11;
    UBYTE ec_BaseAddress; /* 0x48 set new config address (high) */
    UBYTE ec_Shutup;      /* 0x4C shut up */
    UBYTE ec_Reserved14;
    UBYTE ec_Reserved15;
    UBYTE ec_Reserved16;
    UBYTE ec_Reserved17;
    UBYTE ec_Reserved18;
    UBYTE ec_Reserved19;
    UBYTE ec_Reserved1a;
    UBYTE ec_Reserved1b;
    UBYTE ec_Reserved1c;
    UBYTE ec_Reserved1d;
    UBYTE ec_Reserved1e;
    UBYTE ec_Reserved1f;
};
```

`ECOFFSET(ec_field)` = `sizeof(ExpansionRom) + (int)&((struct ExpansionControl *)0)->ec_field`.

Base address write sequence: write the high nibble of the target high byte to $48 (bit position D15..D12), then the low nibble to $4A, then high nibble of the low byte to (somewhere depending on chip width). Actual sequence is handled by `WriteExpansionByte` via a carefully-crafted double-write that works whether the board is byte-wide or nibble-wide:

```
write low nybble to bits D15..D12 of byte offset (offset*4) + 2
write entire byte to bits D15..D8 of byte offset (offset*4)
```

(Autodocs expansion.library/WriteExpansionByte.)

## A.3 `ConfigDev` (system-side record of a configured board)

```c
struct ConfigDev {
    struct Node        cd_Node;
    UBYTE              cd_Flags;      /* CDF_SHUTUP | CDF_CONFIGME */
    UBYTE              cd_Pad;
    struct ExpansionRom cd_Rom;       /* image of expansion ROM area */
    APTR               cd_BoardAddr;  /* where in memory the board is */
    APTR               cd_BoardSize;  /* size in bytes */
    UWORD              cd_SlotAddr;   /* which slot number */
    UWORD              cd_SlotSize;   /* number of slots the board takes */
    APTR               cd_Driver;     /* pointer to node of driver (if bound) */
    struct ConfigDev * cd_NextCD;     /* linked list for drivers that want it */
    ULONG              cd_Unused[4];
};
```

`cd_Flags`:

- `CDF_SHUTUP` (0x01) — board has been shut up, no longer responding.
- `CDF_CONFIGME` (0x02) — board needs a driver to claim it (set after AutoConfig, cleared after BindDriver).

`cd_SlotAddr` / `cd_SlotSize` are in 64 KB slot units. `cd_BoardAddr` / `cd_BoardSize` are the byte-equivalents. `cd_Driver` points to the `Node` of the library/device/resource that bound this card, or NULL.

## A.4 `CurrentBinding`

```c
struct CurrentBinding {
    struct ConfigDev *cb_ConfigDev;  /* first configdev in chain */
    UBYTE            *cb_FileName;    /* driver filename (DEVS:...) */
    UBYTE            *cb_ProductString; /* product code */
    UBYTE            **cb_ToolTypes;  /* tooltypes from disk object */
};
```

Used by `BindDriver` to pass extra arguments to newly-loaded drivers without changing the (long since fixed) initialization calling sequence.

## A.5 `DiagArea`

```c
struct DiagArea {
    UBYTE da_Config;       /* see below */
    UBYTE da_Flags;
    UWORD da_Size;         /* total diag area size in bytes */
    UWORD da_DiagPoint;    /* offset to diagnostic code or 0 */
    UWORD da_BootPoint;    /* offset to boot code */
    UWORD da_Name;         /* offset to null-terminated board name */
    UWORD da_Reserved01;
    UWORD da_Reserved02;
};
```

`da_Config` layout:

- Bits 7..6 (DAC_BUSWIDTH): 00 = WORDWIDE, 01 = BYTEWIDE, 10 = NIBBLEWIDE.
  - `DAC_NIBBLEWIDE` 0x00 — read the diag area as nibbles.
  - `DAC_BYTEWIDE`   0x40 — read as bytes.
  - `DAC_WORDWIDE`   0x80 — read as words. (No, wait: actual constants are different; cross-check include file.)
- Bits 5..4 (DAC_BOOTTIME):
  - `DAC_NEVER` 0x00 — do not call boot point.
  - `DAC_CONFIGTIME` 0x10 — call `da_BootPoint` at config-time (when the board is first being configured).
  - `DAC_BINDTIME` 0x20 — call `da_BootPoint` at bind-time.

## A.6 `BootNode`

```c
struct BootNode {
    struct Node       bn_Node;       /* priority in bn_Node.ln_Pri */
    UWORD             bn_Flags;
    struct DeviceNode *bn_DeviceNode;
};
```

## A.7 `EClockVal`

```c
struct EClockVal {
    ULONG ev_hi;
    ULONG ev_lo;
};
```

Used by `ReadEClock()` in `timer.device` (V36+).

## A.8 `timerequest`

```c
struct timerequest {
    struct IORequest tr_node;
    struct timeval   tr_time;
};

struct timeval {
    ULONG tv_secs;
    ULONG tv_micro;
};
```

## A.9 `IOExtSer` (serial.device)

```c
struct IOExtSer {
    struct IOStdReq  IOSer;           /* sizeof(IOStdReq) */
    ULONG   io_CtlChar;               /* XON/XOFF/INQ/ACK packed */
    ULONG   io_RBufLen;               /* input buffer size */
    ULONG   io_ExtFlags;              /* reserved */
    ULONG   io_Baud;                  /* 110..292000 */
    ULONG   io_BrkTime;               /* break duration microseconds */
    struct IOTArray io_TermArray;     /* 8 EOF bytes */
    UBYTE   io_ReadLen;               /* 7 or 8 */
    UBYTE   io_WriteLen;              /* 7 or 8 */
    UBYTE   io_StopBits;              /* 1 or 2 */
    UBYTE   io_SerFlags;              /* SERB_* */
    UWORD   io_Status;
};

struct IOTArray {
    ULONG TermArray0;
    ULONG TermArray1;
};
```

## A.10 `IOExtPar` (parallel.device)

```c
struct IOExtPar {
    struct IOStdReq IOPar;
    ULONG  io_PExtFlags;              /* reserved */
    UWORD  io_Status;
    UBYTE  io_ParFlags;               /* PARB_* */
    struct IOTArray io_PTermArray;
};
```

## A.11 `IOAudio` (audio.device)

```c
struct IOAudio {
    struct IORequest ioa_Request;
    struct Message  *ioa_WriteMsg;
    UBYTE           *ioa_Data;
    ULONG            ioa_Length;
    UWORD            ioa_Period;
    UWORD            ioa_Volume;
    UWORD            ioa_Cycles;
    UBYTE            ioa_AllocKey;
};
```

## A.12 `InputEvent` (input.device, keyboard.device, gameport.device)

```c
struct InputEvent {
    struct InputEvent *ie_NextEvent;
    UBYTE ie_Class;
    UBYTE ie_SubClass;
    UWORD ie_Code;
    UWORD ie_Qualifier;
    union {
        struct {
            WORD ie_x;
            WORD ie_y;
        } ie_xy;
        APTR ie_addr;
    } ie_position;
    struct timeval ie_TimeStamp;
};

#define ie_X ie_position.ie_xy.ie_x
#define ie_Y ie_position.ie_xy.ie_y
```

## A.13 `Interrupt` (handler chain element)

```c
struct Interrupt {
    struct Node is_Node;
    APTR       is_Data;
    VOID     (*is_Code)();
};
```

---

# Appendix B — Resource / device summary table

| Name | Kind | Opens via | Purpose | Arbitration model |
|------|------|-----------|---------|-------------------|
| `audio.device` | Device | OpenDevice | DMA audio playback, channel arbitration | Precedence-based alloc of channel subsets via allocation mask |
| `serial.device` | Device | OpenDevice | Paula UART + modem control | Shared or exclusive; claims `misc.resource` MR_SERIALPORT / MR_SERIALBITS |
| `parallel.device` | Device | OpenDevice | CIA-A PRB parallel I/O | Shared or exclusive; claims `misc.resource` MR_PARALLELPORT / MR_PARALLELBITS |
| `input.device` | Device | OpenDevice | Merged event stream (keyboard + gameport + timer + DOS inject) | Shared; handler chain via IND_ADDHANDLER |
| `keyboard.device` | Device | OpenDevice | CIA-A SDR-based keyboard protocol, reset handlers, key matrix | Low-level access; reset handler chain |
| `gameport.device` | Device | OpenDevice (2 units) | Mouse/joystick events, controller type configuration | Exclusive per unit; OpenDevice fails if held |
| `timer.device` | Device | OpenDevice (4 units: VBLANK, MICROHZ, ECLOCK, WAITUNTILECLOCK) | Timing, waiting, system time | Shared; multiple open/request queues OK |
| `trackdisk.device` | Device | OpenDevice (4 units) | Floppy disk (MFM) I/O | see `amiga-dos-filesystem-disk.md` |
| `printer.device` | Device | OpenDevice | Printer driver indirection over serial/parallel | Uses serial.device or parallel.device internally |
| `console.device` | Device | OpenDevice (per window) | ANSI-ish terminal with keymap translation | Per-window |
| `cia.resource` | Resource | OpenResource("ciaa.resource" / "ciab.resource") | Arbitrates CIA ICR interrupts | AddICRVector / RemICRVector |
| `potgo.resource` | Resource | OpenResource | Arbitrates POTGO 4-bit GPIO | AllocPotBits / FreePotBits |
| `misc.resource` | Resource | OpenResource | Arbitrates serial/parallel port ownership | AllocMiscResource / FreeMiscResource, assembly only |
| `disk.resource` | Resource | OpenResource | Arbitrates CIA-B disk-select/motor/step bits | See dos/disk doc |
| `keyboard.resource` | Resource | OpenResource | Low-level keyboard CIA access | Wrapped by keyboard.device |
| `battclock.resource` | Resource | OpenResource | RTC read/write | ReadBattClock / WriteBattClock |
| `battmem.resource` | Resource | OpenResource | Battery-backed RAM | ReadBattMem / WriteBattMem |
| `card.resource` | Resource | OpenResource (V37+) | PCMCIA slot management | OwnCard / ReleaseCard |
| `expansion.library` | Library | OpenLibrary | AutoConfig management | AddConfigDev / FindConfigDev / ConfigChain / AddBootNode |
| `graphics.library` | Library | OpenLibrary | Display primitives | see graphics doc |
| `exec.library` | Library | (pre-linked) | Task/memory/message model | see exec doc |
| `dos.library` | Library | OpenLibrary | AmigaDOS | see dos doc |
| `intuition.library` | Library | OpenLibrary | GUI / window manager | see graphics doc |
| `keymap.library` | Library (V36+) | OpenLibrary | Keyboard layout translation | shared |
| `commodities.library` | Library (V36+) | OpenLibrary | Higher-level input event matching | shared |

---

# Appendix C — ADKCON and INTENA audio-relevant bit tables

## C.1 `ADKCON` / `ADKCONR` ($09E / $010)

(HRM Appendix A)

| Bit | Name | Function |
|----:|------|----------|
| 15 | SET/CLR | Set/clear control for the write to this register. |
| 14..13 | PRECOMP1-0 | MFM precompensation time (disk). 00 = none, 01 = 140 ns, 10 = 280 ns, 11 = 560 ns. |
| 12 | MFMPREC | 1 = MFM precomp, 0 = GCR precomp. |
| 11 | UARTBRK | 1 = force TXD pin to 0 (serial break). |
| 10 | WORDSYNC | 1 = enable disk read sync on DSKSYNC word. |
| 9 | MSBSYNC | 1 = enable disk sync on MSB (Apple GCR). |
| 8 | FAST | Disk data clock rate: 1 = fast 2µs, 0 = slow 4µs. |
| 7 | USE3PN | Use audio ch 3 to modulate nothing (ATPER3). |
| 6 | USE2P3 | Use audio ch 2 to modulate period of ch 3 (ATPER2). |
| 5 | USE1P2 | Use audio ch 1 to modulate period of ch 2 (ATPER1). |
| 4 | USE0P1 | Use audio ch 0 to modulate period of ch 1 (ATPER0). |
| 3 | USE3VN | Use audio ch 3 to modulate nothing (ATVOL3). |
| 2 | USE2V3 | Use audio ch 2 to modulate volume of ch 3 (ATVOL2). |
| 1 | USE1V2 | Use audio ch 1 to modulate volume of ch 2 (ATVOL1). |
| 0 | USE0V1 | Use audio ch 0 to modulate volume of ch 1 (ATVOL0). |

Setting any USExPy or USExVy disables that modulator channel's audio output. See §1.6 for the modulation data format.

## C.2 `INTENA` / `INTREQ` / `INTENAR` / `INTREQR` ($09A / $09C / $01C / $01E) — audio bits

(HRM figure 7-4)

| Bit | Name | Source | Level |
|----:|------|--------|------:|
| 15 | SET/CLR | write control | — |
| 14 | INTEN | Master enable | — |
| 13 | EXTER | CIA-B / external | 6 |
| 12 | DSKSYN | Disk sync detected | 5 |
| 11 | RBF | Receive buffer full (serial) | 5 |
| 10 | AUD3 | Audio channel 3 | 4 |
| 9 | AUD2 | Audio channel 2 | 4 |
| 8 | AUD1 | Audio channel 1 | 4 |
| 7 | AUD0 | Audio channel 0 | 4 |
| 6 | BLIT | Blitter finished | 3 |
| 5 | VERTB | Vertical blank | 3 |
| 4 | COPER | Copper | 3 |
| 3 | PORTS | CIA-A / external INT2 | 2 |
| 2 | SOFT | Software | 1 |
| 1 | DSKBLK | Disk block finished | 1 |
| 0 | TBE | Transmit buffer empty (serial) | 1 |

(Note AUD3 is bit 10 despite being channel 3 — not bit 11. The AUD bits are 7,8,9,10 for AUD0,1,2,3 respectively. HRM is explicit about this.)

## C.3 `DMACON` / `DMACONR` ($096 / $002) — audio bits

| Bit | Name | Function |
|----:|------|----------|
| 15 | SET/CLR | Set/clear |
| 14 | BBUSY | Blitter busy (read only) |
| 13 | BZERO | Blitter output all zero (read only) |
| 10 | BLTPRI | Blitter priority ("nasty") |
| 9 | DMAEN | Master DMA enable |
| 8 | BPLEN | Bitplane DMA |
| 7 | COPEN | Copper DMA |
| 6 | BLTEN | Blitter DMA |
| 5 | SPREN | Sprite DMA |
| 4 | DSKEN | Disk DMA |
| 3 | AUD3EN | Audio ch 3 DMA |
| 2 | AUD2EN | Audio ch 2 DMA |
| 1 | AUD1EN | Audio ch 1 DMA |
| 0 | AUD0EN | Audio ch 0 DMA |

Both `DMAEN` and the per-channel `AUDxEN` must be set for a channel to DMA.

---

# Appendix D — Gaps in corpus

Things the corpus does not fully document, and where to go next:

1. **`card.resource` (PCMCIA) detailed function reference**. The corpus mentions PCMCIA only in passing (A600/A1200 context) — `card.resource` autodocs are in the later Kickstart 2.05+/3.0 Autodocs, which are not in this corpus. Cross-reference the "Amiga Developer CD" or the `Mapping the Amiga 2nd ed` appendix (partial). An emulator targeting A600/A1200 PCMCIA should track down a 3.0 Autodocs PDF.

2. **Exact MSM6242B register addresses as mapped on the A2000 / A3000 / A500+ motherboards**. HRM and TRM both gloss this. Software talks to `battclock.resource`, not directly to the chip, so corpus focuses on the API, not the hardware mapping. For emulator accuracy at the hardware level, consult the service manual for each specific machine — the register spacing (every 4 bytes, low nibble of byte) is standard but the base offset within $DC0000 varies by board.

3. **"Buster"** chip behavior for Zorro III bus cycles is not in the corpus. Zorro III (A3000/A4000) is mentioned only tangentially; the full bus protocol with its multiplexed address/data and cache support requires the A3000 service manual, not available here.

4. **Detailed Paula analog filter response** is given as a curve in HRM figure 5-5 but no explicit transfer function. Emulators use a one-pole RC filter with ~4.5 kHz cutoff and it sounds right; exact accuracy would require SPICE models from the original Paula design, which are not public.

5. **cia.resource function prototypes** are documented in the includes (`resources/cia.h`) but the Autodocs pages for cia.resource in this corpus are truncated (they appear in the index but only brief). Cross-reference the 3.0 Autodocs.

6. **`printer.device`** — the corpus contains printer.device documentation but this document does not cover it (the task explicitly excludes it). For printer emulation consult RKM L&D chapter 15.

7. **Keyboard-MPU reset timing** for A1000 vs later keyboards: HRM Appendix G says "this feature is available on some A1000 and A2000 keyboards" for reset warning, but does not enumerate which ROM revisions of the keyboard MPU have it. For an exhaustive emulator, check the hardware service manuals for A1000, A2000 rev 4.x-6.x, A500, etc.

8. **POT0/POT1 integration constant**: HRM gives 470 kΩ / 0.047 µF as recommended and 528 kΩ max, but doesn't give the exact voltage threshold at which the counter stops. For emulation, a linear mapping 0..255 over the 0..max-R range is "close enough" — fine-grained curves for light-pen and paddle games would need to be measured.

9. **Keyboard matrix wiring**: HRM Appendix G shows the matrix but the column/row conventions and exact MPU pins are not given. Emulators rendering the matrix (for KBD_READMATRIX) need to map each logical keycode to a matrix (row, col) coordinate — the table is in HRM Appendix G figure "Matrix Table" but is formatted as ASCII art and somewhat garbled in the OCR.

10. **Audio state machine transition timing in attach mode**: HRM §Audio "Audio State Machine" describes the states but the exact clock count for each state transition is glossed over ("14 clock cycles later"). Exact emulation at the state level would need the Paula HDL, not documented in this corpus.

11. **Zorro II bus cycle timing** in nanoseconds is given in the TRM §Expansion Bus but varies with CPU speed and is largely informational for PIC designers, not emulator-critical.

12. **`commodities.library`** (V36+) — not in this corpus. Its docs are in V37+ Autodocs. If an emulator targets Workbench 2.0+, obtain those separately.

---

# Appendix E — Source map

Where each subsystem's authoritative documentation lives in the corpus:

| Subsystem | Primary | Secondary |
|-----------|---------|-----------|
| Paula audio hardware | HRM §Audio (ch.5) | HRM Appendix A (register summary); Mapping the Amiga ch.7 |
| `audio.device` | RKM L&D ch.5 | Autodocs audio.device |
| Paula UART hardware | HRM §Interface Hardware (§Serial I/O) | HRM Appendix A (SERDATR/SERDAT/SERPER) |
| `serial.device` | RKM L&D ch.13 | Autodocs serial.device |
| Parallel port hardware | HRM §Interface Hardware (§Parallel) + Appendix F (CIAs) | HRM Appendix E (pinout) |
| `parallel.device` | RKM L&D ch.14 | Autodocs parallel.device |
| Keyboard protocol | HRM Appendix G | HRM Appendix F (CIA SDR) |
| `keyboard.device` | RKM L&D ch.10 | Autodocs keyboard.device |
| Mouse / gameport hardware | HRM §Interface Hardware (§Controller Port) | HRM Appendix E (pinout) |
| `gameport.device` | RKM L&D ch.11 | Autodocs gameport.device |
| `input.device` | RKM L&D ch.9 | Autodocs input.device |
| `timer.device` | RKM L&D ch.6 | Autodocs timer.device |
| CIA timers / TOD / SDR | HRM Appendix F | TRM (A500/A2000 hardware), SPG |
| `cia.resource` | Autodocs cia.resource (partial in this corpus) | HRM Appendix F for hardware |
| `potgo.resource` | Autodocs potgo.resource | HRM §Interface, POTGO description |
| `misc.resource` | Autodocs misc.resource | (minimal in corpus) |
| `battclock.resource` / `battmem.resource` | Autodocs battclock | (MSM6242B pinout not in corpus) |
| `expansion.library` | Autodocs expansion.library | RKM L&D (expansion chapter); SPG |
| AutoConfig protocol | TRM §Auto Configuration ("Address Specification Table") | libraries/configregs.h and libraries/configvars.h (Autodocs include dumps) |
| Zorro II electrical | TRM §Expansion Bus ("100 Pin Connector Pinouts") | HRM §Interface Hardware (passing mentions) |
| Chip revisions | HRM Appendix C (ECS) | Mapping the Amiga chip appendix; SPG; TRM notes |

Citations inline throughout the document use:

- `HRM` = *Amiga Hardware Reference Manual, 3rd ed.* (1991 Commodore-Amiga / Addison-Wesley) — in corpus as `Amiga_Hardware_Reference_Manual_3rd_edition.txt`.
- `TRM` = *Commodore Amiga A500/A2000 Technical Reference Manual* (1987) — `Commodore_Amiga_A500_A2000_Technical_Reference_Manual_1987_Commodore_text.txt`.
- `RKM L&D` = *Amiga ROM Kernel Reference Manual: Libraries and Devices* — `Amiga_ROM_Kernel_Reference_Manual_Libraries_and_Devices.txt`.
- `Autodocs` = *Amiga ROM Kernel Reference Manual: Includes and Autodocs* — `Amiga_ROM_Kernal_Reference_Manual_Includes_and_Autodocs.txt`.
- `SPG` = *Amiga System Programmer's Guide* (Abacus, 1988) — `Amiga_System_Programmers_Guide_1988_Abacus.txt`.
- `Mapping` = *Mapping the Amiga, 2nd ed.* (1993 Compute! Books, Thomson/Anderson) — `1993-thomson-randy-rhett-anderson-mapping-amiga-2nd-edition.txt`.
- `Exec RKM` = *Amiga ROM Kernel Reference Manual: Exec* — `Amiga_ROM_Kernel_Reference_Manual_Exec.txt`.

*End of document.*
