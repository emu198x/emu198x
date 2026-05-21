# Decision: Motorola 68020 implementation plan

**Date:** 2026-05-21
**Status:** Proposed (execution plan)

## What this is

A phased execution plan for fleshing out `motorola-68020` from
skeleton crate (where it sits today) to a working 68020 / 68EC020
implementation. Implements Seam 2 of
[`amiga-full-family-architecture-review.md`](amiga-full-family-architecture-review.md).

The 68020 is the gating CPU for several major Amiga targets:

- **A1200** (68EC020 @ 14 MHz, AGA chipset)
- **CD32** (68EC020 @ 14 MHz, AGA + AKIKO)
- **A2500/30/UX accelerator boards** for A2000

Without a working 68020 the workspace can't model any AGA hardware
beyond a desk-check. With it landed, the entire AGA chipset
infrastructure (Lisa Denise, fat Agnus AGA) plugs onto a working
CPU and the catalogue can grow into the 1992-1994 software corpus.

## What we are *not* changing

- Tom Harte 68000 100% pass on `motorola-68000`. Untouched.
- `motorola-68k-common` shared substrate (addressing modes, ALU,
  bus pin types, register file, status flags, `CpuModel` enum).
  All 68020 additions land as additive modules or new arms.
- Crate boundary. `motorola-68020` is the home; this plan does not
  add new crates.

## What's already in place

The scaffold is done (skeleton at
`crates/motorola-68020/src/lib.rs`, 153 lines):

- Detailed silicon-feature doc comment covering bus, pipeline,
  new control registers (CACR/CAAR/MSP/ISP), new instructions
  (bitfield family, 32-bit MUL/DIV, CAS/CAS2, CHK2/CMP2, EXTB.L,
  PACK/UNPK, TRAPcc, CALLM/RTM, Bcc.L), changed addressing
  semantics (barrel shifter, scaled index, full extension word),
  coprocessor interface, new exception frames ($1/$2/$9/$A/$B).
- Type aliases `Cpu68020 = Cpu68000`, `Cpu68EC020 = Cpu68000` —
  stubs that route through the 68000 implementation. The 68000
  silently accepts shared-subset instructions; 68020-only
  instructions trap as illegal.
- Marker types `M68020Variant`, `M68EC020Variant` with
  `CpuModel::M68020` / `M68EC020` identifiers (already in the
  shared enum).

What's missing: the actual state machine, decode tables, new
addressing-mode parsing, instruction implementations, bus
protocol, cache, coprocessor interface stubs.

## The architectural fault line

The 68020 is a strict ISA superset of the 68000, but the bus
protocol shifts fundamentally:

| Aspect | 68000 | 68020 |
|---|---|---|
| Data bus width | 16-bit | 32-bit |
| Bus cycle | Asynchronous, DTACK-handshaked | Synchronous, clock-edge sampled |
| Slave size declaration | None (always 16-bit) | DSACK0/DSACK1 dynamic bus sizing per cycle |
| Address strobe | AS only | AS + SIZ0/SIZ1 |
| Minimum cycle | 4 clocks | 3 clocks |
| Pipeline | 2-word prefetch | 2-word prefetch + parallel decode |
| Instruction cache | None | 256-byte direct-mapped, 16 lines × 4 words |

The Amiga A1200's 68EC020 sits on a 16-bit chip bus (matching the
OCS/ECS/AGA chipset's data path) but runs at 14 MHz with
dynamic-bus-sized memory accesses. So the bus protocol differences
matter for any Amiga code that touches custom registers — the
typical 68000 chipset machine assumes word-aligned 16-bit
read/write, the 68020 may emit narrower or wider transactions.

This means the machine layer wiring is also new work:
`machine-commodore-amiga-aga` will need a `service_cpu_bus`
analogue that understands SIZ0/SIZ1 and DSACK0/1, not the
68000's pure DTACK-vs-VPA dispatch.

## Tom Harte test corpus

The Tom Harte project (SingleStepTests on GitHub) ships
single-step test vectors for the 68000 (canonical, in CI at
100%) and community-generated coverage for 68020 / 68030 /
68040 (separate corpus, also single-step format).

