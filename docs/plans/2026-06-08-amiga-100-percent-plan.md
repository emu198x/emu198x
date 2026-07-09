> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Commodore Amiga to 100% — honesty tier, cycle-exact chipset, I/O fidelity, mass storage, the new-machine tail"
type: plan
date: 2026-06-08
system: docs/systems/commodore/amiga/index.md
basis: six code-grounded assessments (68k CPU family, Agnus/Copper/Blitter, Denise/Lisa, Paula, CIA/system/boot, storage/formats/drivability), 2026-06-08
---

# Commodore Amiga — road to 100%

What it would take to bring the Amiga to feature- and accuracy-complete, grounded
in a six-dimension code-level survey of the actual crates and tests. The system
doc was honest at the headline but thin and optimistic in its gaps; corrected as
part of this work.

## Executive summary

**The Amiga is a fourth distinct shape — "deep chips, shallow seams."** The other
three platforms each had one dominant story; the Amiga's is that its chips are
individually excellent *in isolation* but the **system isn't fully assembled**, and
three big subsystems are facades that the "boots Workbench 3.1" headline hides.

- **Spectrum** — finished core, cheap front-loaded breadth.
- **C64** — plays its library, hard core-accuracy long pole (VIC-II rewrite).
- **NES** — most-finished cores, breadth + two bugs.
- **Amiga** — the chips are deep and well-tested *as units* (the 68000 is the
  best-validated CPU in the fleet; every Blitter minterm and Copper instruction is
  implemented; OCS Denise is pixel-exact for playfields; Paula audio + IRQ are
  complete; the 8520s and AutoConfig are done). But the integration is unfinished,
  the flagship features are facades — and the seams between deep-but-isolated chips
  are where the bugs hide (the Agnus→Denise sprite handoff being a worked example,
  below).

The three facades the headline conceals:

1. **AGA renders as 12-bit.** The 256-colour 24-bit palette, HAM8, and 32/64-px
   wide sprites are decoded and stored but never displayed — the board path
   resolves every variant through the OCS 12-bit palette. AGA today is an ECS
   machine that boots the 3.1 desktop. **Denise: OCS ~95%, ECS ~90%, AGA ~40%.**
   *(2026-06-09: the Agnus→Denise hardware-sprite handoff — gap #162 — is now
   wired and displaying. Two seam bugs surfaced and were fixed once a real
   DMA sprite finally rendered: the board fed the sprite comparator
   pipeline-relative instead of absolute beam coordinates, so sprites never
   matched; and the shifter ran at one pixel per colour clock instead of per
   lores pixel, so every sprite was double-width. Workbench 1.3 now draws its
   mouse pointer. The OCS/ECS sprite path is sound; AGA wide sprites remain
   below.)*
2. **The Blitter runs in zero cycles** (synchronous on the BLTSIZE write). The
   correct incremental per-slot scheduler is *built and tested* but wired into no
   machine; there's no BBUSY/BZERO. Blitter timing and bus contention are absent.
3. **No disk save, no hard disk.** Floppy read is solid; disk write-back is built
   at the drive layer but unwired (a Workbench SAVE is silently lost), and there's
   no HDF/IDE path at all (Gayle is a stub).

**The encouraging part:** almost none of this is novel chip research. The hard,
risky logic — the 68000 core, the 256 Blitter minterms, line-mode octants, the AGA
FMODE fetch cadence, the FPU math, the MMU table-walk — **is already written and
tested.** The remaining work is overwhelmingly **wiring, integration, and render
paths**, with the riskiest correctness already retired. The two in-flight refactors
(single-bus-per-cck, unified-driver) *are* the critical path to a cycle-exact
chipset, not a distraction.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — Honesty tier | make AGA actually render (24-bit palette + HAM8 + wide sprites), wire disk save, fix the two CIA-B bugs, doc honesty | **~5–7 weeks** |
| B — Cycle-exact chipset | single-bus-per-cck unification, incremental Blitter (+ BBUSY/BZERO), copper↔blitter WAIT, unified driver, DIW fetch trimming | **~6–9 weeks** |
| C — Audio + I/O fidelity | LED/RC filter + PWM, serial baud + host transport, POT analog, true mouse quadrature | **~3–4 weeks** |
| D — Mass storage + variants | hardfile/HDF + Gayle IDE (WB-on-HD boot), KS 2.0x proof + version gating, extended-ADF/IPF/DMS, RF5C01A RTC, Zorro-II chaining | **~6–9 weeks** |
| E — New machines + advanced CPU | CDTV, CD32 (+Akiko), A3000 (68030+MMU+Zorro-III+SCSI), A4000 (68040+FPU), FPU/MMU F-line wiring, 020 ISA gap-fill + timing, PCMCIA/Ethernet/RTG | **~18–28 weeks** |

