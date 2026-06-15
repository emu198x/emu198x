# Full MC68881/MC68882 FPU Implementation Plan

**Status:** Working plan
**Date:** 2026-06-15
**Issue:** #112 (Amiga 68881/2 FPU F-line dispatch + wiring)
**Branch:** `amiga-fpu-fline-112`

## Purpose

Take the 68881/68882 FPU from "the everyday subset works" to a **complete,
documented-behaviour-exact** implementation: every instruction, every data
format, the full exception model (report *and* trap), the state-frame
instructions, and the program-control instructions — validated against the
strongest oracle available for each piece.

This plan resolves the one question that decides how far "full" can go:
**what does bit-exact mean for the transcendentals?** (Section 2.)

## Decisions (resolved 2026-06-15)

Research into the vendored emulators settled the open decisions:

- **D1 — Transcendental reference → Andreas Grabher's softfloat FPSP**, as
  shipped in **WinUAE** (`emulators/amiga/WinUAE/softfloat/softfloat_fpsp.cpp`,
  also used by the Previous NeXT emulator). It implements all 19
  transcendentals + packed-decimal (`softfloat_decimal.cpp`) + the FSxxx
  precision handling, is built on SoftFloat 2a (the same family as our 2b
  port, on the exact `floatx80` primitives we already have), and was
  **validated against real 68881/68882/68040/68060 silicon** by Toni Wilen
  and Andreas Grabher. So "bit-exact" for transcendentals means
  **WinUAE/Previous-equivalence**, which *is* the hardware-validated gold
  standard. We do **not** port raw Motorola 68k FPSP assembly.
- **D2 — Licensing → settled.** Grabher's code is dual-licensed SoftFloat-2a
  + BSD, both GPL-2.0-or-later compatible (WinUAE is GPL and ships it; our
  existing `softfloat.rs` is the same posture). Port with attribution. No
  Motorola-FPSP entanglement.
- **D3 — Timing → inline with each phase.** Cycle counts land *with* each
  instruction group as it is implemented (cycle-accurate from the start),
  not retrofitted in a late phase. Phase 9 becomes the shared timing-model +
  cycle-table reference that each phase feeds, not a deferred catch-up.
- **D1/Phase 10 — Real-silicon validation → optional confirmation.** Faithful
  C-diff against WinUAE's softfloat inherits its hardware validation, so our
  own silicon/FPGA capture is a nice-to-have, kept low-priority.
- **D4 — 68881 vs 68882 → one result path + a model flag.** Results are
  identical; a `68881`/`68882` config flag selects the FSAVE frame size
  (Phase 5) and the timing/concurrency model (per-phase timing). No separate
  result path.

The consequence: the transcendentals (Phase 7), packed-decimal (Phase 4),
and FSxxx precision (Phase 2) all become **transliterations of Grabher's
softfloat onto our existing Rust `floatx80` port** — the same proven
"port + C-diff against the C reference" method used for `softfloat.c`.

## Source-of-truth hierarchy

1. This plan (for sequencing and scope).
2. **MC68881/MC68882 User's Manual** — `reference/by-topic/fpu-68881/mc68881um.txt`
   (1.4 MB, Docling-extracted). The primary datasheet: instruction semantics,
   FPCR/FPSR layout, exception model, packed-decimal, coprocessor protocol,
   state frames, accuracy bounds, cycle counts.
3. **Synthesis** — `syntheses/commodore-amiga/amiga-fpu-68881-reference.md`
   (distilled, cited to the UM by section).
4. **Reference algorithm for transcendentals** — the **Motorola 68040 FPSP**
   (Floating-Point Software Package), to be vendored (Section 3.3). The UM
   specifies the *accuracy* of the transcendentals but not the ROM polynomial
   coefficients; the FPSP is Motorola's published, bit-reproducible reference.
5. M68000 Programmer's Reference Manual (instruction encodings).

## 1. Current state (the baseline this plan builds on)

Done and validated on this branch (steps 1–5c):

