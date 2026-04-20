# Amiga emulator restart — incremental boot-driven build (all variants)

**Date:** 2026-04-19
**Status:** Approved and underway. M0-M6 complete (Phase A).

## Progress (live)

| Milestone | Status | Tests | Notes |
|---|---|---|---|
| M0 | ✅ | 4 | CPU + ROM + OVL mapping. Reset vector reads correct. |
| M1 | ✅ | 3 | Chip RAM + CPU bus integration. 100K CCKs without bus error. |
| M2 | ✅ | 2 | Custom-register storage with set/clear semantics. |
| M3 | ✅ | 2 | OVL clear via CIA-A PRA bit 0. |
| M4 | ✅ | 1 | Chip-RAM aliasing for the size probe. |
| M5 | ✅ | 1 | Bootstrap ExecBase placed at $0676. |
| M6 | ✅ | 2 | Beam counter (PAL) + VBL → INTREQ.VERTB → CPU IPL. |
| M7 | ✅ | 2 | VPOSR/VHPOSR + CIA-A input-bit floating-high. |
| M8 | ✅ | 2 | CIA-A timer A/B + ICR + CIA→Paula IRQ. |
| M9 | ✅ | 2 | CIA-B stub at $BFD000 + IRQ→EXTER. |
| M10 | next | — | Copper module + DMA scheduling. |
| M11-M12 | pending | — | Bitplane DMA + Denise pixels, slow-RAM golden. |

Total tests: **35 passing** (21 integration + 14 unit). Diagnostic
shows boot reaches PC=$FC3132 / SSP=$7FFFC0 / INTENA=$202C / ExecBase
= $0676 / AttnFlags = $FFFF_0000 by 50M CCKs.

### Why the boot still stalls at INTENA=$202C

The path to re-enable INTENA master is reached:

```
$FC3080: TST.L  $126(A6)        ; AttnFlags+AttnResched
$FC3084: BGE.S  $FC308E         ; skip if non-negative
$FC3086: MOVE.W #$C000, $DFF09A ; INTENA |= MASTER
```

AttnFlags correctly sets to $FFFF_0000 (negative as signed long), so
BGE doesn't take and the master-enable IS executed. But SR=$2718
(supervisor mode, IPL=7) means the CPU is masking ALL interrupts.
The boot is inside a critical section.

What reduces IPL from 7? Either an RTE that pops a less-masked SR,
or an explicit MOVE TO SR. Both happen at end of larger init phases
that depend on subsystems we haven't added (Paula audio/disk
register reads, copper, etc).

### Where we are: parity with the archived chip-only investigation

After M0-M9, the restart reproduces the **exact same stall point**
the old emulator hit on chip-only KS 1.3 (per
`amiga-chip-only-boot-failure.md`). Diagnostic findings:

- Boot reaches user mode, OVL clear, ExecBase placement, AttnFlags
  setup, INTENA cycling.
- Peak INTENA = `$602C` — master enable IS reached (bit 14 latches
  briefly).
- 71 INTENA writes over 500M CCKs ≈ 1 write per 7M CCKs.
- Boot cycles `Disable() → work → Enable() → busy-wait → Disable()`
  forever; the "work" doesn't advance.
- Only **6** chipset-register reads in 50M CCKs (boot isn't
  chipset-polling).
- SSP slowly decreases (subroutines run) but never reaches
  display init.

This is the **same KS 1.3 chip-only deadlock** the archived
investigation characterised: the boot needs proper copper + Denise
+ bitplane DMA to escape its outer wait loop. The archive
documented that the V34 ROM uses ExecBase as a copper-list
placeholder, which only works once proper graphics.library init
allocates a real list — and that init can't run because the
scheduler can't dispatch the task that would do it.

### Pickup notes for next session

- Diagnostic: `cargo test -p machine-commodore-amiga-ocs --test
  m5_diagnostic --release -- --ignored --nocapture`
- Register-read counter: `cargo test -p machine-commodore-amiga-ocs
  --test m8_register_read_diagnostic --release -- --ignored --nocapture`
- **M9-M11 must be done together** to make further forward progress
  meaningful. Single-milestone steps from here likely won't move
  the boot's state. Consider:
  - M9: Paula skeleton (audio/disk register storage; no DMA yet)
  - M10: Copper module + DMA scheduling for copper slots
  - M11: Bitplane DMA + Denise pixel pipeline
  - M12: Slow-RAM golden test (the FS-UAE pixel-exact target)



## Two-stage scope

**Stage 1 — Insert-disk screen for every variant.** All 12 machines
reach their characteristic boot screen. This is what we've been
building toward; what the existing FS-UAE goldens validate.

**Stage 2 — Workbench boot for every variant.** Each machine boots
its appropriate Workbench from disk (floppy or CD), reaches the
Workbench screen with disk icon, mouse cursor responsive. User has
all Workbench disk images available.

Stage 2 is significantly larger because it requires functional disk
I/O (floppy MFM read, IDE/SCSI for HDD-boot variants, CD-ROM for
CDTV/CD32) and the full DOS / filesystem boot chain. We treat Stage
2 as a follow-on phase after each variant's insert-disk milestone in
Stage 1 passes.