**True 100% of everything ≈ 38–57 weeks** — by far the largest in the fleet, because
the Amiga *is* the largest machine and Tier E is five-or-more net-new subsystems.
But the distribution is friendly: the **"honest AGA Amiga that saves disks" slice
(Tier A) is ~5–7 weeks**, and a **cycle-exact OCS/ECS/AGA games-and-demos machine
(A+B+C) is ~14–20 weeks**. Tier E is the long preservation/high-end tail and is
mostly *additive* — new machines, not fixes to the launch surface.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Honesty tier (make the headline true)

| Item | Effort | Notes |
|------|--------|-------|
| **AGA 24-bit palette render** | **L** | Resolve pixels through `palette_24` (ARGB direct) instead of the OCS 12-bit `resolve_color_rgb12`. The register decode + storage already exist; only the render path is missing. Unlocks every AGA program. |
| **HAM8** | **L** | 8-bit hold-and-modify with 24-bit chaining (`ham_prev_rgb24` exists). The other critical AGA-software item. |
| **AGA wide sprites (32/64px)** | **M** | Wire FMODE → the inner OCS `spr_width` the shifter reads (the AGA wrapper writes a dead field; nothing even calls it). |
| **BPLCON4 BPLAM bitplane XOR** | **S** | Stored, not applied to the bitplane index. |
| **ADF disk write-back** | **M** | De-risked: `flush_write_capture`/`save_adf` are built and unit-pass in the floppy crate with zero callers. Wire Paula's write-DMA → `note_write_mfm_word`, add a write-side DSKBLK completion, add a runtime `flush_df0`/`save_disk` surface + writable-mount flag + write-protect sense, mirroring the C64 `disk-save-write-back.md` decision. Add a round-trip test. |
| **CIA-B FLAG (disk index) + TOD wiring** | **S + S** | Route the discarded `drive.tick()` index pulse to `cia_b.flag_falling_edge()` and pulse CIA-B TOD. Correctness. |
| **Doc honesty pass** | **S** | `chipset/denise.md` reads as an optimistic spec (overstates the AGA render path); `knowledge/amiga/paula-8364-porting-gap-list.md` is badly stale (describes Paula as unimplemented vs a `chipset.rs` that's gone); the `agnus-aga` "FMODE not wired" comment is now false. Correct all three. |

## Tier B — Cycle-exact chipset (the integration spine)

The two named in-flight refactors are items 1 and 4 here — they *are* the unlock.

| Item | Effort | Notes |
|------|--------|-------|
| **Single-bus-per-CCK unification** | **L** | Collapse Agnus's `cck_bus_plan` and Denise's `dma_claim` into one authority; reconcile the even-vs-odd copper-slot polarity disagreement. The foundation everything else needs. |
| **Incremental Blitter** | **L** | Replace `run_blit_to_completion()` with the already-built-and-tested `tick_blitter_scheduler` path. Delivers blitter DMA contention + blitter-nasty (BLTPRI) in one move. |
| **BBUSY + BZERO readback** | **S** | Inject status bits into DMACONR ($002); compute BZERO across the blit. Gated on the incremental blitter. |
| **Copper↔Blitter WAIT (BFD=0) sync** | **M** | Plumb `blitter_busy` into `copper.tick_cck` and honour BFD=0. Gated on the incremental blitter. |
| **Unified driver replatform** | **L** | Merge the three machine `tick()` loops so B lands once, not three times. Do it alongside the single-bus work to avoid triple-maintenance. |
| **Horizontal DIW fetch trimming + ECS programmable sync** | **M–L** | DIW-driven fetch hard-stop (only DDF is modelled today) + a real ECS VARBEAMEN sync generator (currently coarse) for overscan/genlock-precise output. |
| **ECS large blits (true BLTSIZV/BLTSIZH)** | **M** | Currently packed back into the legacy 10+6-bit BLTSIZE, so >1024-line / >63-word blits wrap. |
| **AGA 8-plane-lowres + wide-sprite DMA fetch** | **S–M** | Tables exist (`LOWRES_DDF_TO_PLANE_AGA`, `spr_fetch_width`) but aren't routed through `current_slot`/sprite fetch. |

## Tier C — Audio + I/O fidelity

| Item | Effort | Notes |
|------|--------|-------|
| **Audio output filter** | **M** | The fixed RC low-pass + the LED-switchable Butterworth (gated off CIA-A PRA bit 1). Today the output is unfiltered. |
| **Paula volume-PWM / aliasing** | **M–L** | The 6-bit PWM volume + period-driven resampling for authentic Paula character. |
| **Audio DMA-start 3-state + bus-derived latency** | **S–M** | Replace the "seed two requests" simplification + the hard-coded 14-CCK return latency. |
| **Serial baud timing + host transport** | **M** | SERPER divisor is stored but unused (TX is instantaneous) and `receive_serial` has no caller. A PTY/TCP bridge here also unlocks the AmiTCP/Miami internet path + the Rachel netplay goal. |
| **POT analog ramp** | **S–M** | RC-charge ramp for paddles/proportional controllers (digital mouse buttons already work). |
| **True mouse Gray-code quadrature** | **S** | Position-delta counter today; demos reading raw quadrature timing differ. |

## Tier D — Mass storage + variants breadth

| Item | Effort | Notes |
|------|--------|-------|
| **Hardfile/HDF + RDB + Gayle IDE drive** | **L** | The "boots a real Workbench-on-hard-disk install" headline. Host-side HDF read/write + minimal IDE-drive-behind-Gayle (FFS comes from AmigaOS, so no host FFS needed for boot). The full donor IDE path is harvestable from `Emu198x-Oldest/`. |
| **KS 2.0x ECS boot proof + version gating** | **M + S** | A500+/A600 ECS boot is plausibly working but has no golden/CI row; add it. Plus a Kickstart-version detection/mismatch guard (a user pairing KS 1.3 with an A1200 silently misbehaves today). |
| **Extended ADF** | **M** | Non-standard/copy-protected tracks. |
| **IPF (SPS)** | **L** | The preservation format for many originals; needs the CAPS/SPS flux decode. The `format-ipf` crate is referenced but was never created. |
| **DMS / ADZ** | **M / S** | Disk Masher / gzipped ADF wrappers in the media path. |
| **RF5C01A RTC** | **M** | Some A1200s/A3000s use the Ricoh, not the modelled MSM6242. |
| **Zorro-II multi-board chaining** | **M** | One fast-RAM board today; real machines chain several (+ non-RAM board types). |
| **Parallel port** | **M** | Printer / sound-digitiser; CIA-B PRB isn't bridged to any host device. |

## Tier E — New machines + advanced CPU (the long tail)

| Item | Effort | Notes |
|------|--------|-------|
| **A3000 (68030)** | **XL** | 68030 + MMU F-line wiring + Zorro-III + Super-DMAC SCSI + Ramsey/Fat Gary. |
| **A4000 (68040)** | **XL** | 68040 + on-chip FPU/MMU + MOVE16/CINV/CPUSH + IDE + Zorro-III. |
| **FPU F-line dispatch + wiring** | **L–XL** | The math (`motorola-68040/src/fpu.rs`, 705 LoC) exists and is tested but is called from nowhere — needs F-line ($Fxxx) decode + cpID-1 routing + extended-precision accuracy + a corpus (which doesn't exist publicly — needs a WinUAE oracle). Unlocks 68881/2 for any 020+. |
| **MMU decode + bus integration** | **XL** | `motorola-68030/src/mmu.rs` (2421 LoC) exists and is tested but does no translation — needs PMOVE/PFLUSH/PTEST/PLOAD decode + ATC/table-walk-during-bus-cycle + restartable bus-error frames (Format $8/$B/$7). Needed for A3000/A4000 virtual memory. |
| **68020 ISA gap-fill** | **M** | CAS/CAS2, CHK2/CMP2, TRAPcc, PACK/UNPK, Bcc.L, CALLM/RTM, MUL/DIV memory-source. |
| **68020+ cycle timing** | **M** | The 3-clock bus / I-cache / barrel-shifter constant-time model (the wrapper runs the 68000's 4-clock timing today, overstating A1200 counts). Accuracy-only — correctness already works. |
| **CDTV** | **L** | OCS + CD drive + ISO/CUE + DMAC SCSI-CD. |
| **CD32 (+Akiko)** | **XL** | Needs working AGA video first, then the Akiko chunky-to-planar + CD controller + the CD32 11-button serial pad + boot ROM. The biggest single net-new block. |
| **PCMCIA (SRAM/CF/NE2000)**, **A2065 Ethernet**, **RTG (Picasso96/uaegfx)** | **L each** | Net + retargetable-graphics breadth; the long-term-scope (Vampire/PiStorm/RTG) targets. The pin-level no-Bus-trait CPU surface already accommodates a non-Motorola AC68080/PiStorm core without redesign. |

## Recommended sequence (highest leverage first)

1. **AGA 24-bit palette + HAM8 + wide sprites** (L+L+M) — make the flagship's
   headline feature true. Highest leverage: today AGA software doesn't render.
2. **ADF disk write-back** (M) — de-risked, follows the C64 blueprint; without it
   Workbench can't save. Plus the two CIA-B wiring bugs (S+S) and the doc-honesty
   pass (S) while in the area.
3. **Single-bus-per-cck + incremental Blitter + unified driver** (L+L+L) — the
   chipset cycle-exactness spine; unlocks demos and timing-sensitive games.
   BBUSY/BZERO + copper↔blitter WAIT fall out (S+M).
4. **Audio filter + serial host transport** (M+M) — audible fidelity + the net path.
5. **Hardfile/HDF + Gayle IDE** (L) — "boots a real Workbench install"; harvest the
   donor IDE path. KS 2.0x proof + version gating (M+S) alongside.
6. **Formats (extended-ADF/IPF/DMS/ADZ) + RTC + Zorro chaining** (M/L/M/S + M + M)
   — preservation breadth.
7. **FPU + MMU wiring → A3000/A4000** (XL+XL) then **CDTV/CD32** (L/XL) — the
   high-end + CD long tail, mostly additive new machines.

## Key files

- CPU: `crates/motorola-68000/src/{cpu,decode,ea,execute}.rs` (the engine), `crates/motorola-680{10,20,30,40}/src/cpu.rs` (wrappers), `crates/motorola-68040/src/fpu.rs` + `crates/motorola-68030/src/mmu.rs` (dormant), `crates/motorola-68000/tests/{tom_harte,isa_conformance}.rs`.
- Agnus: `crates/commodore-agnus-ocs/src/agnus.rs` (`current_slot`/`cck_bus_plan`/`tick_blitter_scheduler`), `crates/common-commodore-amiga/src/{copper.rs,denise.rs}` (the live copper + `dma_claim`).
- Denise: `crates/commodore-denise-ocs/src/chip.rs` (the engine), `crates/commodore-denise-aga/src/lib.rs` (storage-only AGA), `crates/common-commodore-amiga/src/denise.rs:402` (the 12-bit board render loop to fix).
- Paula: `crates/commodore-paula-8364/src/lib.rs` (audio/disk/IRQ/serial/POT), `crates/peripheral-commodore-amiga-floppy/src/lib.rs` (`flush_write_capture`/`save_adf`, unwired).
- System: `crates/mos-cia-8520/src/lib.rs`, `crates/common-commodore-amiga/src/{memory.rs,rtc.rs}`, `crates/commodore-amiga-autoconfig/src/lib.rs`, `crates/commodore-gayle/src/lib.rs` (IDE stub), `crates/machine-commodore-amiga-{ocs,ecs,a1200}/src/lib.rs` (3× tick loops), `crates/runtime-commodore-amiga/{src/profiles.rs,tests/golden_matrix.rs}`.
- Decisions/reference: `knowledge/decisions/disk-save-write-back.md`, `knowledge/decisions/motorola-68k-variant-pattern.md`; vAmiga, WinUAE, fs-uae, Minimig-AGA (`emulators/amiga/`); the donor IDE/NE2000 in `Emu198x-Oldest/`.
