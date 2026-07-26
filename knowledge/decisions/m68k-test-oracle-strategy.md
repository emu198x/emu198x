# Decision: M68k test-oracle strategy

**Date:** 2026-05-21

## The decision

**The 68000 is tested against the implementation-generated SingleStepTests/680x0 corpus. The 68010 / 68020 / 68030 / 68040 are tested against Musashi-generated corpora because no equivalent retained SingleStepTests corpus exists for those variants. Both are software oracles; neither is treated here as a silicon capture.**

This is a deliberate stopping point, not the final shape. The cross-validation roadmap below names the work that follows.

## Why this matters

The SingleStepTests/680x0 README describes roughly a million randomly generated 68000 vectors. It says the generating implementation conforms to available documentation, passes other published test sets, and has been verified through use in an emulated machine. It does not identify the generator or claim that the vectors came from a physical processor or logic analyser.

For the 68010 / 68020 we generate test vectors via `m68k-test-gen` (lives in `Emu198x-Oldest/`), which uses **Musashi** as the reference oracle. Musashi is pressure-tested by vAmiga, fs-uae's optional path, MAME's m68k, and decades of community use. It is not infallible. The BCD V and DIV overflow divergences we caught this session ([motorola-68k-variant-pattern](motorola-68k-variant-pattern.md) §musashi_bcd_v / musashi_div_overflow) prove that Musashi and SingleStepTests encode different results. The PRM leaves BCD V and DIV overflow N/Z undefined while specifying DIV C clear and V set. The software-oracle difference does not determine the undefined bits on a physical processor.

The risk: either software oracle could contain systematic errors that our CPU reproduces but real silicon does not. Agreement between the two is useful cross-checking, but independence is not established because the SingleStepTests generator is unidentified.

## Provenance correction

Earlier versions of this decision described SingleStepTests/680x0 as a physical-processor, logic-analyser and real-hardware corpus. The suite's tracked metadata does not support those claims. They were removed on 2026-07-21 without changing the recorded pass counts.

## Additional 68000 software oracle

The separate `SingleStepTests/m68000` repository identifies MAME's
microcoded MC68000 core as its generator. Revision
`64b253116a3de04aaac4346c43680960dc9b67e5` contains 127 compact binary
fixtures with 317,500 rows and an MIT licence. Its README marks TAS and
TRAPV unverified and records special `re` and `we` transactions for
address errors whose abstract transfer is rejected without asserting
address strobe.

Emu198x keeps two separate comparisons:

- an agreement sweep excludes the 5,000 upstream-unverified rows,
  55,606 address-error rows, 14,304 instruction-family divergences and
  2,500 STOP rows, then requires all remaining 240,090 rows to agree;
- an address-error evidence sweep requires all 55,606 source-designated
  events to be observed with matching direction, frame address bus and
  function code, while separately pinning frame-field and final-state
  agreement.

The address-error sweep currently has 17,689 complete frame and final-state
agreements. The remaining rows are classified rather than accepted as
correct: most differ in frame PC, with smaller address-register,
data-register, status and instruction-word classes. The row-stable
taxonomy fingerprint is `efbdf3e93d4281ca`.

This corpus is another software oracle. Agreement strengthens regression
coverage, especially around an internally rejected transfer that the
ordinary machine bus cannot observe. Disagreement identifies accuracy
work; it is not resolved by assuming either generator represents silicon.

## Mitigations, in order of cost

### A. Inherited-subset cross-check (do now)

The 68010 / 68020 inherit the 68000 ISA with a small known-divergent set: BCD V, DIV overflow, exception and bus/address-error frame formats, MOVE-from-SR privilege, the M-flag, 68020 misaligned-data handling, and the 68020 `$FF` long-branch encoding. Run the SingleStepTests 68000 corpus through the `Cpu68010` and `Cpu68020` wrappers and confirm the **inherited subset** remains compatible with that corpus. Source rows that exercise variant-specific semantics are excluded before execution and pinned by count, class, and row identity; any failure in the retained subset is a regression relative to the suite.

This gives a cross-suite regression check for the inherited 68000 subset. It is cheap because the harness, corpus and wrappers already exist, and it tells us whether the variant wrappers have inadvertently broken compatibility with the established 68000 baseline. It is not hardware validation.

**Cost**: one session.

### B. Second-oracle generation (when wiring a 68020 machine)