## Scope: every Amiga variant

Target machines:

| # | Variant | CPU | Chipset | Chip RAM | ROM | Special chips |
|---|---|---|---|---|---|---|
| 1 | A1000 | 68000 | OCS | 256K (or 512K) | KS 1.2 (V33) | — |
| 2 | A500 | 68000 | OCS | 512K | KS 1.3 (V34) | — |
| 3 | A500 + slow RAM | 68000 | OCS | 512K + 512K slow | KS 1.3 | — |
| 4 | A2000 | 68000 | OCS | 512K | KS 1.3 / 2.04 | Zorro II |
| 5 | CDTV | 68000 | OCS | 512K | KS 1.3 + cdtv.rom | DMAC, CD-ROM |
| 6 | A500+ | 68000 | ECS | 1M (Fat Agnus) | KS 2.04 (V37) | — |
| 7 | A600 | 68000 | ECS | 1M | KS 2.05 (V37) | Gayle (IDE/PCMCIA) |
| 8 | A3000 | 68030 | ECS | 1M | KS 2.04 / 3.1 | Ramsey, Buster, DMAC SCSI, Zorro III |
| 9 | A1200 KS3.0 | 68EC020 | AGA | 2M (Alice) | KS 3.0 (V39) | Gayle, Budgie |
| 10 | A1200 KS3.1 | 68EC020 | AGA | 2M | KS 3.1 (V40) | Gayle, Budgie |
| 11 | A4000 | 68030/040 | AGA | 2M | KS 3.0 / 3.1 | Ramsey, Buster, IDE, Zorro III |
| 12 | CD32 | 68EC020 | AGA | 2M | KS 3.1 + cd32.rom | Akiko, CD-ROM |

Plus Fast RAM variants of all of the above. Per the user's note, Fast
RAM should not affect boot to insert-disk (autoconfig'd memory only
enters the picture after the boot allocator is up). We test it after
basic boot works for each variant — surfaces any chip-vs-fast routing
bugs.

## ROM availability

All ROMs in `~/.emu198x/roms/commodore-amiga/` (alternate revisions
and region variants archived to `_archive/`):

- ✓ `kick12.rom` (V33.180) — A1000, A500, A2000
- ✓ `kick13.rom` (V34.005) — A500, A2000
- ✓ `kick204.rom` (V37.175) — A500+
- ✓ `kick205.rom` (V37.299) — A600
- ✓ `kick30.rom` (V39.106) — A1200 KS3.0
- ✓ `kick30a4000.rom` (V39.106) — A4000 KS3.0
- ✓ `kick31.rom` (V40.063) — A500, A600, A2000 KS3.1
- ✓ `kick31a1200.rom` (V40.068) — A1200 KS3.1
- ✓ `kick31a3000.rom` (V40.068) — A3000 KS3.1
- ✓ `kick31a4000.rom` (V40.068) — A4000 KS3.1
- ✓ `kick31cd32.rom` (V40.060) — CD32 KS3.1
- ✓ `cdtv.rom` — CDTV extension ROM (V1.0)
- ✓ `cd32.rom` — CD32 extension ROM (V40.60)
- ✓ `a1000-bootstrap.rom` — A1000 boot ROM (loads Kickstart from disk)
- ✓ `a3000-boot-rom0.rom` / `a3000-boot-rom1.rom` — A3000 SuperKickstart
  boot ROMs (V1.4 r36.16, the bootstrap that loads Kickstart from disk
  on early A3000s)

Tests skip silently when ROM is missing.

## Why restart

## Why restart

Five passes of investigation on the chip-only KS 1.3 boot failure (see
`amiga-chip-only-boot-failure.md`) ended at "real V34 must do something
we're not modelling, but we don't have cycle-accurate reference data to
identify what." The deeper diagnosis is structural: we built the entire
Amiga chipset before testing the boot end-to-end, so latent bugs hide
across multiple subsystems and only surface when configurations
diverge. The slow-RAM config passes pixel-exact against FS-UAE, but
that's coincidence-of-cancellation rather than evidence of correctness.

This pattern is exactly what `feedback_cycle_accurate_from_start.md`
warns against: "cycle accuracy must be foundational, not retrofitted."

## What this plan is, what it isn't

**Is**: a restart of the *integration* and *chipset* layers, driven by
the actual KS 1.3 boot sequence, one ROM access at a time, with a test
at every milestone.

**Isn't**: a from-scratch CPU. The 68000 in `mos-68000` passes 26 of
43 Tom Harte opcodes including all load-bearing boot instructions. We
keep it.

## Principles

1. **Hardware-driven.** Implement only what the running ROM demands.
   No pre-built audio, no sprites until the boot uses sprites.
2. **Verified per step.** Every milestone has a test that proves it.
   No milestone is "done" without a passing test.
3. **Reference-anchored.** Each chip behavior matches documented
   hardware behavior, not "what makes the next test pass."
4. **No retrofitting.** Cycle accuracy from day 1; per-feature tests
   from day 1.