**Action needed early in execution**: locate or download the
68020 corpus into `~/Projects/198x/assets/test-suites/`,
register a path env var (`M68020_TEST_DATA`), and wire a
`#[ignore]`'d integration test in `motorola-68020/tests/`
mirroring `motorola-68000/tests/tom_harte.rs`. The 68020
corpus is per-instruction JSON files, 10,000 vectors each.

Without the corpus the implementation has no objective
correctness oracle and would silently diverge from silicon.

## Phased implementation

Each phase below ends with a green test gate. Phases are sized
to fit roughly one focused session, but can stretch — Phase 0 is
small, Phase 1+ are large.

### Phase 0 — Tom Harte 68020 corpus + first vector pass

**Goal**: harness in place + the shared 68000-subset already
runs through it via the type alias.

**Status: complete (2026-05-21).**

- ~~Download Tom Harte 68020 corpus~~ — none is published
  upstream (the SingleStepTests/680x0 repo stops at 68000).
  Instead we use `Emu198x-Oldest/crates/m68k-test-gen`, which
  drives Musashi as the reference oracle and emits MessagePack
  vectors using the same schema. Generated corpus lives at
  `~/Projects/198x/assets/test-suites/m68k-generated/m68020/v1/`
  — 240 fixtures, 10 vectors each, 6.6 MB.
- `motorola-68020/tests/tom_harte.rs` is in place. Wiring
  detail worth recording for future maintainers: the fixture's
  `initial.prefetch` is Musashi's raw `[IR, PREF_DATA]`, not the
  opcode bytes — Musashi's IR after `pulse_reset` is stale, so
  the harness reads the opcode and IRC straight from
  `initial.ram` (where `encode_instruction` poked them) and
  ignores the fixture's prefetch field.
- Run: `cargo test --release -p motorola-68020 --test tom_harte
  -- --ignored harte_baseline_full_sweep --nocapture`.

**Baseline**: **2072 / 2400 = 86.33 %** on `Cpu68020 = Cpu68000`.
198 / 240 fixtures fully passing; 16 fully failing; 26 partial.

The 14 % gap maps cleanly onto the later phases:

| Cluster | Fixtures | Pass rate | Resolves in |
|---|---|---|---|
| Scaled-index brief extension word | 16 (ADD/ADDI/CLR/LEA/MOVE/PEA/CHK `idx` variants) | 10-60 % (≈ 1-in-4 cases happen to use scale = ×1) | Phase 3 |
| Bit-field family (BFTST/BFEXTU/BFEXTS/BFINS/BFCLR/BFSET/BFCHG/BFFFO) | 8 | 0 % | Phase 5e |
| 32-bit MUL.L / DIV.L | 2 (MULL, DIVL) | 0 % | Phase 5a |
| 68010-era control regs / RTD / BKPT / EXTB.l / MOVE from CCR | 5 | 0 % | Phase 1.5 (forking `Cpu68010` ahead of `Cpu68020`) |
| SR M-flag (bit 12) handling in MOVE-to-SR / ORI-to-SR / EORI-to-SR | 3 partial | 10-60 % | Phase 6 |
| DIVS / DIVU edge-case flags | 2 partial | 40-80 % | Phase 5a follow-up |
| NBCD / SBCD flag edge cases | 2 partial | 70-80 % | likely Phase 1 (carries over from the 68000 corner cases) |

This phase produces no production code — just a CI gate +
baseline measurement.

### Phase 1 — fork `Cpu68020` from the type alias

**Goal**: `motorola-68020` owns its own state machine instead of
re-exporting `Cpu68000`.

- Add a real `Cpu68020` struct in `motorola-68020/src/cpu.rs`.
  Initially: clone `Cpu68000`'s layout, with a few new fields
  for 68020 state (CACR, CAAR, MSP, ISP, prefetch pipeline of
  two words instead of one).
- Implement the tick loop via re-use: most 68000 cycles delegate
  to the same micro-op processing the 68000 uses (reuse the
  `motorola-68k-common::microcode` module).