When we wire the 68020 into an actual Amiga machine (CD32 / A1200 — Amiga-full-family-architecture-review Seam 2), we'll already be consulting **WinUAE** as the reference for AGA chipset behaviour. WinUAE has its own 68020 / 68030 / 68040 implementations in `cpummu.cpp` / `newcpu.cpp` that have been pressure-tested by the Amiga community for years.

Plan: extract WinUAE's CPU as a callable library, add it as a second oracle to `m68k-test-gen`, generate a **consensus corpus** (vectors where Musashi and WinUAE agree). The disagreement set becomes a manual-review queue — those are places one of the two oracles is probably wrong, and the answer is interesting in itself.

**Cost**: 1–2 weeks. Defer until the 68020 actually has a machine to wire into.

### C. Real-software pixel-diff (long term, with the catalogue)

The terminal test for any CPU is: does the rendered output of a curated set of real Amiga software match the rendered output of the reference emulator? `vAmiga` and `WinUAE` are the obvious references. This catches things unit tests cannot — DMA timing × CPU interleave × interrupt latency × chipset bus contention. It is also where every emulator project eventually lives. The Spectrum catalogue already uses this approach (FUSE captures + real-hardware photos); the Amiga catalogue will too.

**Cost**: weeks of work, but it's work we're committed to anyway for the catalogue.

## Drift triggers

Stop and revisit this decision if:

- **A documented silicon-derived corpus appears** for any M68k variant, or SingleStepTests publishes a higher-variant corpus with traceable methodology. Register its exact provenance before changing the oracle hierarchy.
- **Musashi merges a significant correctness fix** that affects M68k semantics. We pin `m68k-test-gen`'s Musashi version, so a known-good baseline doesn't drift, but a fix may indicate a class of bugs to investigate.
- **A real-software regression** appears that the corpus passes. That's the canary for "Musashi got something wrong, we copied it, the corpus can't catch it." (The AGA palette bug below was exactly this canary — caught by booting Workbench, not by the corpus.)
- **You touch 68020+ effective-address decode** (`ea.rs`, indexed modes, extension words). The inherited-subset harness skips indexed cases; `m68k-test-gen` exercises brief indexed words but not full-format words. See the coverage-gap section. Add hand-written cases or extend the generator; do not treat either green result as full-format coverage.

## Current state

| Crate | Pass rate | Oracle |
|---|---|---|
| `motorola-68000` | 1,000,058 / 1,000,058 = 100.00 % | SingleStepTests/680x0 (implementation-generated) |
| `motorola-68000` MAME agreement subset | 240,090 / 240,090 = 100.00 % | SingleStepTests/m68000 (MAME-generated) |
| `motorola-68010` | 1,831,992 / 1,832,000 = 99.99956 % | Musashi (m68k-test-gen, count=8000) |
| `motorola-68020` | 1,920,000 / 1,920,000 = 100.00 % | Musashi (m68k-test-gen, count=8000) |
| `motorola-68030` | 1,920,000 / 1,920,000 = 100.00 % | Musashi (m68k-test-gen, count=8000) |
| `motorola-68040` | 1,920,000 / 1,920,000 = 100.00 % | Musashi (m68k-test-gen, count=8000) |
| 68010 inherited retained subset | 753,676 / 753,676 exact | SingleStepTests 68000 subset; 157,667 structural address-error rows excluded |
| 68020 inherited retained subset | 666,066 / 666,066 exact | SingleStepTests 68000 subset; 124,225 structural address-error and 73 long-branch rows excluded |

**~7.6 million Musashi comparisons + 1 million SingleStepTests/680x0
comparisons + 240,090 MAME-generated agreement comparisons + 1.42 million
inherited-subset exact comparisons = ~10.25 million recorded
comparisons.** All five processor variants report ≥ 99.99956 % in their
primary generated suites.

The 68000 line was re-executed locally on 2026-07-25 against clean SingleStepTests revision `e0d5ece9670205cc84a0101081837deb446f86a3`. All 124 fixture files passed; the corpus contained 1,000,060 rows, of which two named invalid `ASL.b` rows were excluded and 1,000,058 were compared. Of those, 968,687 were exact agreements and 31,371 were narrowly classified address-error function-code or I/N differences, pinned by row fingerprint `52fb9713c00ab6ae`. The MAME-generated agreement and address-error sweeps were run on the same date. The higher-variant Musashi figures were not re-executed during the provenance correction.

The inherited-subset sweeps were re-executed on 2026-07-26 against the
same retained revision. The 68010 source partition contains 911,343 rows.
Its harness excludes 157,667 rows whose fixture transactions and final
memory identify a complete 68000 address-error event: 149,908 reads and
7,759 writes. All 753,676 retained rows agree exactly. The source-only
exclusion fingerprint is `c62326b76f889408`.