5. **Boot-shaped tests.** Every test runs the real KS 1.3 ROM and
   asserts state at a specific point in real boot. No synthetic
   "drive Agnus directly" tests until the chipset is well-understood
   in isolation.

## Crate structure

The "build everything minimally" goal drives the structure. Each chip
is its own crate; each chipset family has its own version of the
chips that differ; one machine crate per CPU+chipset combo wires it
all together.

### Chip crates (one per chip, per chipset variant)

```
crates/
  motorola-68000          (CPU — exists, kept)
  motorola-68020          (new — needed for A1200, CD32)
  motorola-68030          (new — needed for A3000, A4000)
  motorola-68040          (new — needed for some A4000) — defer to phase D

  commodore-agnus-ocs     (new — Agnus 8370/8371, 512K chip max)
  commodore-agnus-ecs     (new — Fat Agnus 8372A, 1M/2M chip)
  commodore-alice-aga     (new — Alice 8374, 2M chip, full bandwidth)

  commodore-denise-ocs    (new — Denise 8362)
  commodore-denise-ecs    (new — ECS Denise 8373, productivity / SuperHi-res)
  commodore-lisa-aga      (new — Lisa 4203, AGA color modes)

  commodore-paula         (new — Paula 8364, used by all)
  mos-cia-8520            (new — same chip on all variants)

  commodore-gary          (new — Gary address decoder, A500/A2000)
  commodore-gayle         (new — A600/A1200 IDE+PCMCIA bridge)
  commodore-akiko         (new — CD32 chunky-to-planar + CD)
  commodore-ramsey        (new — A3000/A4000 memory controller) — phase C/D
  commodore-buster        (new — A3000/A4000 Zorro III bridge) — phase C/D
  commodore-budgie        (new — A1200 chip RAM controller)
  commodore-dmac          (new — A3000 SCSI, CDTV CD-ROM)

  peripheral-commodore-amiga-floppy
  peripheral-commodore-amiga-keyboard
  peripheral-commodore-amiga-cdrom    (new — CDTV/CD32)
  peripheral-commodore-amiga-ide      (new — A600/A1200/A4000)
  peripheral-commodore-amiga-scsi     (new — A3000)
```

### Machine crates (one per CPU+chipset combination)

```
crates/
  machine-commodore-amiga-ocs      (68000 + OCS — A1000/A500/A2000/CDTV)
  machine-commodore-amiga-ecs      (68000 + ECS — A500+/A600)
  machine-commodore-amiga-ecs-030  (68030 + ECS — A3000)
  machine-commodore-amiga-aga-020  (68EC020 + AGA — A1200/CD32)
  machine-commodore-amiga-aga-030  (68030 + AGA — A4000/030)
  machine-commodore-amiga-aga-040  (68040 + AGA — A4000/040)
```

Each machine crate wires the appropriate chips and exposes a single
`AmigaXxx::new(rom, model_config)` constructor. The `model_config`
selects between machine variants within the same CPU+chipset combo
(e.g. A500 vs A1000 — both are 68000+OCS; differ in the address
decoder, available RAM, peripheral set).

## What stays

- `crates/mos-68000` — CPU. Passes Tom Harte for boot instructions.
- `crates/format-commodore-amiga-adf` — ADF parsing. No chip
  dependencies, well-tested.
- `Emu198x-Reference/_organised/by-system/commodore-amiga/` — reference
  docs (boot traces, register references, etc.).
- `crates/machine-commodore-amiga/tests/golden/` — FS-UAE captures (4
  active + 3 ECS/AGA preserved for future).
- `crates/machine-commodore-amiga/tests/support/mod.rs` — golden
  comparison helpers.
- Tom Harte 68000 fixtures at `~/Projects/Emu198x-archive/test-data/`.

## What gets archived

Renamed `<name>` → `<name>-archive` so the code stays readable but is
out of the build:

- `crates/machine-commodore-amiga` (integration layer)
- `crates/runtime-commodore-amiga` (runtime wrapper)
- `crates/commodore-agnus-ocs`
- `crates/commodore-denise-ocs`
- `crates/commodore-paula-8364`
- `crates/commodore-gary`
- `crates/mos-cia-8520`
- `crates/peripheral-commodore-amiga-floppy`
- `crates/peripheral-commodore-amiga-keyboard`

Reasons to keep them around archived (not deleted):
- Slow-RAM golden passes against current chipsets — there's correct
  behavior in there worth referencing.
- Test files (chip_only_*, intena_progression, etc.) document the
  failure modes we now want to avoid.
- Diagnostic infrastructure (debug_cpu_write_log, etc.) is reusable.

## Build order — phased

The 12 variants split into four phases by chipset family / CPU. Each
phase has its own M0-Mn milestones, but later phases reuse most of
what earlier phases built (only the chip differences matter).

