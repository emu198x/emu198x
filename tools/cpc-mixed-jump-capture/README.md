# CPC mixed JP/NOP bus capture

This stimulus is for a real CPC464, CPC664 or CPC6128 logic capture. It
addresses the mixed-stream part of [#971][issue], not the separate IM 1
acknowledge discrepancy. It does not change the emulator's timing model.

## Run

Start from the normal Locomotive BASIC prompt. Enter `jp-nop.bas` and
`RUN` it. The loader reserves RAM above `&7FFF` and copies the
22-byte routine to `&8000`. It runs 256 passes and prints
`PROBE COMPLETE` on return. The routine masks interrupts during capture
and does not change display registers or access peripherals. It expects
interrupts enabled on entry, as in the normal BASIC environment.

The assembly and BASIC loader describe the same bytes. To rebuild:

```sh
asm198x --dialect pasmo jp-nop.asm -o jp-nop.bin
```

## Capture

Use 5 V-compatible inputs and a common ground. Capture the CPU's 4 MHz
clock, `/WAIT`, `/M1`, `/MREQ` and `/RD`. Also capture at least A0–A4:
that is enough to label the addresses **inside this loop**. If available,
record the full address bus and the Gate Array's 1 MHz character clock to
verify address and phase independently. At 64 MS/s there are about 16
samples per Z80 T-state, so keep the raw timestamps as well as the
rounded T-state counts.

Start the capture before `RUN` and trigger on a read fetch of `&8009`
(or its low five bits and the repeating address pattern below). Use the
falling edge of `/M1` for each fetch boundary and check that the same
pulse has a memory read, not interrupt acknowledge. Also retain the time
when both `/MREQ` and `/RD` are low in that pulse: the emulator column
below uses that qualified memory-read edge. The non-M1 reads of
`&800A` and `&800B` are the jump's operands; record their edge placement
against the clock and `/WAIT`. Do not count a held-low `/M1` as a new
fetch. The skipped address `&800C` must not appear as an opcode fetch.
If the analyser has too few channels for the whole address bus, check
the repeating low-address sequence before using the trace.

| From fetch | To fetch | Emu198x at `261c0a73` | If each instruction took a whole microsecond |
| --- | --- | ---: | ---: |
| `&8008` (NOP) | `&8009` (JP) | 4 T | 4 T |
| `&8009` (JP) | `&800D` (NOP) | 11 T | 12 T |
| `&800D` (NOP) | `&800E` (NOP) | 5 T | 4 T |
| `&800E` (NOP) | `&800F` (NOP) | 4 T | 4 T |

The two middle intervals total 16 T either way. Measuring only their
sum or taking an average over the loop therefore cannot distinguish the
models. Report several consecutive passes, not just a single duration,
and include the CPC board revision, CPU and Gate Array markings, CRTC
type if known, sample rate, channel mapping and the original capture.
A different pattern is also useful; do not force the trace into either
column.

## Evidence boundary

The emulator column was observed at qualified opcode fetches with this
exact binary: all 256 passes on the CPC464 with its boot ROM, and 256
each on the CPC664 and CPC6128 with synthetic firmware. The other
column follows the whole-microsecond instruction costs in
[Longshot's CRTC Compendium, chapter 26][compendium], not a capture of
this mixed stimulus. Existing [CPC oscilloscope traces][scope] show
READY and bus timings for individual accesses, but do not label this
instruction sequence. No hardware result is asserted here.

[issue]: https://github.com/emu198x/emu198x/issues/971
[compendium]: https://shaker.logonsystem.eu/ACCC1.11-EN.pdf#page=282
[scope]: https://bread80.com/2021/06/03/understanding-the-amstrad-cpc-video-ram-and-gate-array-subsystem/