The 68020 source partition contains 790,364 rows. Its harness excludes
124,225 structurally identified 68000 address-error rows (118,664 reads
and 5,561 writes), because the 68020 permits misaligned data operands and
uses different bus-fault frames. It separately excludes 73 `$FF`
long-branch rows (38 Bcc and 35 BSR), whose encoding has variant-specific
meaning. All 666,066 retained rows agree exactly. The combined source-only
exclusion fingerprint is `660f34e427a17dca`.

Both harnesses classify and exclude these source rows before constructing
or executing a CPU. Their fingerprints contain only the fixture case name
and exclusion class; they cannot bless the emulator's current output.
Corpus checksums pin the fixture bytes separately.

The MC68000's group-0/group-1 exception-processing flag is serialized
because it controls the I/N bit of a subsequent address-error frame and
whether a recursive group-0 fault halts the processor. Adding that hidden
execution state changes every containing postcard payload. The Amiga
runtime therefore uses snapshot schema 14 and rejects version 13 rather
than restoring a semantically incomplete CPU.

The 68010's 8 failures are two clusters — 7 in `MOVEC_010` and 1 in `ADD.w_idxPC_D1` — at the 4-parts-per-million scale where multi-step exception sequences diverge in *when* state is captured (Musashi's `execute()` hook fires at instruction boundaries; some exception-frame pushes capture mid-sequence vs after). These are 68010-specific edge cases not present on the higher variants and not exercised by the inherited cross-check. Recorded here rather than chased: investigation cost ≫ correctness benefit at this scale, and the diagnostic value of count=8000 (catching the three Phase-7.6/Cpu68040-MOVEC bugs at higher rates than count=1000 would have) has already been collected.

Mitigation A (inherited-subset cross-check) landed as `harte_real_hw.rs` in the 68010 / 68020 crates. The filename is a legacy identifier and is not evidence of hardware provenance.

## Coverage gap: full-format effective addresses (found 2026-05-28)

**The inherited-subset harness does not exercise indexed addressing, while `m68k-test-gen` exercises only brief indexed extension words. Neither generated harness covers 68020 full-format extension words, despite the 100.00 % figures above.** Full format is a structural blind spot rather than a Musashi disagreement, so comparing the existing pass rates cannot expose it.

The two harnesses have different coverage boundaries:

- **`harte_real_hw.rs`** skips every case whose disassembled name contains `"(d8,"` (`is_indexed_addressing_case`). The 68000 corpus's random brief extension words carry non-zero scale bits (10-9) that the 68020 honours but the corpus expectation treats as unused. The substring skip therefore drops all inherited-subset indexed cases.
- **`m68k-test-gen`** only emits **brief** extension words: `generate_brief_ext_word` builds `d8 = rng.random::<u8>() & 0xFE`, so bit 8 (the brief/full selector) is always 0. The full format is never generated for any variant.

Result: the AGA Workbench palette bug — `lea (A3,D0.w*2),A5` (`$4BF3 $0310`) decoded as the brief `(16,A3,D0.w)`, a constant +16 — survived a reported "68020 100 % Tom Harte" pass. The fix (full-format EA decode following WinUAE `get_disp_ea_020`) is covered by hand-written `motorola-68020/tests/full_format_ea.rs`, because no generated corpus reaches it.

**Standing caveat:** the pass-rate table measures the generated or retained subset, not the whole ISA. A green inherited-subset run says nothing about indexed addressing. A green `m68k-test-gen` run covers brief indexed addressing but not full-format extension words. Closing the remaining gap means **extending `m68k-test-gen` to emit full-format words** (set bit 8, randomise BS/IS/BD-size/IS-field/scale, fetch the base/outer displacement words) and regenerating against Musashi or, preferably, the WinUAE consensus oracle of mitigation B.

## Related

- [Motorola 68k variant pattern](motorola-68k-variant-pattern.md) — the architectural shape that absorbs SingleStepTests-vs-Musashi divergence as variant flags (precedent: `variant_musashi_bcd_v`, `variant_musashi_div_overflow`).
- [Motorola 68020 implementation plan](motorola-68020-implementation-plan.md) — the phased work that reached the current state.
- [Amiga full-family architecture review](amiga-full-family-architecture-review.md) — Seam 2 (68k family completion) names when option B becomes timely.