- Re-introduce the type alias only as a deprecated transition:
  drop the `Cpu68020 = Cpu68000` re-export, expose
  `Cpu68020` as the canonical type.
- Tom Harte coverage: subset that passes on `Cpu68000` should
  continue to pass on `Cpu68020`.

### Phase 2 — synchronous bus protocol

**Goal**: `Cpu68020` exposes the 68020 pin surface (SIZ0/1,
DSACK0/1, AS, DS, R/W, FC0/1/2, IPL0/1/2, BERR, HALT, RESET,
CDIS, CIIN).

- Define the bus pin types in `motorola-68k-common::bus` (extend
  the existing `BusPins` shape; 68020 needs more pins).
- Bus state machine: replace 68000's "wait for DTACK low" with
  68020's "sample DSACK on next clock edge, decode size from
  SIZ pins."
- The machine layer for AGA will drive these pins; for now
  `Cpu68020` just exposes them and the test harness wires them
  up.

### Phase 3 — scaled index + brief extension word

**Goal**: 68020 brief extension word with scale field.

- Extend `motorola-68k-common::addressing` to decode the scale
  field (bits 9-10) on `(d8,An,Xn.SIZE*SCALE)`.
- 68000 ignores bits 9-10 of the brief extension word; 68020
  reads them as scale (×1, ×2, ×4, ×8).
- Tom Harte: instructions using scaled-index addressing
  (`MOVE.L (d8,A0,D0.L*4),...` and similar) start passing.

### Phase 4 — full extension word format

**Goal**: support `([bd,An,Xn.SIZE*SCALE],od)` and the full
extension word's pre-/post-index + base displacement + outer
displacement.

This is the single largest piece of decode work in the entire
68020 — full extension word parsing is roughly the same effort
as everything else combined per the M68000PRM. Phaseable
internally:

- 4a: base displacement (BD) — adds 16/32-bit BD field.
- 4b: outer displacement (OD).
- 4c: pre-/post-indexed memory indirection — the `([])` syntax
  in disassembly. Requires the decoder to perform a sub-fetch
  during operand resolution.
- 4d: suppress-base (BS bit) and suppress-index (IS bit) —
  build the address from only the displacements.

Tom Harte coverage grows substantially after this lands.

### Phase 5 — new instructions

In rough order of test-vector frequency / Amiga code dependency:

- 5a: **Bcc.L** (long-displacement branch). Trivial — adds one
  branch-class to the decoder.
- 5b: **Barrel shifter**. The 68000's shift counts loop; 68020
  is constant-time. Cycle-count change, not semantic — most
  test vectors don't notice.
- 5c: **EXTB.L**, **PACK**, **UNPK**. Small standalone
  instructions.
- 5d: **TRAPcc** family.
- 5e: **Bitfield** family (BFTST/BFEXTU/BFEXTS/BFINS/BFCLR/
  BFSET/BFCHG/BFFFO). Biggest of these — each takes a
  field-offset/field-width extension word.
- 5f: **CHK2 / CMP2**.
- 5g: **CAS / CAS2** — atomic compare-and-swap. The CAS2 form
  is a 12-byte instruction with two memory operands and four
  data registers.
- 5h: **32-bit MUL.L / MULS.L / MULU.L / DIVS.L / DIVU.L**
  including the 64-bit dividend `DIVx.L Dh:Dl` form.
- 5i: **CALLM / RTM** — module call. Largely vestigial; the
  68030 removed them. Implement for spec-compliance.

Each sub-phase ends with the Tom Harte slice for those
instructions passing.

### Phase 6 — exception frames + interrupt model

**Goal**: format $1 / $2 / $9 / $A / $B frames + MSP/ISP
split + RTE format dispatch.

- Implement format $1 (throwaway interrupt frame — 8 words).
- Implement format $2 (instruction-error trap — 6 words).
- Implement format $9 (coprocessor mid-instruction — 10 words).
- Implement format $A (short bus / address error — 16 words).
- Implement format $B (long bus / address error — 46 words).
  This is the big one — captures internal CPU state for the OS
  to restart after fixing the page table. Skip for now if no
  Amiga code requires it (most A1200 software does not).
