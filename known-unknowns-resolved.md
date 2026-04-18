# Known Unknowns — Resolved Archive

Gaps that have been answered. Each entry retains its original section context and resolution source.

---

## Topics with thin coverage

### Cycle-accurate timing edges

- **Amiga 68000 ↔ chip-RAM contention** — *cycle-exact* timing under Agnus's bus arbitration. **RESOLVED:** per-colour-clock DMA slot table (refresh / disk / audio / sprite / bitplane / copper / blitter / CPU), the chip-bus arbiter, the nasty-blitter counter, and long-line/short-line LOL handling are all distilled from WinUAE source into `amiga-cycle-accurate.md` (1700+ lines), cross-validated by the Minimig-AGA Verilog references in `amiga-a500-hardware-reference.md` and `amiga-custom-chips-reference.md`. The Guru Book sections 4.3 and 6.5 (`amiga-guru-book-reference.md`) confirm at the prose level: Agnus *owns* the chip bus; CPU contends for any unused slot.
  _resolved in this distillation pass_

### Disk formats

- **Amiga MFM track layout** — **RESOLVED:** definitive per-byte track / sector / sync / DSKLEN reference now lives in `amiga-mfm-track-format.md`. ADF↔MFM conversion, sector header layout, EADF/UAE-1ADF, copy-protection (long tracks, fuzzy bits, custom syncs) and trackdisk command/buffer semantics are also covered in `amiga-devices-reference.md` §trackdisk.device. CAPS/IPF format spec from SPS is the only remaining hole and is only required for protection-preserving disk images; not on the boot-from-OFS critical path.
  _resolved in this distillation pass_

### Cartridge / expansion port formats

- **Amiga autoconfig** — **RESOLVED:** full Zorro II/III autoconfig probe sequence, ConfigDev / ExpansionRom struct layouts, and ROM-based driver (Autoboot) stages are now distilled in `amiga-libraries-overview.md` §expansion.library and cross-referenced in `amiga-memory-map-reference.md` §"Autoconfig — Zorro II / Zorro III probe space". RigidDiskBlock partition chain also in `amiga-libraries-overview.md`.
  _resolved in this distillation pass_

---

## Motorola 68000

### ~~Per-instruction cycle counts~~ — RESOLVED 2026-04-16