- **Dispatch/decode:** F-line cpID/op-class routing, FPU-present gate.
- **Arithmetic (12 opmodes):** FMOVE, FABS, FNEG, FTST, FADD, FSUB, FMUL,
  FDIV, FSQRT, FCMP, FINT, FINTRZ — on a faithful Berkeley SoftFloat
  `floatx80` port (`crates/motorola-68k-common/src/softfloat.rs`).
- **Data formats:** Long/Single/Extended/Word/Double/Byte, load + store.
- **Addressing:** all four `begin_fp_memory` branches ((An), auto-inc/dec,
  static via `calc_ea_start`, immediate); FMOVECR; FMOVE FPcr↔ea.
- **Control flow:** FBcc.W/.L, FScc (predicates 0x00–0x1F), FMOVEM.
- **Exceptions:** FPSR EXC/AEXC *reporting* (step 5c), SNAN/OPERR split.
- **Validation infrastructure already in place:**
  - Musashi 68040 single-step corpus — 63 fixtures, 157,500 vectors, 100%
    (`crates/motorola-68020/tests/fpu_corpus.rs`).
  - SoftFloat C-diff vs the vendored `softfloat.c` — 11 ops × 200k vectors,
    value + exception flags (`crates/motorola-68k-common/validation/`).

## 2. The accuracy target — and the transcendental ceiling

"Full" means **documented-behaviour-exact**. For most of the FPU that equals
**bit-exact**, because the behaviour is fully specified:

- **Arithmetic** (FADD…FSCALE, FMOD/FREM, all conversions): IEEE-754 mandates
  ≤ ½ ULP round-to-nearest. SoftFloat already produces exactly this, and the
  68881 is IEEE-compliant here, so **SoftFloat output == 68881 output, bit for
  bit.** Achievable and already-oracled.

- **Transcendentals** (FSIN, FCOS, FTAN, FETOX, FLOGN, FATAN, …): **not
  IEEE-bounded.** The MC68881UM (§4.3) specifies only: worst case ~1 ULP
  double = **4096 ULP extended**; typical ~**64 ULP extended**. Two different
  conformant 68k FPUs (a 68881 vs the 68040 FPSP vs an FPGA AC68080) will
  legitimately differ in the low ~6 mantissa bits. There is **no single
  bit-exact answer** to chase.

  Therefore "full and correct" for transcendentals means: **port the Motorola
  68040 FPSP algorithm** (the published reference) and validate
  *bit-exactly against the FPSP*, while accepting that matching a *specific*
  silicon 68881 in the last bits is (a) not what the spec promises and (b)
  only verifiable with real-hardware capture. See Section 3.3 and Phase 7.

**Resolved (D1, 2026-06-15):** the target is **WinUAE/Previous-equivalence**
via Grabher's softfloat FPSP — which is itself silicon-validated, so this
gives hardware-grade accuracy reproducibly, without our own silicon capture.
See the Decisions section above.

## 3. Validation strategy (oracle per layer)