- MSP / ISP split: SR M-bit (bit 12) selects MSP. Interrupts
  always use ISP; OS-scheduled tasks use MSP when M=1.

### Phase 7 — instruction cache

**Goal**: 256-byte direct-mapped instruction cache, CACR /
CAAR control, hit/miss in the fetch path.

- Cache state: 16 lines × 4 words = 64 entries, each with a
  20-bit tag + FC bits + valid flag.
- Fetch path: tag-and-FC match → hit, return cached word;
  miss → memory cycle, fill the line.
- CACR bits: EI (enable), FI (freeze), CI (clear). Toggled via
  `MOVEC` privileged.
- Per-line invalidate via CAAR + CACR CD (clear) bit.

The cache is *transparent* — programs see the same memory
either way, modulo write-through coherency (the data side isn't
cached on 68020). It exists in this implementation mainly so
cache-control instructions work correctly; runtime performance
is fine without modelling cache misses cycle-accurately.

### Phase 8 — coprocessor interface stubs (F-line)

**Goal**: F-line opcodes don't trap as illegal; they perform
the coprocessor handshake (CIR/CSR/CCR memory-mapped reads/
writes) and return without doing FPU/MMU work.

- Decoder recognises F-line range ($F000-$FFFF) and
  dispatches to a coprocessor handler.
- Handler reads `CIR` / `CSR` / `CCR` via the bus to negotiate
  the operation type — but executes nothing.
- 68EC020 path: F-line traps as `LINE 1111 EMULATOR` (vector 11)
  per the EC variant's no-coprocessor behaviour.
- Full 68020 path: handshake completes, the "FPU"/"MMU" returns
  trivial replies (idle/no-op). When the real FPU lands in
  `motorola-68040::fpu` (or its own crate), F-line routes through
  there.

The Amiga A1200 / CD32 use 68EC020 — they take the F-line trap.
A500 + accelerator with real 68020 + 68881 FPU is the case that
needs the full handshake later.

## Per-phase scope estimates

Lines added (test code excluded; tests roughly 2× production):

| Phase | Scope | Implementation lines |
|---|---|---|
| 0 | Tom Harte harness | ~150 |
| 1 | Fork Cpu68020 from alias | ~300 |
| 2 | Bus protocol | ~400 |
| 3 | Scaled index | ~50 |
| 4 | Full extension word | ~800 |
| 5 | New instructions (all sub-phases) | ~1500 |
| 6 | Exception frames + MSP/ISP | ~400 |
| 7 | Instruction cache | ~300 |
| 8 | Coprocessor stubs | ~150 |

**Total**: ~4000 lines of implementation, ~8000 lines including
tests. Matches the review's "~3000-5000 lines per CPU" estimate
once test code is included.

Realistic calendar: 8-15 focused sessions if each lands one
phase cleanly. Some phases (4, 5) may need multiple sessions on
their own.

## Cross-validation strategy

For every phase, three reference points:

1. **Tom Harte vectors** — primary correctness oracle. Each
   instruction has 10,000 input/output vectors. Pass rate
   per-instruction is the per-phase done criterion.
2. **Musashi** (vendored at `Emu198x-Unclean/emulators/cpu-libs/`)
   — C implementation of 68000-68040. Read for "how did Musashi
   handle this?" when an instruction's spec is ambiguous.
3. **WinUAE** — most cycle-accurate open Amiga emulator; covers
   the full 68k family. Use for bus-protocol questions and
   chipset-bus interaction edge cases.

The M68020 User's Manual at
`/Users/stevehill/Projects/198x/reference/by-topic/cpu-68020/`
is the canonical silicon spec.

## Open questions to resolve early

1. **Bus protocol modelling depth.** Do we model the 3-clock
   bus cycle phases (S0..S5) explicitly, or treat each bus
   transaction as atomic the way the 68000 currently does?
   The Amiga's chipset DMA arbitration may force the explicit
   model — the 68020 sits on a chipset bus that's still
   primarily Agnus-arbitrated.