| Phase | Variants | New chips needed | Estimated effort |
|---|---|---|---|
| **A: OCS** | A1000, A500, A500+slow, A2000, CDTV | OCS Agnus, OCS Denise, Paula, CIA, Gary, floppy, kbd, DMAC (CDTV), CD-ROM (CDTV) | 3-5 weeks (foundational) |
| **B: ECS** | A500+, A600 | ECS Agnus, ECS Denise, Gayle (A600), IDE skeleton (A600) | 1-2 weeks (mostly delta from OCS) |
| **C: AGA + 020** | A1200/3.0, A1200/3.1, CD32 | 68EC020, Alice, Lisa, Budgie, Akiko (CD32), CD-ROM (CD32) | 2-4 weeks |
| **D: Big-box** | A3000, A4000 | 68030 (A3000), 68040 (A4000), Ramsey, Buster, SCSI (A3000), IDE (A4000) | 3-5 weeks |

**Phases A → B → C → D in order.** Each variant within a phase has
its own boot-to-insert-disk milestone (with its own golden test).

## Phase A milestones (OCS — current focus)

Each milestone has: **Goal**, **Reference**, **Test**, **Add**, **Don't add yet**.

A milestone's "Add" list is exhaustive — anything not listed is not
built at that milestone. If an instruction needs something not yet
built, that's the trigger to advance to the next milestone (or split
into a sub-milestone).

### M0 — Reset to first instruction

- **Goal**: CPU fetches reset vector from ROM-via-OVL, executes 1
  instruction.
- **Reference**: `kick13.rom` reset vector. SSP at file offset 0,
  PC at offset 4.
- **Test**: After 1 reset cycle and enough CCKs for the prefetch,
  `cpu.regs.pc` = ROM bytes 4-7 as a longword.
- **Add**:
  - 28.375 MHz master oscillator (PAL); ticks CCK at master/4.
  - 68000 CPU integrated at 2 CCLK per CCK.
  - 512 KiB Kickstart ROM at `$FC0000` (and 4-way mirror to
    `$F80000-$FFFFFF` since KS 1.3 is 256K — actually 256K in our
    case, so the mirror covers $F80000-$FFFFFF).
  - Default OVL=1 maps ROM to `$0-$7FFFF` for reads.
  - Memory writes outside ROM range silently drop.
  - Memory reads outside ROM/OVL range return `$FF` (floating bus).
- **Don't add**: chip RAM (no writes go anywhere yet), custom
  registers, CIAs, interrupts, DMA, anything else.

### M1 — First chip RAM write

- **Goal**: Boot proceeds to the first instruction that writes to a
  chip-RAM address (non-ROM, non-custom).
- **Reference**: V34 disassembly from `$FC00D2` forward. Identify the
  PC of the first chip-RAM write.
- **Test**: After N CCKs, the chip-RAM-write completes; reading the
  written address returns the written value.
- **Add**:
  - 512 KiB chip RAM at `$0-$7FFFF`.
  - Address decode: reads to `$0-$7FFFF` go to chip RAM (when OVL=0)
    or ROM (when OVL=1). Writes to `$0-$7FFFF` go to chip RAM
    regardless of OVL (real Amiga behaviour).
- **Don't add**: chip RAM mask/aliasing for `$80000+`; that comes when
  the chip-RAM probe needs it (M3 or later).

### M2 — First custom-register access

- **Goal**: Boot proceeds to first `$DFFxxx` read or write.
- **Reference**: V34 disassembly. Identify register, access type.
- **Test**: After N CCKs, the access completes correctly. If it's a
  write, the register state is preserved. If a read, the read returns
  reset-default value.
- **Add**:
  - Address decode for `$DFF000-$DFF1FE` (custom register space).
  - Storage for the specific register accessed (a single field).
  - Read returns stored value; write stores new value.
- **Don't add**: any chipset *behavior* beyond raw storage. No DMA,
  no copper, no Denise interpretation. The register is just a
  variable.

### M3 — OVL clear via CIA-A

- **Goal**: Boot writes to CIA-A `$BFE001` (PRA bit 0) to disable OVL.
- **Reference**: V37 trace `$F8003E-$F8004C` (V34 has same shape).
  Documented in `amiga-rom-boot-traces.md` §"OVL clear".
- **Test**: After the OVL clear, reads from `$0-$7FFFF` go to chip
  RAM, not ROM. The reset-vector word at `$0` is now zero (or
  whatever chip RAM was initialised to), not the ROM SSP value.
- **Add**:
  - CIA-A address decode at `$BFE001` (PRA odd-byte).
  - PRA bit 0 (OVL) toggles overlay state in memory map.
- **Don't add**: full CIA emulation. Just PRA bit 0. Other CIA
  registers reach M3 as "stored but no behaviour."

### M4 — Chip-RAM probe (incomplete address decode)

- **Goal**: Boot writes magic patterns to `$0`, `$4000`, ..., `$80000`
  and detects 512K via wrap.
- **Reference**: V37 trace §"Phase 7: Chip RAM Probe ($F801CE-$F801FC)".
- **Test**: After the probe, `MaxLocMem` (computed by the boot, not
  set by us) equals `$80000`.
- **Add**:
  - Chip RAM address mask (`addr & $7FFFF`) for both reads and writes
    in the `$0-$1FFFFF` range.
- **Don't add**: anything else.

### M5 — Bootstrap ExecBase placement

- **Goal**: Boot allocates the bootstrap ExecBase in chip RAM, writes
  ExecBase pointer to `$00000004`.