| Tier | Oracle | Covers | Bit-exact? |
|------|--------|--------|------------|
| A | Vendored `softfloat.c` (C-diff harness) | All IEEE arithmetic + conversions + exception flags | Yes |
| B | Musashi 68040 single-step corpus | Anything Musashi implements (note: its transcendentals are host `libm`, *not* a bit-exact oracle) | Yes for arith; no for transcendentals |
| C | **WinUAE softfloat** (Grabher's FPSP, already vendored) compiled standalone | Transcendentals, packed-decimal, FSxxx precision | Yes vs WinUAE (itself hardware-validated) |
| D | Real 68881/68882 silicon or FPGA (AC68080) vectors | Everything, gold standard | Yes vs that chip (optional, D1) |

### 3.1 Reuse the existing harnesses
The SoftFloat C-diff pattern (`validation/run.sh` + `examples/sf_gen.rs` +
`sf_check.c`) extends to every new `floatx80` routine. The Musashi corpus
(`m68k-test-gen --fp`) extends to every opmode Musashi implements.

### 3.2 What Musashi can and cannot oracle (already mapped)
Musashi-68040 `fpgen` implements: FMOVE, FINT/FINTRZ, FSQRT, FABS/FNEG,
FSIN/FCOS/FSINCOS (**libm**), FGETEXP, FDIV/FMOD, FSGLDIV, FADD/FMUL/FSGLMUL,
FREM, FSUB/FCMP/FTST. It `fatalerror`s on everything else (FSCALE, FGETMAN,
all other transcendentals, FDBcc/FTRAPcc, dynamic FMOVEM, packed). It never
sets the FPSR exception bytes.

### 3.3 The FPSP reference is already vendored (WinUAE)
No new vendoring or obtaining is needed. WinUAE
(`emulators/amiga/WinUAE/softfloat/`) ships Andreas Grabher's softfloat
extension — `softfloat_fpsp.cpp` (2409 LoC, 19 transcendentals),
`softfloat_decimal.cpp` (492 LoC, packed-decimal), `softfloat_fpsp_tables.h`
(528 LoC). It is built on the same `floatx80` primitives our port already has
(`floatx80_add`/`mul`/`sub`/`div`/`round_and_pack`/`propagateFloatx80NaN`), so
each function transliterates onto our Rust port. To validate, compile WinUAE's
`softfloat/` standalone (the same way `validation/run.sh` compiles
`softfloat.c`) and C-diff our Rust output against it. SoftFloat-2a + BSD,
GPL-2.0-or-later compatible — attribution retained, same as the existing port.

## 4. Phases

Each phase is independently shippable, gated on its own oracle, and ends with
the relevant suite green. Effort is rough (S ≤ 1 day, M ≈ 2–4 days, L ≈ 1 week,
XL > 1 week). Phases 1–6 + 8 complete the **non-transcendental FPU**; Phase 7
is the transcendental body; Phases 9–10 are accuracy/timing polish.

### Phase 1 — Remaining non-transcendental arithmetic (M)
**Scope:** FMOD (0x21), FREM (0x25), FSCALE (0x26), FGETEXP (0x1E),
FGETMAN (0x1F), FSGLMUL (0x27), FSGLDIV (0x24).
**Approach:** FMOD/FREM via a `floatx80_rem` port (SoftFloat has it).
FSCALE/FGETEXP/FGETMAN are exponent/significand bit-manipulation (the old
f64 `fpu.rs` has reference logic). FSGLMUL/FSGLDIV = compute then round to
single precision (the rounding core already supports precision 32).
**Oracle:** Musashi for FMOD/FREM/FGETEXP/FSGLDIV/FSGLMUL; SoftFloat C-diff +
unit tests for FSCALE/FGETMAN (Musashi `fatalerror`s). Add to the FP corpus.
**Risk:** low. FREM quotient bits land in FPSR bits 23-16 (wire the Quotient
byte — currently unused).

### Phase 2 — FPCR rounding precision + FSxxx/FDxxx variants (M)
**Scope:** honour FPCR bits 7-6 (single/double/extended rounding) and the
opmode FSxxx/FDxxx prefix (currently *stripped and ignored* to match Musashi).
**Approach:** the rounding core (`round_and_pack_floatx80`) already takes a
precision argument and implements 32/64; thread the real precision from FPCR
(and the opmode prefix) instead of hard-coding 80. (Grabher's softfloat uses
exactly this `floatx80_rounding_precision` field, so it lines up with the
Phase 7 port.)
**Oracle:** SoftFloat C-diff (Musashi does not apply precision → would
diverge; the corpus must keep precision = extended). Unit tests per mode.
**Risk:** medium — this is a deliberate divergence *from* the Musashi corpus;
gate the corpus to extended precision and validate precision via C-diff only.
Update `knowledge/decisions/fpu-softfloat-port.md`.

### Phase 3 — Exception traps, FPIAR, BSUN, monadic SNAN (M)
**Scope:** turn exception *reporting* into exception *delivery*.
- FPCR exception-enable byte (bits 15-8): when an FPSR EXC bit is set *and*
  its enable bit is set, take the FP exception (vectors 48–55: BSUN=48,
  INEX=49, DZ=50, UNFL=51, OPERR=52, OVFL=53, SNAN=54, unimplemented=55).
- **FPIAR**: latch each FP instruction's address (exception handlers read it).
- **BSUN**: set on the IEEE-nonaware predicates (0x00–0x1F that aren't EQ/NE)
  when the NAN condition is set, in FBcc/FScc/FDBcc/FTRAPcc (UM §4.2.4).
- **Monadic SNAN**: FMOVE/FABS/FNEG/FTST set SNAN on a signalling-NaN operand
  (the documented gap from step 5c — they bypass SoftFloat).
**Oracle:** spec (UM) + unit tests; no external oracle (Musashi doesn't trap).
**Risk:** medium — the trap path interacts with the core exception machinery;
reuse the existing group-1/2 exception plumbing.

### Phase 4 — Packed-decimal format (FMOVE.P) (M)
**Scope:** format 3 load and store, including the k-factor (static and
dynamic) controlling output digit count (UM §6.4 / §4.x).
**Approach:** transliterate WinUAE's `softfloat_decimal.cpp` (Grabher's
96-bit packed-BCD ↔ floatx80 + k-factor) onto our port.
**Oracle:** WinUAE softfloat C-diff (same harness as the transcendentals);
spec + unit tests.
**Risk:** medium — BCD rounding and the k-factor edge cases are fiddly.

### Phase 5 — FSAVE / FRESTORE state frames (M)
**Scope:** op-classes 4/5; save/restore the FPU internal state frame
(NULL / IDLE / BUSY formats). Privileged. **Per D4, the `68881`/`68882` model
flag selects the frame size** — the one place the results-identical chips
diverge.
**Approach:** model the minimal internal-state frame the UM defines; a
non-exceptional FPU saves a NULL or IDLE frame. Needed for AmigaOS
FPU-aware task switching.
**Oracle:** spec + unit tests; cross-check frame format against the UM and a
real Amiga context-switch trace if available.
**Risk:** medium — must match the exact frame byte layout the OS expects.

### Phase 6 — FDBcc / FTRAPcc (S)
**Scope:** the decrement-branch and trap-on-condition forms (op-class 1,
EA mode 1 / mode 7 regs 2-4).
**Approach:** reuse the already-validated predicate (`fpu::test_condition`)
plus the integer DBcc/TRAPcc mechanics; add BSUN per Phase 3.
**Oracle:** spec + unit tests (Musashi `fatalerror`s).
**Risk:** low.

### Phase 7 — Transcendentals via the FPSP (XL)
**Scope:** the ~18 functions: FSIN, FCOS, FTAN, FASIN, FACOS, FATAN, FSINH,
FCOSH, FTANH, FATANH, FETOX, FETOXM1, FTWOTOX, FTENTOX, FLOGN, FLOGNP1,
FLOG10, FLOG2, plus FSINCOS (0x30–0x37, dual-result).
**Approach (D1/D2 resolved):** transliterate **Grabher's
`softfloat_fpsp.cpp`** (the `floatx80_sin`/`cos`/`etox`/`logn`/… functions)
onto our Rust `floatx80` port + the `softfloat_fpsp_tables.h` constants. These
are argument-reduction + polynomial approximations built on the primitives we
already have. Sub-phase by family:
  - 7a. exp/log family (FETOX, FETOXM1, FTWOTOX, FTENTOX, FLOGN, FLOGNP1,
        FLOG10, FLOG2).
  - 7b. trig family (FSIN, FCOS, FTAN, FSINCOS).
  - 7c. inverse trig (FASIN, FACOS, FATAN).
  - 7d. hyperbolic (FSINH, FCOSH, FTANH, FATANH).
**Oracle:** compile WinUAE's `softfloat/` standalone and C-diff each function
(same harness as `softfloat.c`). Bit-exact vs WinUAE = hardware-grade accuracy
(WinUAE was silicon-validated). No longer blocked.
**Risk:** XL effort, but each function is self-contained and independently
validatable; the foundation (the floatx80 primitives) is already proven to
match the SoftFloat family by the existing C-diff (0 mismatches over 2.2M
vectors).

### Phase 8 — Pseudo-encodings & edge cases (S–M)
**Scope:** unnormal, pseudo-infinity, pseudo-NaN, pseudo-zero handling
(80-bit-specific invalid encodings the 68881 treats specially); denormalised
input normalisation paths.
**Oracle:** FPSP / spec + targeted vectors.
**Risk:** low individually, but easy to miss; drive from a UM-derived checklist.

### Phase 9 — FP cycle-timing model (cross-cutting; D3 = inline)
**Scope:** the shared timing infrastructure — the cpGEN stall model (the 020
stalls during a coprocessor op) + the MC68881UM cycle tables + the
68881/68882 timing flag (D4). **Per D3, the actual cycle counts land inline
with each phase** as its opcodes are implemented (cycle-accurate from the
start, the way #41 did integer timing); this phase is the shared model + the
reference tables + a coverage tracker, not a deferred catch-up.
**Oracle:** UM cycle tables; cross-check vs real-HW timing traces if available.
**Risk:** medium — observable but rarely depended on by software.

### Phase 10 — Real-hardware / FPGA validation pass (L, optional — D1)
**Scope:** capture vectors from a real 68881/68882 (or the AC68080 FPGA, per
the project's long-term scope) and diff the *entire* implementation — the
gold-standard accuracy check, especially for transcendentals where FPSP and
silicon legitimately differ.
**Oracle:** the silicon itself.
**Risk:** gated on hardware access; optional but definitive.

## 5. Sequencing & dependencies

```
Phase 1 (arith)  ─┐
Phase 2 (precision)─┼─ independent; do 1 first (finishes the SoftFloat-backed set)
Phase 6 (FDBcc)  ─┘
Phase 3 (traps/FPIAR/BSUN/SNAN) ── enables correct behaviour for 4,5,6,7
Phase 4 (packed) ── independent
Phase 5 (FSAVE)  ── independent (needs the FPU state model)
Phase 7 (transcendentals) ── needs D1/D2 (FPSP); largest; sub-phased 7a–7d
Phase 8 (pseudo-encodings) ── fold into 1–7 as encountered + a final sweep
Phase 9 (timing) ── after functional completeness
Phase 10 (silicon) ── hardware-gated, final
```

**Recommended order for "functionally complete, runs all software":**
1 → 6 → 3 → 5 → 7 → 2 → 4 → 8 → 9 → 10.
(Phase 7 before 2/4 because transcendentals are far more commonly used by
real Amiga software than non-extended FPCR precision or packed BCD.)

## 6. Definition of done

- Every documented opmode, format, addressing mode, condition, and
  system-control instruction executes (none decline to vector 11 except
  genuinely illegal encodings).
- Exceptions are both reported (FPSR) and delivered (traps when enabled).
- Arithmetic + conversions bit-exact vs SoftFloat; transcendentals bit-exact
  vs the FPSP reference (D1).
- The Musashi FP corpus stays green for everything Musashi implements; new
  ops have their own C-diff / FPSP-diff / unit coverage.
- `knowledge/decisions/fpu-softfloat-port.md` updated for the precision and
  FPSP decisions; a short `knowledge/decisions/fpu-transcendental-oracle.md`
  records D1/D2.

## 7. Decisions — all resolved (2026-06-15)

- **D1 — Transcendental reference → Grabher's softfloat FPSP (WinUAE/Previous),
  already vendored.** Bit-exact vs WinUAE = hardware-grade. Real-silicon
  validation (Phase 10) kept as optional confirmation, low-priority.
- **D2 — Licensing → settled.** SoftFloat-2a + BSD, GPL-2.0-or-later
  compatible; no Motorola FPSP needed.
- **D3 — Timing → inline with each phase** (cycle-accurate from the start);
  Phase 9 is the shared model + tables + coverage tracker.
- **D4 — 68881 vs 68882 → one result path + a model flag** (frame size in
  Phase 5; timing/concurrency per-phase + Phase 9).