Cycle tables (User's Manual Tables 8-1 through 8-14) are now captured in
`cpu-68000-reference.md` under "Instruction Timing", including MUL/DIV
data-dependent formulas. 68008 (8-bit-bus, Section 7) and 68010 (Section 9)
tables are **not** distilled — low priority, Amiga is 68000.

A second sweep on 2026-04-16 cross-checked the remaining gaps against
emulator source (Musashi + vAmiga's Moira core). Results inline below.

_resolved on 2026-04-16_

### Illegal opcode behaviour — RESOLVED 2026-04-16

Resolution in `cpu-68000-reference.md` under
"Community-reverse-engineered behaviour → Illegal / Line-A / Line-F
opcode decoding":

- A-line = full range `$A000–$AFFF` → vector 10, no sub-decoding.
- F-line = full range `$F000–$FFFF` → vector 11 on bare 68000.
- ILLEGAL = every other non-valid opcode → vector 4, decoded by
  exhaustion (Moira fills all 65536 slots with ILLEGAL at init then
  overwrites valid ones; any slot that survives is illegal).
- Exception cost is **34 CLK** for all three on 68000 (Moira and Musashi
  agree).

_resolved on 2026-04-16_

### Bus arbitration slot timing — RESOLVED 2026-04-17

**RESOLVED:** Per-CCK chip-RAM slot table and CPU contention behaviour
now live in `amiga-cycle-accurate.md`. The arbiter logic
(`wait_cpu_cycle_read` / `_write`) is documented from WinUAE source
with Minimig Verilog cross-checks. Bank-type table (chip / fast / slow /
CIA / kickrom) and per-bank wait-state behaviour all captured.

_resolved on 2026-04-17_

### Bus / address error stack frame — RESOLVED 2026-04-16

14-byte "group 0" stack frame layout and access-type word bit meanings
(R/W, I/N, FC2:0) now captured from Musashi and Moira sources in
`cpu-68000-reference.md` under "Community-reverse-engineered behaviour
→ Bus/address error stack frame". Both cores agree on layout. Stacked PC
is "best effort within 2 words of fault" — neither core promises exact.

_resolved on 2026-04-16_

---

## Motorola 68010

The following inline gap entries from the 68010 "Gaps:" list are resolved.

- **Special Status Word bit layout (Format $8 frame, +$08)** — **RESOLVED**
  via Moira source (`MoiraTypes.h:440-451`, `MoiraDataflow_cpp.h:530-560`):
  bits 0–2 = FC, bit 8 = R/W, bit 12 = DF (data fetch in progress), bit 13 =
  IF (instruction fetch in progress); bits 3–7, 9–11, 14–15 reserved. Added
  to `cpu-68010-reference.md` Format $8 section and the new
  "Community-reverse-engineered behaviour" section.
  _resolved in this distillation pass_
- **16 words of internal microcode state at $1C–$3A in Format $8** —
  **RESOLVED-NEGATIVE** (Motorola deliberately does not document these).
  Both Musashi (`m68ki_stack_frame_1000`, `m68kcpu.h:1667-1714`) and Moira
  (`writeStackFrame1000`, `MoiraExceptions_cpp.h:103-149`) write zero for
  all 16 internal words and accept whatever is read back on RTE without
  re-running the bus cycle. Moira verifies the version nibble at `+$1A`
  (bits 10-13) is zero on RTE and raises Format Error otherwise
  (`MoiraExec_cpp.h:5026-5034`).
  _resolved in this distillation pass_
- **Exact cycle counts for MOVEC/MOVES/RTD** — **RESOLVED** from Musashi's
  opcode table (`m68k_in.c:700-722, 805-806`) and Moira's CYCLES macros.
  MOVEC Rc→Rn = 12, MOVEC Rn→Rc = 10, RTD = 16, RTE = 24 (Format $0),
  MOVES byte/word = 14 (+ EA), MOVES long = 16 (+ EA), MOVE from CCR Dn = 4,
  CLR.W mem = 4 (vs 8 on 68000), CLR.L mem = 6 (vs 12). Tabulated in the
  Community-RE section.
  _resolved in this distillation pass_
- **Loop mode entry/exit timing** — **RESOLVED** from Moira's `execDbcc`
  (`MoiraExec_cpp.h:2139-2309`). Entry requires a loopable instruction,
  DBcc with displacement exactly −4, and a taken branch. Exit conditions:
  DBcc condition true, counter exhausts, any interrupt (clears LOOPING in
  `execInterrupt`/`checkForIrq`, `MoiraExceptions_cpp.h:514`,
  `Moira.cpp:422-423`), any exception. In loop mode the prefetch is frozen
  (`looping<I>() ? noPrefetch<C>(N) : prefetch<C,POLL>();`); a 2-cycle
  `loopModeDelay` applies on exit. Confirmed the exact 33-instruction
  loopable set and the restricted addressing-mode mask (`(An)`, `(An)+`,
  `-(An)` only).
  _resolved in this distillation pass_
- **68010 errata** — **RESOLVED-NEGATIVE**. Neither Musashi nor Moira
  carries silicon-revision conditional code for 68010. Community consensus
  (comp.sys.m68k archives; WinUAE mailing list) reports no published mask
  revisions affect Amiga-relevant instructions. The only widely known
  "gotcha" is the documented `MOVE from SR` privilege change.
  _resolved in this distillation pass_

---

## Motorola 68881 / 68882

### FMOVECR constant ROM content — RESOLVED

- **Resolved from**: WinUAE `fpp.cpp` lines 164–231 gives a 22-entry defined
  table plus an 11-entry "undefined-field" table. Musashi `m68kfpu.c` lines
  1238–1350 gives a compatible subset. Defined CCC indices: `$00` (π),
  `$0B` (log₁₀2), `$0C` (e), `$0D` (log₂e), `$0E` (log₁₀e), `$0F` (0.0),
  `$30` (ln 2), `$31` (ln 10), `$32`..`$3F` (1e0 through 1e4096, powers of
  ten). All other `$01`..`$0A`, `$10`..`$2F`, and `$40`..`$7F` are
  undefined. WinUAE additionally encodes eleven specific bit-patterns that
  real silicon returns for the most commonly-probed undefined CCCs (with
  rounding-mode-dependent low-bit adjustments).
- **Still unknown (tier 2)**: the full 128 × (bit-pattern, FPSR side-effect)
  mapping is only captured for indices that WinUAE's authors exercised
  against real silicon. Indices `$40`–`$7F` are documented as "all return
  same value" but the precise value and whether it sets INEX2 vs. NAN is
  model-dependent (68881 vs 68882 differ in rounding-mode edge cases per
  WinUAE comments at line 218 "68881 and 68882 have identical undefined
  fields" — contradicts older lore).
- **Acceptance**: full table will be added to the reference document.

_resolved in this distillation pass_

### Exception priority ties — RESOLVED

- **Resolved from**: WinUAE `fpsr_check_arithmetic_exception` (fpp.cpp:440)
  and `fpsr_get_vector` (fpp.cpp:427). Behaviour is concrete:
  1. **All matching EXC bits are set simultaneously** in FPSR. The manual's
     "priority" applies only to *which trap vector is taken*, not to which
     bits show up in EXC.
  2. Vector selection scans from bit 15 (BSUN) down to bit 8 (INEX1) and
     returns the first set-and-enabled bit's vector. Table:
     `{INEX1=49, INEX2=49, DZ=50, UNFL=51, OPERR=52, OVFL=53, SNAN=54, BSUN=48}`.
     INEX1 and INEX2 share vector 49.
  3. AEXC always accumulates by the documented equations regardless of which
     vector was dispatched — matches manual Section 6.
- **Specific multi-exception cases** (confirmed in WinUAE):
  - SNAN+INEX1 → both bits set; vector 54 (SNAN highest enabled).
  - OPERR+INEX2 → both set; vector 52.
  - OVFL+INEX2 → both set; AEXC(OVFL) and AEXC(INEX) both set; vector 53.
  - UNFL+INEX2 → both set; AEXC(UNFL) set only if both bits set together
    (matches manual equation `AEXC(UNFL) = UNFL & INEX2`).
- **Still unknown (tier 2)**: whether real silicon differs from WinUAE on
  OPERR+INEX1 vs OPERR+INEX2 classification for packed-decimal operations.

_resolved in this distillation pass_

### Packed-decimal k-factor rounding — RESOLVED

- **Resolved from**: WinUAE `fpp_softfloat.cpp:676` (`fp_from_pack`) and
  softfloat `floatx80_to_floatdecimal` (`softfloat_decimal.cpp:380`). The
  k-factor is a 7-bit sign-extended field in the instruction:
  - Extracted: `kfactor &= 0x7F; if (kfactor & 0x40) kfactor |= ~0x3F;`
    → range −64..+63 (not −64..+17 as the manual implies).
  - `k > 17` is clamped to 17 **and OPERR is set** (`float_flag_invalid`).
    Confirms manual's "k > 17 produces OPERR".
  - `k > 0`: `len = k` (significant digits total).
  - `k ≤ 0`: `len = ilog + 1 − k`, clamped to [1, 17] (digits after point).
  - `k = 0` and `k = 1` differ by exactly one significant digit (len = 1
    vs len = ilog+1): k=0 means "one significant digit"; k=+1 means "one
    digit total" — these collapse for ilog=0 but diverge for |value| ≥ 10
    or |value| < 1.
  - `decExp > 999` raises OPERR and truncates the exponent to 3 BCD digits
    plus the "fourth digit" field (bit position in pack_exp4).
- **Acceptance**: boundary behaviour fully documented; specific (input, k,
  output) tables can be generated from the algorithm.

_resolved in this distillation pass_

---

## MOS 6502

The following inline gap entries from the MOS 6502 list are resolved.

- ~~**Cycle-by-cycle bus activity per opcode**~~ — **RESOLVED**. The 65x02
  `6502/v1/*.json` fixtures (at `~/Projects/Emu198x-Unclean/65x02/6502/v1/`)
  provide per-opcode cycle traces with bus-level reads/writes. Spot-checked
  for RMW (INC), JMP indirect page-wrap, and SHX — all match standard
  community formulas.
  _resolved on 2026-04-16_
- ~~**Precise page-cross penalty rules per opcode**~~ — **RESOLVED**. Mesen2
  distinguishes `AbsX` / `AbsXW` (read vs write/RMW) in its addressing-mode
  table; fceux has `GetABIRD` vs `GetABIWR` macros; nestopia encodes
  per-opcode write clocks in `writeClocks[256]`. All three agree: reads do
  the dummy only on page-cross, stores and RMW always do the dummy read.
  _resolved on 2026-04-16_
- ~~**Interrupt-polling timing and CLI/SEI/PLP delay-by-one**~~ — **RESOLVED**.
  Mesen2 models penultimate-cycle sampling explicitly (`_prevRunIrq = _runIrq`
  in `EndCpuCycle`); nestopia schedules via `irqClock = cycle + InterruptEdge()`.
  CLI/SEI/PLP delay-by-one is implemented via `_PI` caching (Mesen2, fceux) or
  `irqClock = cycles.count + 1` in `Plp` (nestopia). Mesen2 also models the
  taken-non-page-crossing-branch IRQ delay (`branch_delays_irq.nes`); nestopia
  and fceux do not. See reference for details.
  _resolved on 2026-04-16_
- ~~**NMI edge vs IRQ level triggering**~~ — **RESOLVED**. Mesen2's edge
  detector (`if(!_prevNmiFlag && _state.NmiFlag) _needNmi = true`) is the
  textbook implementation. Nestopia and fceux use simpler one-shot latches
  that rely on the caller to only assert on a real edge — equivalent for
  NES-era hardware. See reference.
  _resolved on 2026-04-16_
- ~~**JMP indirect $xxFF page-wrap bug**~~ — **RESOLVED**. Mesen2, fceux,
  nestopia all implement `PCH = Read((addr & $FF00) | ((addr+1) & $FF))`.
  Spot-checked 65x02 fixture `6c ff 70`: PCL from $70FF, PCH from $7000 →
  PC = $989D. All four sources agree.
  _resolved on 2026-04-16_
- ~~**Undocumented opcodes**~~ — **RESOLVED** for stable ops (NOPs, LAX,
  SAX, DCP, ISB/ISC, SLO, SRE, RLA, RRA, ANC, ASR, AXS, LAS): all three
  emulators agree with the community-canonical formulas, confirmed by
  65x02 fixture spot-checks. See reference "Unstable opcodes" block for
  the unstable subset.
  _resolved on 2026-04-16_
- ~~**Unstable illegal opcode behaviour (XAA, ANE, SHA, SHX, SHY, TAS, LAS)**~~
  — **MOSTLY RESOLVED**. Agreement between Mesen2, fceux, nestopia, and the
  65x02 fixtures on: ANE = `(A|$EE)&X&imm`; SHA/SHX/SHY = `R & (H+1)` with
  page-cross masking; TAS = SHA + `S=A&X`; LAS = `A=X=S = M&S`. **Disagreement**
  on LXA/ATX ($AB): all three NES emulators implement simple `A=X=imm`, but
  the 65x02 fixtures require `A = (A|$EE) & imm` (verified 200/200). For a
  stock-NMOS-6502 emulator (C64 / Apple II / Atari 8-bit), use the $EE form;
  for NES, the simpler form is empirically fine because $AB is essentially
  unused in NES ROMs.
  _resolved on 2026-04-16_
- ~~**Reset sequence detail**~~ — **RESOLVED**. All three emulators agree on
  SP=$FD after reset, I=1, with 7 cycles performing reads only. Nestopia's
  `Reset(on, hard)` is clearest: hard reset clears A/X/Y to zero; soft reset
  decrements SP by 3 and sets I. D is forced to 0 on 2A03.
  _resolved on 2026-04-16_
- ~~**Stack wraparound behaviour**~~ — **RESOLVED**. Mesen2: `SetSP(SP() - 1)`
  with 8-bit register silently wraps. Nestopia: `sp = (sp - 1) & 0xFF`
  explicit. All three agree: wrap silently, no trap, no warning.
  _resolved on 2026-04-16_

---

## Zilog Z80

The following inline gap entries from the Z80 list are resolved.

- ~~**WZ / MEMPTR internal register update rules per opcode.**~~
  **RESOLVED (2026-04-17)** via cross-validation against the pre-existing
  Unclean Z80 distillation, which sourced Gennady Slutskin's *MEMPTR,
  esoteric register of the Z80 CPU* (z80.info). The per-instruction table
  now lives in `cpu-z80-reference.md` §Emulator Implementation Notes
  item 3.
  _resolved on 2026-04-17_
- ~~**F5 / F3 derivation for arithmetic-adjacent ops.**~~
  **RESOLVED (2026-04-18)** via four-way cross-validation against FUSE
  (`fuse-1.7.0/z80/z80_ed.c`, `z80_macros.h`), z80cpp (`z80cpp/src/z80.cpp`),
  SpecIde (`SpecIde/source/src/Z80Ini.h`, `Z80Outi.h`, `Z80BitNPtrHl.h`)
  and ares (`ares/ares/component/processor/z80/instructions.cpp`). All
  four agree on the per-instruction rules. See `cpu-z80-reference.md`
  §Community-reverse-engineered behaviour for the full table.
  _resolved on 2026-04-18_
- ~~**BIT instruction flag semantics on memory operands.**~~
  **RESOLVED (2026-04-18)**. All four cores (FUSE, z80cpp, SpecIde, ares)
  agree: `BIT n,(HL)` sources F5/F3 from the **high byte of WZ**; `BIT
  n,(IX+d)` / `BIT n,(IY+d)` source from the high byte of `(IX/IY)+d`;
  S = (bit tested is bit 7 AND that bit is set); Z = NOT(bit); P/V = Z;
  H = 1, N = 0, C unaffected. See reference §Community-reverse-engineered
  behaviour.
  _resolved on 2026-04-18_
- ~~**Exact interrupt acceptance cycle count and what happens on each
  T-state during INTA.**~~ **RESOLVED (2026-04-18)**. FUSE adds exactly
  **7 T-states** to the normal 4T M1 for mode 0/1 INT acceptance (matches
  the Zilog spec of M1 extended by 2T + normal 5T memory-write sequence),
  and z80cpp calls `interruptHandlingTime(7)` for the same effect.
  Totals: Mode 0 ≈ 13T (RST form), Mode 1 = 13T, Mode 2 = 19T. Interrupts
  are checked only at **instruction boundaries**; DDCB/FDCB cannot be
  split mid-prefix (see next bullet).
  _resolved on 2026-04-18_
- ~~**Block-I/O flag formulas.**~~ **RESOLVED (2026-04-18)** via four-way
  cross-validation (FUSE + z80cpp + SpecIde + ares all agree):
  - `k = data + ((C ± 1) & 0xFF)` for INI/IND (`+1` for INI, `-1` for IND)
  - `k = data + L` for OUTI/OUTD (HL already inc/dec'd before the calc)
  - `H = C = (k > 0xFF)`; `N = bit 7 of data`; `P/V = parity((k & 7) XOR B)`
  - S/Z/F5/F3 from the decremented B
  - Repeating forms (INIR/INDR/OTIR/OTDR) apply the same per-iteration
    calculation; ares additionally tweaks P/V and H on repeat iterations
    based on B parity and the C flag from the single-step result — this
    last detail is present only in ares among the cores we compared, and
    matches Sean Young's description.
  See reference §Community-reverse-engineered behaviour.
  _resolved on 2026-04-18_
- ~~**NMOS vs CMOS (Z84C00) behavioural differences.**~~ **RESOLVED
  (2026-04-18)** for the three silicon-divergence cases that matter:
  - `OUT (C),0` outputs 0 on NMOS, 0xFF on CMOS (FUSE gates via
    `IS_CMOS`).
  - SCF/CCF F5/F3 differ: NMOS = `((last_Q ^ F) | A) & 0x28`; CMOS = `A
    & 0x28` unconditionally (FUSE selects via `IS_CMOS` for both SCF
    and CCF; z80cpp and SpecIde hard-code the NMOS form).
  - `LD A,I` / `LD A,R` P/V is cleared on NMOS if an INT is accepted
    during the instruction (FUSE tracks via `z80.iff2_read` flag and
    clears P/V when `IS_CMOS` is false). CMOS parts do not have this
    erratum.
  _resolved on 2026-04-18_
- ~~**DDCB/FDCB prefix chaining with interrupts.**~~ **RESOLVED
  (2026-04-18)**. Both FUSE (main loop dispatches one full opcode,
  including all prefixes, per iteration before re-sampling INT) and
  z80cpp (`prefixOpcode` latch; INT only accepted when no prefix is
  pending) agree: a pending INT that arrives mid-prefix is **deferred
  until the full prefixed instruction retires**. There is no "DD is
  discarded" behaviour on real silicon as documented by these cores —
  INT sampling simply waits.
  _resolved on 2026-04-18_
- ~~**Exact DD/FD prefix "chaining" rules.**~~ **RESOLVED (2026-04-18)**.
  FUSE and z80cpp implement the same mechanism by different means but
  identical observable behaviour: each DD/FD consumes **4T + 1 R-tick**;
  only the *last* DD/FD before a real HL-using opcode takes effect.
  FUSE's z80_ddfd.c `default:` branch backtracks PC/R by one and
  re-dispatches; z80cpp sets `prefixOpcode = DD/FD` and re-enters the
  main loop. `DD ED …` runs the ED subtable normally (ED ignores DD/FD);
  the DD is a wasted 4T + R-tick.
  _resolved on 2026-04-18_

---

## Amiga Exec

### Quantum length and time-slice granularity — **resolved (2026-04-16)**

- **Tick source**: the V36+ `timer.device` autodoc states that
  `UNIT_VBLANK` uses *either* the power-supply strobe (CIA-A's TOD
  clock, counted at 50 Hz PAL / 60 Hz NTSC) *or* the E-clock on
  machines without PS strobes. Kickstart's scheduler increments
  `ExecBase.Elapsed` off the same VBlank interrupt that drives
  intuition's IDCMP. Full detail in `amiga-devices-reference.md`
  §timer.device.
- **Quantum default**: `ExecBase.Quantum` at offset 288 (UWORD);
  default value is **4** VBlank ticks — i.e. 80 ms PAL / 67 ms NTSC.
  Set during ROM init and rarely changed at runtime. No official
  setter — application code writes directly. Confirmed in the
  ExecBase dump in `amiga-memory-map-reference.md`.
- **PAL/NTSC tracking**: offset 530 `VBlankFrequency` (UBYTE) and 531
  `PowerSupplyFrequency` (UBYTE) hold actual rates in Hz. Per
  `execbase.i` lines 118–126: *"These values replace the obsolete
  AFB_PAL and AFB_50HZ flags."* PAL → both read 50; NTSC → both read 60.
- **Kickstart versions**: same default (Quantum=4, tick=VBlank) across
  1.2, 1.3, 2.0, 3.1.

_resolved on 2026-04-16_

### Cool-capture / Cold-capture / Warm-capture — **resolved (2026-04-17)**

- **Offsets confirmed**: ExecBase +42 `ColdCapture`, +46 `CoolCapture`,
  +50 `WarmCapture` (NDK 3.9 `execbase.i` + vAmiga `OSDebuggerRead.cpp:107-109`).
- **Order / semantics** (from `execbase.i` comments and the
  `SumKickData` autodoc):
    - **Cold** — called by the ROM reset handler *before* any system
      initialisation, right after the 68000 reads SSP/PC from
      `$000000`. Memory state is whatever survived hardware reset.
      A debugger hooking here must be entirely self-contained.
    - **Cool** — called *after* basic init (ExecBase built, library
      list created) but *before* `AddTask` drops into multitasking.
      Low memory valid; Exec services partially available.
    - **Warm** — called after the library init chain completes and
      just before multitasking starts. Fully-built ExecBase, all
      resident modules initialised.
- **Warm-reset survival**: ExecBase and low memory are *not* cleared
  on warm reset. The `ChkBase = ExecBase XOR $FFFFFFFF` at
  ExecBase+38 lets ROM confirm ExecBase survived. `KickMemPtr` /
  `KickTagPtr` / `KickCheckSum` at offsets 546/550/554 let a resident
  driver add its own ROMTags to the init chain — re-allocated via
  `AllocAbs` *before* expansion memory is added, so these pointers
  must live in Chip-RAM or Ranger ($C00000–$D80000), per the
  `SumKickData` autodoc.
- **Resolved further (2026-04-17)**: `amiga-rom-boot-traces.md` now
  documents the V37 reset trace, including ExecBase validation at
  `$F80182-$F801C6` and capture-vector handling. Per Guru Book §1.6
  the autodoc's claim that `SumKickData` returns nothing is wrong —
  it does return a result (now noted in `amiga-guru-book-reference.md`
  §1.6).
- **Acceptance remaining**: cycle-accurate call order for Kickstart 1.3
  specifically (only V37/V40 are traced).

_resolved on 2026-04-17_

### AttnFlags values for 68030/040/060 and AGA — **resolved (2026-04-16)**

From NDK 3.9 `exec/execbase.i` (lines 172–192) and vAmiga's
`OSDebuggerTypes.h:91-102`:

| Bit | Name | Meaning |
|-----|------|---------|
| 0 | `AFB_68010` | 68010 or better (stays set on 68020/030/040) |
| 1 | `AFB_68020` | 68020 or better (stays set on 68030/040) |
| 2 | `AFB_68030` | 68030 or better (stays set on 68040) |
| 3 | `AFB_68040` | 68040 |
| 4 | `AFB_68881` | 68881 FPU (stays set on 68882) |
| 5 | `AFB_68882` | 68882 FPU |
| 6 | `AFB_FPU40` | Integrated 68040 FPU present and working |
| 7 | — | Reserved |
| 8 | — | **Was `AFB_PAL` — now reserved** (superseded by `VBlankFrequency`) |
| 9 | — | **Was `AFB_50HZ` — now reserved** (superseded by `PowerSupplyFrequency`) |
| 10–14 | — | Reserved |
| 15 | `AFB_PRIVATE` | Exec-internal use |

- **68060 is not in AttnFlags.** Third-party 060 boards (phase5) expose
  CPU type via `68060.library` and a separate probe. Code needing 060
  detection must use `SuperState` + `MOVEC PCR,Dx` in a guarded block.
- **AGA is not in AttnFlags either.** AGA detection is via
  `gfx.library/GfxBase->ChipRevBits0 & GFXF_AA_*`, or a `VPOSR` read
  that returns an Alice/Lisa ID rather than Agnus/Denise. AGA chip
  IDs and detection are now distilled in `amiga-aga-and-chip-revisions.md`.
- **`AFB_FPU40` caveat** (from `execbase.i` lines 183–188): set only
  when the integrated 68040 FPU is working. If `AFB_68040` is set but
  none of `AFB_68881`/`AFB_68882`/`AFB_FPU40` is set, the 040 FPU
  math-emulation library has not loaded and only native 68040 FPU
  instructions are usable.

_resolved on 2026-04-16_

### Line-F / Line-A as library patch points — **resolved (2026-04-16)**

From FS-UAE `newcpu.cpp:3793-3893` (WinUAE-derived):

| Opcode range | 68000 vector | Emulator behaviour |
|--------------|--------------|--------------------|
| `$A000-$AFFF` in RTAREA | — | FS-UAE trap into host function (`m68k_handle_trap(opcode & 0xFFF)`) — this is how ersatz `AllocMem`, `DoIO`, etc. are implemented |
| `$A000-$AFFF` elsewhere | 10 (Line-A, `$28`) | Normal A-line exception. Kickstart ROM does not use line-A for its own libraries |
| `$F000-$FFFF` | 11 (Line-F, `$2C`) | Normal F-line exception. 040+ cores use `$F0xx` for cpu-internal ops. ROM libraries do not trap here; third-party patches (ixemul, MUI classes, 68040.library) install their own F-line handler via the `$2C` vector |
| `$4AFC` / `$FC4A` | 4 (Illegal, `$10`) | Canonical `ILLEGAL` opcode |
| `$FF0D` in RTAREA | — | FS-UAE "user-mode STOP" marker — the idle task uses this to halt the CPU without being in supervisor mode (see ChangeLog 960913) |
| `$4E7B` in ROM | — | `MOVEC` control-register variant — FS-UAE uses this to detect a Kickstart that expects 68020+ on a 68000 emulation |
| `$7100`-range | — | Cloanto encrypted-ROM marker (`MOVEQ`-shaped); active only when `cloanto_rom` flag is set |
| `$4848-$484F` (020+) | — | `BKPT #n` — some boards hang when no BKPT ack cycle occurs (FS-UAE models this via `cs_bkpthang`) |

- **Key takeaway**: ROM-era Kickstart (1.2–3.1) does *not* use Line-A
  or Line-F as library patch points in its own code. These opcode
  ranges matter to emulator infrastructure (RTAREA hooks) and to
  third-party V39+ patches, not to the Exec library itself.

_resolved on 2026-04-16_

### Semaphore internals — **resolved (2026-04-16)**

From NDK 3.9 `exec/semaphores.i`:

```
struct SignalSemaphore {          /* LN_SIZE = 14 */
    struct Node ss_Link;          /*   0 */
    WORD        ss_NestCount;     /*  14 */
    struct List ss_WaitQueue;     /*  16 : 14 bytes (LH_SIZE) */
    struct SemaphoreRequest       /*  30 */
                ss_MultipleLink;  /*     (single-node SSR)    */
    APTR        ss_Owner;         /*  42 : task holding exclusive */
    WORD        ss_QueueCount;    /*  46 : -1 free, 0 excl, >0 shared */
};
/* SS_SIZE = 48 */

struct SemaphoreRequest {         /* waiter queue entry */
    struct MinNode ssr_Link;      /*   0 : 8 bytes (MLN_SIZE) */
    APTR           ssr_Waiter;    /*   8 : task to Signal, or NULL = shared waiter */
};
/* SSR_SIZE = 12 */
```

- **State machine** (from ObtainSemaphore / ObtainSemaphoreShared autodocs):
    - `ss_QueueCount == -1`: free
    - `ss_QueueCount == 0` + `ss_Owner != NULL` + `ss_NestCount > 0`:
      exclusive held; recursive obtains by same owner increment `ss_NestCount`
    - `ss_QueueCount > 0` + `ss_Owner == NULL`: shared held by
      `ss_QueueCount` readers
    - `ss_WaitQueue` non-empty: waiters queued. `ssr_Waiter = NULL` →
      shared wait; `ssr_Waiter = task` → exclusive wait
- **V36 → V39 behaviour change** (autodoc): pre-V39, calling
  `ObtainSemaphoreShared` when already holding the exclusive lock
  deadlocks. V39+ nests correctly.
- **V37 register preservation**: pre-V37 `ObtainSemaphore` could
  clobber A0; V37+ preserves all registers.

_resolved on 2026-04-16_

### `tc_TrapCode` stack frame and register convention — **resolved (2026-04-16)**

From the AllocTrap autodoc (NDK 3.9), now also confirmed by Guru Book
§2.17 (`amiga-guru-book-reference.md`):

- **Stack frame on entry to `tc_TrapCode`**:
    - `0(SP)` = exception vector number (32–47 for `TRAP #0..#15`; or
      the hardware vector for bus/address/illegal/zero-divide/CHK/
      TRAPV/privilege/trace/Line-A/Line-F)
    - `4(SP)` = 68000/68010/68020/68030/68040 CPU exception frame
      (format varies with CPU type + exception class)
- **Mode**: handler runs in **supervisor mode** with the stack frame
  on the SSP. The autodoc explicitly warns: *"You are not allowed to
  write to the exception table yourself. In fact, on some machines
  you will have trouble finding it — the VBR register may be used to
  remap its location"* — cementing the supervisor-mode contract.
- **Registers on entry**: all CPU registers contain the values the
  faulting instruction left them in. Handler must PUSH what it wants
  to preserve (interrupt-style).
- **`tc_TrapData` is unused** by the default dispatcher (per
  AllocTrap autodoc).
- **Scope**: `AllocTrap` reserves only `TRAP #0..#15` (vectors 32–47).
  The other exception classes (bus error, address error, illegal,
  zero divide, CHK, TRAPV, privilege, trace, Line-A, Line-F) always
  route to `tc_TrapCode` for the current task, unconditionally — no
  allocation needed.
- **Exit**: handler returns via `RTE` or longjmp-style jump to
  task-specific recovery code (setjmp/longjmp equivalents in
  `amiga.lib`).

_resolved on 2026-04-16_

---

## Amiga boot sequence

### Custom-chip init register ordering at Phase 4

- **Resolved (2026-04-16, emulator source)**: fs-uae
  `custom.cpp:custom_reset()` (line ~6881) and `audio_reset()`
  confirm DMACON, INTENA, INTREQ, ADKCON, COPCON, BPLCON0, COP1LC,
  FMODE, DSKLEN all come up at `$0000` on hard reset. Kickstart's
  `MOVE.W #$7FFF,…` writes to the W1C registers are therefore
  **belt-and-braces** on cold boot — they only matter on warm reboot
  where the registers survive. No emulation-visible ordering
  constraint between the four `$7FFF` writes themselves; the only
  load-bearing order is that graphics.library writes COP1LC/COPJMP1
  and then sets DMACON `$8380` (Copper + BPL + master) during Phase 8.
  See the "Cross-checks from emulator source" section of
  `by-system/commodore-amiga/amiga-boot-sequence.md`.
- **Resolved further (2026-04-17)**: `amiga-register-reset-states.md`
  (now in the library) gives per-register reset values from WinUAE +
  vAmiga, and Guru Book §2.12 (in `amiga-guru-book-reference.md`)
  confirms that **custom-chip register state survives soft RESET** —
  early-boot colour-flash from leftover COLOR00 is cosmetic, not a bug.
- **Resolved further (2026-04-17)**: ROM-side init disassembly is now
  in `amiga-rom-boot-traces.md` Phase 3 (`$F8010C-$F80148`). Exact
  ROM offsets for Kickstart 2.04 / 3.1 single-stepping. Kickstart 1.3
  ROM offsets remain undisassembled.

_resolved on 2026-04-16 / 2026-04-17_

### Bootblock entry register convention

- **Resolved (2026-04-16, emulator source)**: fs-uae
  `disk.cpp:270-283` ships the exact 49-byte OFS bootblock the
  emulator synthesises. Disassembly confirms:
  - A6 = ExecBase (bootcode does `JSR -$60(A6)` = OldOpenLibrary
    immediately at offset `$0010`)
  - D0 = 0 on entry (success flag; bootcode returns non-zero to abort)
  - A1 = → trackdisk IOStdReq (the sample doesn't touch A1, so the
    contract is compatible but not demonstrated by this sample)
  - FFS variant (line 276–283) also calls `JSR -$228(A6)` against
    expansion.library — so FFS bootblocks require Kickstart 1.3+
  Full disassembly lives in the boot-sequence doc's new
  "Cross-checks from emulator source" section.
- **Cross-validated (2026-04-17)**: Guru Book §5.5 (in
  `amiga-guru-book-reference.md`) gives the boot block layout and
  documents the variants. OFS and FFS bootblocks confirmed.

_resolved on 2026-04-16 / 2026-04-17_

---

## Amiga A500

### Cycle-exact DMA slot allocation — RESOLVED

**Resolution**: Minimig-AGA Verilog gives the gate-level truth. Priority
cascade in `rtl/agnus.v` lines 135–224: disk → refresh → audio → bitplane
→ sprite → copper → blitter → CPU. Per-CCK slot layout in
`rtl/agnus_refresh.v` (hpos 0x09/0x0B/0x0D/0x0F), `rtl/agnus_diskdma.v`
(0x08/0x0A/0x0C + 0x04/0x06 when "disk fast" bit set), `rtl/agnus_audiodma.v`
(channels 0/1/2/3 at hpos 0x0E/0x10/0x12/0x14). Blitter-nasty arbitration
in `agnus.v` line 404 (BLS counter with `BLS_CNT_MAX = 3`) and WinUAE
`custom.cpp::dma_cycle()` line 12551 (*"4 consecutive busy cycles grant
CPU next slot in nice mode; never in nasty mode"*).

See `amiga-a500-hardware-reference.md` §"Per-CCK slot table" and
§"Blitter-nasty arbitration"; full distillation now also in
`amiga-cycle-accurate.md`.

_resolved in this distillation pass_

### Per-revision Agnus errata — RESOLVED (negative result)

**Resolution**: WinUAE exposes Agnus revision only through `VPOSR` bits
8–14 (preference `cs_agnusrev` in `custom.cpp:2531`) and does *not*
branch on it for any observable behaviour. The "rev-5 Fatter Agnus with
2 MB chip" referenced in some literature is not a real chip
(`custom.cpp` line 2538 explicitly notes *"apparently '8372 (Fat-hr)
(agnushr),rev 5' does not exist"*). The observable deltas that exist
(chip-RAM ceiling, addition of `DIWHIGH`/`BEAMCON0`/LOL on 8372A) are
covered by the coarse `ecs_agnus` flag.

The allegedly-different 8370 sprite-DMA-at-line-0 and blitter-line-draw
octant corner cases are **not modelled** in WinUAE or vAmiga, which
strongly suggests they have no practical effect. If you ship one Agnus
semantics, make it 8372A (WinUAE's default for A500 configurations).

See `amiga-a500-hardware-reference.md` §"Agnus silicon deltas"; AGA-side
chip revisions now in `amiga-aga-and-chip-revisions.md`.

_resolved in this distillation pass_

### 8373 OCS-strap register matrix — RESOLVED

**Resolution**: Minimig-AGA `rtl/denise.v` line 134 encodes the strap as
a single flag: `ecsena = bplcon0[0]`. ECS features only active when
BPLCON0 bit 0 is set; Kickstart 1.3 leaves it clear, so a Rev 6A/7 A500
with 8373 behaves identically to an 8362 for 1.3-era software.
DENISEID readout differs (`$FFFC` for 8373, open-bus for 8362), which
is what SetPatch uses for detection.

Full bit-level table (BPLCON3 bits 0, 1, 2, 4, 5) in
`amiga-a500-hardware-reference.md` §"Denise silicon deltas and the 8373
OCS strap".

_resolved in this distillation pass_

### Exact Gary address decode for A500 — RESOLVED

**Resolution**: Minimig-AGA `rtl/gary.v` is the gate-level truth for
A500 Gary (5719 R2); WinUAE `memory.cpp::memory_init_cs()` line 3152+
agrees at the CPU-side bank-mapping level. The A500 Gary differs from
the published A2000 equations only in three places:
1. No $D80000 over-decode into custom-register space (A2000 `/RGAE` bug
   not present).
2. Adds A501 trapdoor decode at $C00000 (A2000 has no slow RAM).
3. ROM mirror layout ($F80000 + $FC0000 as one 256 KB ROM, vs A2000's
   optional 512 KB spanning the full region).

Full decode truth table in `amiga-a500-hardware-reference.md`
§"Gary A500 address decode — derived from Minimig rtl/gary.v".

_resolved in this distillation pass_

### Kickstart ROM-image checksum — RESOLVED

**Resolution**: WinUAE `rommgr.cpp` has both validate and fix-up:
`kickstart_checksum_do()` at line 1327 and `kickstart_fix_checksum()` at
line 2417. End-around-carry 1's-complement 32-bit sum over big-endian
words; valid image sums to `0xFFFFFFFF`. Checksum slot at offset
`0x3FFE8` (256 KB) or `0x7FFE8` (512 KB). First-byte magic:
`0x11 0x14` (512 KB) or `0x11 0x11` (256 KB). Cross-validated by Guru
Book §1.5: **additive-carry** (not two's complement) — confirmed in
`amiga-guru-book-reference.md`.

Note: this is the **ROM-image** checksum Kickstart uses at boot to
sanity-check the image — distinct from the ExecBase capture-vector
checksums (which have their own algorithm documented in the startup
routine and now in `amiga-kickstart-rom-internals.md`).

See `amiga-a500-hardware-reference.md` §"Kickstart ROM-image checksum".

_resolved in this distillation pass_

### Audio filter exact values — RESOLVED

**Resolution**: vAmiga `Core/Components/Paula/Audio/AudioFilter.cpp`
gives the exact three-filter pipeline with component values:

| Filter | R / C | Cutoff |
|--------|-------|--------|
| DC-blocking HPF (always on) | R = 1390 Ω, C = 22.33 nF | 5.13 Hz |
| LED-gated Sallen-Key LPF | R1 = R2 = 10 kΩ, C1 = 6.8 nF, C2 = 3.9 nF | 3.07 kHz, Q ≈ 0.74 |
| Fixed anti-aliasing LPF | R = 360 Ω, C = 0.1 µF | 4.42 kHz |

The "3.3 kHz vs 5 kHz" literature confusion is resolved: 3.07 kHz is
the LED-on Sallen-Key, 4.42 kHz is the fixed post-LED one-pole. Both
operate on summed stereo pairs, not per-channel. The DC-blocking HPF
is always in circuit on an A500. Cross-validated by Guru Book §4.6
(7 kHz cut-off filter prose) in `amiga-guru-book-reference.md`.

See `amiga-a500-hardware-reference.md` §"Audio filter pipeline" and
the full state-machine + filter coefficient tables in
`amiga-paula-audio-model.md`.

_resolved in this distillation pass_

### A500 board-revision ECO deltas — RESOLVED (negative result)

**Resolution**: audit of 870152 / 870207 / 870222 / 870302 / 880238
(Rev 5) and 880283 (Rev 6A/7) against WinUAE and vAmiga source turns up
no branch-on-ECO behaviour anywhere. All ECOs are either
FCC-compliance, power-transient, or E-clock termination tweaks — the
last of which is absorbed by any emulator that already synchronises CIA
access to E. Emulators should ignore ECO revisions; the only
emulation-relevant split is Rev 3/5 (OCS 8362/8370) vs Rev 6A/7 (8372A/8373
strapped OCS).

See `amiga-a500-hardware-reference.md` §"Per-ECO emulation impact".

_resolved in this distillation pass_

---

## Amiga custom chips

The following inline gap entries from the "Resolved from emulator source" list are resolved.

- **Per-slot DMA cycle allocation**: RESOLVED. Odd HPOS for OCS —
  refresh at $01; disk at $07 $09 $0B; audio at $0D $0F $11 $13;
  sprites at $15 through $33 inclusive (16 slots, 2 per sprite).
  vAmiga `SequencerDas.cpp:26-58` and Minimig
  `agnus_diskdma.v`, `agnus_audiodma.v`, `agnus_spritedma.v` agree.
  Full table also in `amiga-cycle-accurate.md` §2.
  _resolved in this distillation pass_
- **Copper HP-phase alignment**: RESOLVED. Copper uses **even HPOS
  only**; on odd cycles it reschedules (vAmiga
  `CopperEvents.cpp:38-39, 52-53, 82-83`). Minimig confirms
  (`agnus_copper.v:131`, comment line 338: "copper only uses even
  cycles: hpos[1:0]==2'b01"). See also `amiga-cycle-accurate.md` §6.
  _resolved in this distillation pass_
- **Blitter BLIT-interrupt timing**: RESOLVED. Fires **one DMA cycle**
  after the last destination write, via the `BLTDONE` pipeline tag
  (vAmiga `SlowBlitter.cpp:1085-1090` —
  `scheduleIrqRel(IrqSource::BLIT, DMA_CYCLES(1))`). Earlier reference
  note saying "two cycles" was inaccurate and has been corrected in
  `amiga-custom-chips-reference.md` note #6.
  _resolved in this distillation pass_
- **DENISEID values**: RESOLVED — and the earlier reference was
  **backwards**. OCS returns $FFFF (no register; reads float as bus),
  ECS returns $FFFC, AGA Alice returns $00F8 (or $FCF8 with A4000
  IDE). vAmiga `DeniseRegs.cpp:107` and WinUAE `custom.cpp:2336-2357`
  agree. Register map row corrected. Note: Mapping the Amiga *omits*
  DENISEID entirely, since it predates widespread chip-ID; see new
  cross-validation gap below.
  _resolved in this distillation pass_
- **Fat Agnus BBUSY cutoff**: RESOLVED as a *model* distinction, not
  a production-run serial cutoff. A1000 Agnus sets BBUSY only on
  first DMA cycle (WinUAE `custom.cpp:2377-2382` guarded by the
  `agnusa1000` flag); Fat Agnus and later set it immediately on
  BLTSIZE write (vAmiga `Blitter.cpp:491`, no revision branch).
  Neither emulator encodes a within-8372A revision transition.
  _resolved in this distillation pass_
- **EHB on early A1000**: RESOLVED — and the attribution was **wrong**.
  EHB absence is a **Denise** property (palette right-shift lives in
  Denise), not Agnus. WinUAE gates EHB via `denisea1000_noehb`
  (`custom.cpp:1226`, `drawing.cpp:3184`), triggered by
  `CSMASK_A1000_NOEHB` / `DENISEMODEL_VELVET` / `DENISEMODEL_A1000NOEHB`.
  vAmiga does not model this case. Reference note #11 updated.
  _resolved in this distillation pass_
- **POT discharge timing**: RESOLVED. Both vAmiga (`PaulaRegs.cpp:176`)
  and WinUAE (`inputdevice.cpp:485`, `POTDAT_DELAY = 8`) use
  **8 horizontal lines** of discharge before counter advance —
  ~508 µs (NTSC) / 512 µs (PAL). The community "~320 µs" figure was
  an underestimate: it's *8 lines*, not a fixed microsecond value.
  Cross-referenced from `amiga-resources.md` §potgo.resource.
  _resolved in this distillation pass_
- **Interlace LOF auto-toggle edge**: RESOLVED. LOF toggles at
  **vpos=0**, after hsync housekeeping and before COPJMP1, gated by
  BPLCON0 LACE bit (WinUAE `custom.cpp:11434-11436`; vAmiga
  `Beam.cpp:40, 81, 273`). VPOSR reads return a beam position
  advanced by 5 HPOS cycles (vAmiga `AgnusRegs.cpp:239-250`) so games
  polling VPOSR around VB get a consistent answer.
  _resolved in this distillation pass_

---

## Amiga DOS / filesystem

- **RDSK (Rigid Disk Block) partition chain.** The `formats.md`
  teaching reference sketches the RDB layout but the full chain
  (PART blocks, FSHD blocks, BADB blocks, LSEG blocks for loadable
  filesystems) is not extracted. **RESOLVED:** full chain now
  documented in `amiga-libraries-overview.md` §expansion.library
  →RigidDiskBlock ("hardblocks").
  _resolved in this distillation pass_

---

## Amiga graphics.library

- **Exact LVO table — all 200+ entries across V33/V36/V39.** **RESOLVED:**
  full per-version LVO offsets are in `amiga-headers-reference.md`
  (NDK 3.9 verbatim, including `graphics_lib.fd`) and the boot-path
  subset is tabulated in `amiga-memory-map-reference.md` §"graphics.library
  LVOs (load-bearing subset)". Acceptance met: every public graphics
  function has its LVO.
  _resolved in this distillation pass_