- **Reference**: V37 trace §"Phase 8". Our ExecBase pointer should
  match what V34 places (likely `$0676` for chip-only — verified by
  earlier investigation).
- **Test**: After the placement, `read_long($4)` returns the ExecBase
  pointer; ChkBase at ExecBase+`$26` is the one's-complement.
- **Add**: nothing chipset-side — this is pure CPU + chip RAM. If
  earlier milestones are right, this works without new code.
- **Don't add**: any chipset.

### M6 — DMACON write enables DMA bits

- **Goal**: Boot writes `$DFF096` to enable master DMA + bitplane DMA.
- **Reference**: V34 disassembly. Identify the DMACON write.
- **Test**: After the write, `dmacon` field reflects the bits set
  (using the set/clear semantics: bit 15 = SET if 1, CLEAR if 0).
- **Add**: DMACON register handling with set/clear semantics. No DMA
  behavior; just the register state.
- **Don't add**: actual DMA scheduling.

### M7 — INTENA / INTREQ register handling

- **Goal**: Boot programs the interrupt controller.
- **Reference**: V34 disassembly. Identify INTENA writes (we found
  many earlier — `$FC130C`, `$FC141E`, etc.).
- **Test**: After each write, `intena` reflects the set/clear
  operation correctly. No interrupts fire yet.
- **Add**: INTENA / INTREQ registers with set/clear semantics. Master
  bit (bit 14) tracked but doesn't gate any IRQ delivery yet.
- **Don't add**: actual interrupt delivery to CPU. Paula doesn't
  exist yet as a module — these are just registers.

### M8 — Vertical-blank interrupt

- **Goal**: Boot installs a VBL handler and starts receiving VBL
  interrupts.
- **Reference**: V34's VBL setup; visible at intena_progression frame
  ~85+ where VERTB enables.
- **Test**: After M8-N CCKs, the boot's VBL counter (somewhere in
  ExecBase or its locals) increments per VBL.
- **Add**:
  - Beam counter (vpos / hpos) in Agnus.
  - VBL detection (vpos crosses 312 PAL).
  - Interrupt level 3 delivery to CPU when VERTB requested+enabled.
- **Don't add**: copper, blitter, sprites, bitplane DMA, audio.
  Just the beam + VBL.

### M9 — Copper executing a list

- **Goal**: Boot sets COP1LC, enables COPEN, copper starts executing.
- **Reference**: V34's first COP1LC write (we found this at
  `$FCAC82`).
- **Test**: After COPEN enable, the copper executes the list at
  COP1LC. Specifically: writes to chip registers from copper MOVE
  instructions appear in the register state.
- **Add**:
  - Copper module with MOVE / WAIT / SKIP instruction execution.
  - DMA scheduling: copper gets specific CCK slots (4n+1 in non-
    bitplane region per A500 timing).
- **Don't add**: blitter, sprites, audio, bitplane DMA. Copper only.

### M10 — Bitplane DMA + Denise pixel output

- **Goal**: Boot enables bitplane DMA, Denise produces pixels for the
  insert-disk screen.
- **Reference**: V34 sets BPLCON0 + DDFSTRT/DDFSTOP + bitplane
  pointers. Pixel format is documented in
  `amiga-custom-chips-reference.md`.
- **Test**: After M10-N CCKs, the framebuffer has non-black pixels
  (the boot has written palette + bitplane data; Denise should
  produce pixels). Just a pixel-count test, not pixel-exact yet.
- **Add**:
  - Bitplane DMA (Agnus): reads bpl pointers, fetches bitplane data
    into Denise shift registers per CCK.
  - Denise: pixel output from BPL shift registers + palette + BPLCON0
    BPU decode. The fix from this session
    (`amiga-denise-bpu-zero-rendering.md`) applies.
- **Don't add**: blitter, sprites, audio, dual-playfield, HAM, EHB.

### M11 — Insert-disk screen (slow-RAM A500)

- **Goal**: Boot reaches the insert-disk screen at frame 250 with the
  slow-RAM-expanded A500 config.
- **Reference**: existing FS-UAE golden
  `a500-ks13-512k-chip-512k-slow-frame250.png`.
- **Test**: golden test passes pixel-exact (already exists).
- **Add**: whatever the boot demands by this point. Likely:
  - 512 KiB slow RAM at `$C00000`.
  - Sprite DMA (the disk graphic uses sprites? or blitter? need to
    verify).
  - Blitter DMA (the boot uses blitter to compose the graphic).
  - CIA-A keyboard handler skeleton.
- **Don't add**: anything not exercised by this golden.

### M12 — Insert-disk screen (chip-only A500)

- **Goal**: Boot reaches insert-disk on chip-only A500.
- **Reference**: existing golden `a500-ks13-512k-chip-frame250.png`.
- **Test**: golden test passes.
- **Add**: whatever differs from M11. Critically: whatever mechanism
  prevents the COP2LC=ExecBase toxicity that broke the previous
  emulator. This is the milestone that *forces* us to identify the
  missing real-hardware behavior — we cannot pass this test
  without solving it.

### M13 — KS 1.2 chip-only (A1000)

