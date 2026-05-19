# Paula 8364 — port-gap analysis (2026-04-20)

Phase 1 gap list for the archive → live-tree port. Follows the
`archive-port-methodology.md` three-phase discipline that retired the
CIA-8520 archive successfully (commits `61f5b49`, `644fead`, `c45acd5`).

## What Paula is

Paula (MOS 8364) owns **audio DMA + output**, **floppy disk DMA + MFM
match**, **serial I/O**, **analog inputs** (POTGO/POTxDAT), **interrupt
controller** (INTENA/INTREQ), and a shared peripheral-control register
(ADKCON). Pin-for-pin it's three chips bolted together onto one die.

## Current-tree coverage (`crates/machine-commodore-amiga-ocs/src/chipset.rs`)

| Area | Current-tree state |
| --- | --- |
| INTENA / INTREQ storage | ✅ set/clear semantics, 16-bit |
| INTENA master-enable gate (bit 14) | ✅ `compute_ipl` respects it |
| IPL priority encoder | ✅ HRM mapping, inline in `Chipset::compute_ipl` |
| ADKCON storage | ✅ set/clear semantics |
| DMACON storage | ✅ set/clear semantics |
| DSKPT / DSKLEN / DSKDAT / DSKSYNC storage | ✅ stored only — no behaviour |
| DSKLEN double-write arming | ❌ not implemented |
| DSKBYTR read register | ❌ not implemented |
| Disk PLL (variable rate / IPF) | ❌ not implemented |
| Disk DMA completion → DSKBLK IRQ | ❌ not wired |
| Audio channel registers (AUDxLC/LEN/PER/VOL/DAT) | ❌ not present |
| Audio DMA engine | ❌ not present |
| Audio DAC / stereo mixing | ❌ not present |
| Audio block-start / reload IRQs | ❌ not wired |
| ADKCON audio modulation (attach period / volume) | ❌ not implemented |
| ADKCON fast/slow-disk timing (`FAST_DISK`) | ❌ not implemented |
| Serial SERDAT / SERDATR / SERPER | ❌ not present |
| Serial TBE / RBF IRQs | ❌ not wired |
| POTGO / POT0DAT / POT1DAT / POTINP | ❌ not present |

## Archive coverage (`crates/commodore-paula-8364-archive/src/lib.rs`)

| Area | Archive state |
| --- | --- |
| INTENA / INTREQ with write log | ✅ `write_intena` / `write_intreq` + 16-deep log |
| `request_interrupt(bit)` helper | ✅ |
| IPL priority encoder | ✅ `compute_ipl()` — HRM mapping |
| ADKCON with set/clear | ✅ `write_adkcon` |
| Audio: 4× `AudioChannel` | ✅ full state: LC, LEN, PER, VOL, DAT, DMA cursor |
| Audio DMA slot service | ✅ `service_dma_slot` + `tick_dma_return` — 14-CCK return latency |
| Audio period clamp (≥124 CCK) | ✅ `MIN_AUDIO_PERIOD_CCK` |
| Audio attach-period / attach-volume modulation | ✅ ADKCON bits 0-7 |
| Audio DAC non-linearity (S-curve) | ✅ `DAC_TABLE` lookup |
| Stereo routing (0+3 L, 1+2 R) | ✅ `mix_audio_stereo` |
| Audio IRQs (AUDx on block start + reload) | ✅ |
| Disk DSKLEN double-write arming flip-flop | ✅ `dsklen_armed` |
| Disk byte timing (FAST_DISK = 14 vs 28 CCK) | ✅ `disk_byte_cck_delay` |
| DSKBYTR read semantics (DSKBYT/DMAON/DISKWRITE/WORDEQUAL/DATA) | ✅ |
| DSKSYNC word-equal latch + delay | ✅ |
| Disk PLL for IPF variable-rate | ✅ `disk_pll_accumulate` |
| Disk write logs (DMA vs PIO) | ✅ diagnostic |
| Serial | ❌ **absent from archive too** — lived in old machine crate |
| POTGO / POTxDAT | ❌ **absent from archive too** — lived in old machine crate |

## HRM cross-check

Spot-check: the archive's `compute_ipl` priority mapping matches HRM
*Hardware Reference Manual* p.33 (interrupt priority table):
- L6: EXTER (bit 13)
- L5: DSKSYN (12), RBF (11)
- L4: AUD3..0 (10..7)
- L3: BLIT (6), VERTB (5), COPER (4)
- L2: PORTS (3)
- L1: SOFT (2), DSKBLK (1), TBE (0)