2. **Where do FPU registers live?** The 68881/68882 has its
   own register file (FP0-FP7, FPCR, FPSR, FPIAR). The full
   68020 + FPU pair needs these somewhere — in `motorola-68020`
   itself, in `motorola-68040::fpu` (whose FPU is on-die),
   or a new `motorola-68881` crate? The review suggests
   `motorola_68040::fpu`; revisit when implementing Phase 8.

3. **Instruction cache coherency model.** Real 68020 has no
   data-side cache (added in 68030); writes go straight through
   to memory. But the I-cache can become stale if code writes
   to instruction memory and doesn't CINV. Most Amiga software
   doesn't self-modify; we can probably ignore this case for
   the A1200 / CD32 catalogue.

4. **68EC020 vs 68020 capability gate.** Per `CpuCapabilities`:
   the EC variant omits the coprocessor interface. We model
   this with the existing capability gates or by routing all
   F-line through a `Cpu68EC020` shim that traps?

## Done criteria

- **Phase-by-phase**: each phase ends with the Tom Harte slice
  for that phase's instructions at >95% pass rate. Per-vector
  failures investigated and either fixed or documented as known
  silicon-edge gaps with M68000PRM citations.
- **Phase 7 end**: `Cpu68020` runs a synthetic test program
  (sequence of instructions exercising scaled index + bitfield
  + 32-bit DIV) to completion in a hermetic test harness.
- **Phase 8 end**: 68EC020 F-line trap behaviour matches WinUAE
  for at least 10 sampled F-line opcodes.
- **Integration**: a future `machine-commodore-amiga-aga` crate
  can substitute `Cpu68020` for `Cpu68000` in the wiring and
  boot the same chipset RAM patterns (NOP loops, simple
  copper lists).

## Non-goals

- **68030 implementation.** PMMU + on-die data cache stay in
  `motorola-68030`'s skeleton. Land 68020 first; 68030 inherits
  the bus + decode work.
- **68040 implementation.** Harvard caches, on-die FPU, MMU — all
  deferred to `motorola-68040`. The skeleton crate documents
  the deltas.
- **AC68080 / Apollo Vampire.** Out of scope until a real
  Vampire target enters the catalogue. The FPGA-internal
  pipeline is not project work.
- **PiStorm.** A PiStorm Amiga uses a real CPU; this plan does
  not model PiStorm-specific timing or host-FS shims.
- **Bus-cycle cycle-accuracy beyond Amiga needs.** The 68020
  in an Amiga sits on Agnus's arbitration grid; sub-cycle
  timing within a bus transaction matters less than wall-time
  CCK alignment. If a per-game accuracy gap surfaces, address
  per-game.

## Related

- [`amiga-full-family-architecture-review.md`](amiga-full-family-architecture-review.md)
  Seam 2 — the parent review
- [`amiga-architecture-review.md`](amiga-architecture-review.md) —
  the OCS-focused review whose 5 seams landed earlier
- [`cpu-bus-interface.md`](cpu-bus-interface.md) — the pin-level
  CPU rule the 68020 implementation respects
- [`within-family-layering.md`](within-family-layering.md) —
  the per-CPU-variant crate pattern

## Reference library cross-links

The 68020-relevant primary references:

| Reference | Topic | Phase |
|---|---|---|
| *M68020 User's Manual* (Motorola, 4th ed.) | Canonical silicon spec — bus, decode, exceptions | All |
| *M68000PRM* (Motorola Programmer's Reference Manual) | Instruction-by-instruction spec including 68020 additions | 5, 6 |
| *M68881 User's Manual* | FPU pair documentation | 8, future FPU |
| Musashi source (vendored) | Reference implementation, 68000-68040 | All — when spec ambiguous |
| WinUAE source (`Emu198x-Unclean/emulators/amiga/winuae/`) | Cycle-accurate Amiga 68020 | 2, 7 |
| Tom Harte 68020 corpus (to obtain) | Per-instruction correctness oracle | All |

The 68020 has more public reference material than any other
68k variant — it was the workhorse of late-80s/early-90s
workstations (Sun-2, Sun-3, NeXT Cube, Mac II) and is
exhaustively documented.