- **Goal**: KS 1.2 reaches the V1.2 insert-disk screen.
- **Reference**: golden `a1000-ks12-512k-chip-frame250.png`.
- **Test**: golden test passes.

### M14 — A2000 boot

- **Goal**: A2000 reaches insert-disk with same KS as A500.
- **Reference**: capture FS-UAE A2000 golden (need to do).
- **Test**: golden test passes.
- **Add**: A2000-specific config (Zorro II expansion address decode,
  internal 512K board layout). Likely just a config flag in the OCS
  machine crate.

### M15 — CDTV boot to insert-disk

- **Goal**: CDTV (A500 + CD-ROM) reaches insert-disk on its own ROM.
- **Reference**: capture FS-UAE CDTV golden.
- **Test**: golden test passes.
- **Add**:
  - CDTV extension ROM mapping.
  - DMAC skeleton (autoconfig response only — CD-ROM not actually
    used until insert-disk equivalent).
  - CD-ROM peripheral skeleton (no media, just "device present").

End of Phase A.

## Phase B milestones (ECS)

Reuse Phase A's CPU + Paula + CIA + memory model. Replace Agnus and
Denise with ECS variants.

### MB1 — ECS Agnus skeleton

- **Goal**: ECS Agnus replaces OCS Agnus in a new machine crate.
  All Phase A behavior still works.
- **Reference**: Hardware Reference Manual Appendix C (ECS additions).
- **Test**: Run KS 2.04 boot to first BPLCON0 write (analogous to
  Phase A milestones). Verify register dispatch.
- **Add**: ECS Agnus crate with OCS-compatible behavior + new
  registers (DIWHIGH, BPLCON3, FMODE-equivalents).

### MB2 — A500+ insert-disk

- **Goal**: A500+ reaches insert-disk with KS 2.04.
- **Reference**: existing golden `a500+-ks204-1m-chip-frame250.png`.
- **Test**: golden passes.
- **Add**: 1M chip RAM via Fat Agnus, ECS Denise minimal, KS 2.04 ROM
  loading.

### MB3 — A600 insert-disk

- **Goal**: A600 reaches insert-disk with KS 2.05.
- **Reference**: existing golden `a600-ks205-1m-chip-frame250.png`
  (need kick205.rom).
- **Test**: golden passes.
- **Add**: Gayle skeleton (autoconfig response, no IDE/PCMCIA
  attached). KS 2.05 ROM.

End of Phase B.

## Phase C milestones (AGA + 020)

### MC1 — 68EC020 CPU

- **Goal**: 68EC020 passes Tom Harte tests for boot opcodes.
- **Reference**: 68EC020 manual + Tom Harte 65816/68k fixtures
  (extend with 020-specific opcodes).
- **Test**: Tom Harte fixture run for 020 opcodes used in KS 3.x boot.
- **Add**: New `motorola-68020` crate. The EC variant has no MMU
  (simpler than full 020).

### MC2 — AGA chipset skeleton

- Goal: Alice + Lisa replace ECS Agnus + Denise. Initial behavior
  matches ECS (no AGA-specific modes used yet).
- Reference: AGA reference docs.
- Test: KS 3.x boot reaches first BPLCON0 write.
- Add: Alice and Lisa crates with ECS-compatible default behavior.

### MC3 — A1200 KS3.0 insert-disk

- Goal: A1200 with KS 3.0 reaches insert-disk.
- Reference: existing golden `a1200-ks30-2m-chip-frame250.png` (need
  kick30.rom).
- Test: golden passes.
- Add: 2M chip RAM, AGA modes used by boot, Gayle, Budgie.

### MC4 — A1200 KS3.1 insert-disk

- Goal: A1200 with KS 3.1 reaches insert-disk.
- Reference: existing golden `a1200-ks31-2m-chip-frame250.png`.
- Test: golden passes.
- Add: KS 3.1 ROM (already have). Likely no chipset changes from MC3.

### MC5 — CD32 boot

- Goal: CD32 reaches its boot screen.
- Reference: capture FS-UAE CD32 golden.
- Test: golden passes.
- Add: Akiko skeleton (autoconfig + CD-ROM stub), CD32 extension
  ROM, no floppy.

End of Phase C.

## Phase D milestones (big-box machines)

Lowest priority — these are the largest jumps because of new CPUs and
specialised chips.

### MD1 — 68030 CPU

- Goal: 68030 passes Tom Harte for boot opcodes used by KS 2.04 / 3.1
  on A3000/A4000.
- Add: New `motorola-68030` crate, with MMU support stub (just enough
  for KS to detect).

### MD2 — A3000 insert-disk

- Goal: A3000 reaches insert-disk.
- Reference: capture FS-UAE A3000 golden.
- Test: golden passes.
- Add: Ramsey, Buster, DMAC SCSI skeleton, Zorro III address space.

### MD3 — A4000/030 insert-disk

- Goal: A4000 with 68030 reaches insert-disk.
- Reference: capture FS-UAE A4000 golden.
- Test: golden passes.
- Add: A4000 IDE skeleton, AGA-on-Buster integration.

### MD4 — A4000/040 insert-disk