Archive's DSKLEN arming mirrors the HRM *Disk Controller* text
verbatim: "the DMAEN bit in DSKLEN must be turned on twice in order
to actually enable the disk DMA hardware." The archive's bit-15=0
write resetting the arm matches the HRM-recommended `$4000` safety
write.

Audio minimum period of 124 CCK matches the HRM audio chapter
("programs should not use values below 124" because the DMA return
latency forbids it) and matches WinUAE's `AUDIO_MIN_PERIOD`.

## Known divergences from HRM to flag during port

1. **Audio DMA bootstrap seeds two requests**. Archive comment:
   *"Seed two requests to bootstrap current+next word fill in the
   simplified model, while still routing all actual fetches through
   audio DMA slots."* This is a simplification of the real chip's
   three-state start sequence (init → fetch LC → fetch LC+2). Worth
   verifying against WinUAE audio.cpp before port.

2. **DMA return latency hard-coded to 14 CCK**. Real Paula's latency
   depends on bus timing and slot availability. The 14-CCK constant
   is a model average; may need calibration against golden audio
   samples during Phase 3.

3. **Disk byte timing has no jitter**. The archive picks exactly
   14 or 28 CCK for the next byte. Real hardware has phase drift
   from the PLL; the PLL-variable-rate path (`disk_pll_accumulate`)
   is the IPF-specific override and doesn't run by default.

4. **Overrun not modelled on DSKBYTR**. Archive comment: *"Single-byte
   receive register semantics: a later byte can replace an unread
   earlier byte in this simplified model (overrun not modeled)."*
   HRM says DSKBYT clears on read and a second byte before read
   constitutes an overrun. Real software (trackdisk.device) doesn't
   rely on the overrun bit for error recovery, so this simplification
   is safe; flag it in comments during port.

## Architectural observation — serial + POTGO ownership

Serial and POTGO lived in the old machine crate
(`machine-commodore-amiga-archive`), not in the Paula archive. On real
hardware both are Paula's responsibility. Recommendation for the port:
pull serial (SERDAT/SERDATR/SERPER, TBE/RBF IRQs) and POTGO
(POTGO/POT0DAT/POT1DAT) into `commodore-paula-8364` alongside the
audio/disk/ICR concerns. Leaves the machine crate with just wiring.

## Per-phase plan

### Phase 1 (characterisation tests) — tasks #118–#122

Each concern gets its own integration test file against the archive.
Every test must pass before Phase 2 begins.

- **#118 INTENA/INTREQ**: SET/CLEAR semantics, master-enable gate,
  level 1-6 priority encoding, `request_interrupt` helper, write-log
  instrumentation.
- **#119 Audio channels**: register storage, DMA bootstrap, period
  clamp, block-start IRQ, reload IRQ, attach-period, attach-volume,
  combined attach, DAC table, stereo routing, modulator muting.
- **#120 Disk DMA + MFM sync**: DSKLEN double-write arming (and
  $4000 safety disarm), DSKBYTR fields, WORDEQUAL latch + delay,
  FAST_DISK timing, disk PLL accumulator, disk write logs,
  completion → DSKBLK IRQ.
- **#121 Serial**: write the tests *before* implementing — these
  start as red and are satisfied by Phase 2 #128.
- **#122 ADKCON**: set/clear, FAST_DISK bit, attach-period /
  attach-volume decode.

### Phase 2 (port) — tasks #123–#129

Same bulk-port approach as CIA:

1. Rename `commodore-paula-8364-archive` → `commodore-paula-8364` in
   the workspace Cargo.toml (keep it excluded until ready).
2. Add the `paula-8364` module to the machine's dependency list.
3. Tidy-pass the archive first (apply the API-shape lessons from the
   CIA cleanup: bits module, named constants, no back-door setters,
   peek() alongside read-with-side-effects).
4. Port in register-concern order — INTENA/INTREQ+ADKCON, audio
   registers, audio DMA, disk registers, disk DMA+MFM, add serial
   and POTGO (new scope, not a port).
5. Every Phase 1 test must pass against the ported code.

### Phase 3 (integrate) — task #130

Replace the machine's inline `chipset.dmacon` / `intena` / `intreq` /
`adkcon` / disk-register storage with Paula-crate calls. Retire the
`commodore-paula-8364-archive` directory name.

Blocks: #130 blocks the floppy and keyboard Phase 3s (they both want
the real Paula behaviour), so this is on the critical path.

## Conclusion

Archive is HRM-accurate with three minor documented simplifications.
Port should be bulk-safe using the same methodology that landed CIA.
The real scope enlargement is **serial + POTGO**, which never existed
in the Paula archive — they need writing from HRM, not porting.