- Goal: A4000 with 68040.
- Add: 68040 CPU crate.

End of Phase D — every variant in the matrix is booting to insert-disk.

## Stage 2 — Workbench boot per variant

After each variant's Stage-1 insert-disk milestone passes, the same
variant gets a Stage-2 Workbench milestone. The Stage-2 milestone
adds: disk I/O subsystem (whatever the variant boots from), functional
trackdisk/scsi/cdrom device, and a longer boot run (typically
~1500 frames = 30s of emulated time) ending at the Workbench-loaded
screen.

### Boot media per variant (canonical configurations)

| Variant | Floppy | HDD | CD | Typical first boot |
|---|---|---|---|---|
| A1000 | yes | (rare sidecar) | — | Floppy |
| A500 | yes | (A590 sidecar, rare) | — | Floppy |
| A500 + slow | yes | — | — | Floppy |
| A2000 | yes | yes (A2091 SCSI / GVP HC8) | — | Either — test both |
| A500+ | yes | — | — | Floppy |
| A600 | yes | yes (Gayle IDE 2.5") | — | HDD if installed, else floppy |
| A1200 | yes | yes (Gayle IDE 2.5") | — | HDD (typical) |
| A3000 | yes | yes (DMAC SCSI internal) | — | HDD (typical) |
| A4000 | yes | yes (Buster IDE / SCSI variants) | — | HDD (typical) |
| CDTV | (optional) | — | yes (built-in) | CD |
| CD32 | (rare add-on) | — | yes (built-in) | CD (autoboot) |

**HDD-bootable variants:** A2000, A600, A1200, A3000, A4000. Each
gets a Stage-2 milestone for HDD boot in addition to (or instead of)
floppy boot. For canonical-config variants (A1200, A3000, A4000),
HDD boot is the PRIMARY Workbench target since that's how they were
actually used. Floppy boot remains a secondary test.

### Disk subsystems (built once, reused per variant)

| Subsystem | Used by | Built in phase | Image format |
|---|---|---|---|
| Floppy MFM read (DSKBLK / DSKSYNC) | A1000, A500, A2000, A500+, A600 floppy boot | Phase A | ADF |
| IDE (Gayle) | A600, A1200 | Phase B | HDF + RDB |
| IDE (Buster) | A4000 | Phase D | HDF + RDB |
| SCSI (A2091) | A2000 | Phase A late | HDF + RDB |
| SCSI (DMAC) | A3000 | Phase D | HDF + RDB |
| CD-ROM (DMAC) | CDTV | Phase A | ISO / CUE |
| CD-ROM (Akiko) | CD32 | Phase C | ISO / CUE |

For HDD-bootable variants we need:
- HDF (Hard Disk File) image loader.
- RDB (Rigid Disk Block) parser — Amiga partition table format.
- FFS / OFS read support — handled by the ROM (we only provide block
  reads via the controller; the filesystem code lives in
  Kickstart/Workbench).

### Stage-2 milestones (one per variant per boot medium)

Each follows the same pattern: `<variant>-workbench-<medium>-<N>frames.png`
golden test. Frame count depends on real Workbench load time
(typically 1500-3000 frames = 30-60s emulated). Capture FS-UAE
Workbench-loaded goldens for each.

Workbench versions per variant (user has them):

| Variant | Workbench version | Boot medium(s) |
|---|---|---|
| A1000 / KS1.2 | Workbench 1.2 | Floppy |
| A500 / KS1.3 | Workbench 1.3 | Floppy |
| A2000 | Workbench 1.3 (or 2.04 late) | Floppy + SCSI HDD |
| CDTV | CDTV bootable CD (WB 1.3 base) | CD |
| A500+ / KS2.04 | Workbench 2.04 | Floppy |
| A600 / KS2.05 | Workbench 2.04 / 2.1 | Floppy + IDE HDD |
| A3000 | Workbench 2.04 / 3.1 | Floppy + SCSI HDD (HDD primary) |
| A1200 / KS3.0 | Workbench 3.0 | Floppy + IDE HDD (HDD primary) |
| A1200 / KS3.1 | Workbench 3.1 | Floppy + IDE HDD (HDD primary) |
| A4000 | Workbench 3.0 / 3.1 | Floppy + IDE HDD (HDD primary) |
| CD32 | CD32 boot CD (autoboot) | CD |

For variants with multiple boot media, HDD boot is treated as the
canonical milestone for AGA-era machines (A1200, A3000, A4000) and
as an additional milestone for A600 and A2000.

### Disk image acquisition

Place images in `~/.emu198x/disks/commodore-amiga/` with conventional
names. User has source media.

| Image type | Examples | Used by |
|---|---|---|
| `.adf` (floppy) | `workbench-1.3.adf`, `workbench-3.1.adf` | All floppy boots |
| `.hdf` (hard disk) | `workbench-3.1-installed.hdf`, `workbench-2.1.hdf` | A2000/A600/A1200/A3000/A4000 HDD boot |
| `.iso` / `.cue+.bin` (CD) | `cdtv-welcome.iso`, `cd32-boot.iso` | CDTV, CD32 |

### Why Stage 2 matters even though insert-disk works

Insert-disk tests cover ~5% of the boot ROM code path. Workbench-loaded
tests cover the FULL boot: graphics.library, intuition.library,
dos.library, layers.library, console.device, trackdisk.device,
filesystem, the boot loader, the strap module's job, the
ScreenManager. If any of those subsystems has a latent bug, Workbench
won't render — making it a far stronger correctness signal.

End of Stage 2 — every variant boots to its Workbench screen.

## Verification — reference data sources

In rough order of trust:

1. **Real KS 1.3 ROM** (`~/.emu198x/roms/commodore-amiga/kick13.rom`) —
   ground truth for what the CPU executes.
2. **FS-UAE golden frames** — ground truth for visible state at
   specific configurations.
3. **WinUAE / FS-UAE / vAmiga source code** (in
   `~/Projects/Emu198x-Unclean/Reference/`) — chipset behavior
   reference.
4. **Hardware Reference Manual** (PDFs at `~/Desktop/AmigaPDFs/txt/`)
   — register semantics.
5. **V37 boot trace document**
   (`amiga-rom-boot-traces.md`) — boot phases, register write sequence.
   V34 same shape.
6. **The Guru Book** — chipset edge cases.

## What we explicitly defer

For Stage 1 (insert-disk):

- **Audio (Paula audio channels)** — not exercised by boot ROM.
- **Sprites beyond what the boot screen uses** — only the sprites
  the strap module touches.
- **Floppy MFM streaming** — Stage 2 only. Stage 1 just needs "no
  disk inserted" detection.
- **Serial / parallel / printer** — defer indefinitely.
- **Joystick / mouse** — Stage 2 only (Workbench needs mouse).
- **HAM, EHB** — defer until something demands them (likely Stage 2
  or beyond, possibly never for boot tests).
- **Multi-Workbench / multi-screen** — far future.

For Stage 2 (Workbench boot):

- **Floppy WRITE** — read-only is enough for boot tests.
- **HDD WRITE** — read-only.
- **Network** — defer indefinitely.
- **CD-ROM audio (DA)** — defer.
- **MMU** (68030/040 PMMU) — minimal stub, just enough for KS to
  detect.

## Testing protocol

- Every milestone: write the test FIRST, then implement until it
  passes.
- Test files live in `crates/machine-commodore-amiga/tests/` and are
  named `m<N>_<description>.rs`.
- Tests are NOT `#[ignore]` by default — they should run on every
  `cargo test` invocation. Slow tests (running > 1 second of emulated
  time) get `#[ignore]` only if absolutely necessary.
- Each milestone passes its own test AND every prior milestone's
  test. No regressions.
- Once a milestone passes, its test is locked in — future changes
  must not break it.

## Concrete next actions

1. **You approve / reject this plan.**
2. If approved: archive the chipset crates (`git mv` to add `-archive`
   suffix; update `Cargo.toml` workspace members; fix references).
3. Create skeleton `crates/machine-commodore-amiga` with M0 only.
4. Write M0 test: "After reset and N CCKs, CPU PC equals ROM
   reset-vector PC."
5. Implement M0 until test passes. Commit.
6. Iterate to M1.

## Risks

- **Time**: Stage 1 (all 12 variants to insert-disk): 9-16 weeks
  focused. Phase A (OCS, A500/A1000/A2000/CDTV): 3-5 weeks. Phase B
  (ECS): 1-2 weeks delta. Phase C (AGA + 020): 2-4 weeks. Phase D
  (big-box): 3-5 weeks. Stage 2 (all 12 to Workbench): another
  6-12 weeks for the disk subsystems and per-variant Workbench
  validation. Total realistic: 15-28 weeks for full coverage.
- **CPU bus arbitration**: the existing arbitration model required a
  bug fix this session. Re-implementing it from scratch may surface
  new issues. Mitigation: the existing arbitration model is in the
  archived code as reference; copy structure but re-validate.
- **Chip detection logic**: V34 has CPU-type detection that probes
  68010+ features. We currently emulate 68000 only. The detection
  must correctly identify "68000" — verify with M-pre-7 tests.
- **Golden re-validation**: the slow-RAM golden was generated against
  the CURRENT chipset. The new chipset may produce subtly different
  output that's still "correct" by FS-UAE standards but doesn't
  match the existing capture pixel-exact. If so, recapture from
  FS-UAE.

## What this plan does NOT solve

The chip-only KS 1.3 toxicity (the COP2LC=ExecBase problem) is a real
issue we hit at M12. The restart doesn't make it disappear — but it
DOES force us to confront it under known-correct foundations, rather
than chasing it through layers of pre-existing behavior.

## Related

- `wiki/decisions/amiga-chip-only-boot-failure.md` — the root cause we
  uncovered, what we won't get to skip.
- `wiki/decisions/amiga-architecture-review.md` — the architecture
  this restart honors.
- `wiki/processes/golden-image-capture.md` — how to capture new
  reference frames if needed.
- `RULES.md` — the hard constraints (master oscillator drives loop,
  ULA/CPU half-cycle, no catch-up, no Bus trait).
