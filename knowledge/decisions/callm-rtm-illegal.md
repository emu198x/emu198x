---
title: CALLM / RTM are deliberately illegal on the 68020 core
date: 2026-06-13
status: binding
scope: motorola-68020 (#114 ISA gap-fill)
---

# CALLM / RTM are deliberately illegal on the 68020 core

## Decision

The 68020 `CALLM` and `RTM` instructions (`$06C0`–`$06FF`) are **not
executed**. The 68020 decode hook takes the illegal-instruction
exception (vector 4) for the whole opcode range, the same as every other
unimplemented opcode. This is a deliberate, documented choice — not an
oversight or a gap left for later.

Pinned by `crates/motorola-68020/tests/callm_rtm.rs` (RTM Dn/An, and
CALLM in indirect / `(d16,An)` / `(xxx).L` / `(d16,PC)` forms all vector
to the illegal handler).

## Why not implement them faithfully

`CALLM`/`RTM` are the 68020's module-call mechanism: a descriptor-based
call/return with two descriptor types — Type 0 (no access-rights change,
shared stack) and Type 1 (access-rights change, negotiated with external
access-control hardware over CPU space, A19–A16 = `0001`). The full
behaviour is in MC68020UM § 9.7–9.8 (module descriptor format, module
call stack frame, access-level control registers).

Four facts make faithful execution the wrong call here:

1. **No oracle.** Both reference 68020 cores we validate against treat
   these as illegal: Musashi `m68k_in.c` has no `callm`/`rtm` op, and
   WinUAE/fs-uae `cpuemu_20.cpp` implements `op_06c0`…`op_06fX` as
   `op_illg`. So a faithful implementation could not be cross-validated
   against anything — hand tests would only assert our own reading of the
   PRM, which the `feedback_verify_*` rules warn against.
2. **Dropped from the 68030+.** Motorola removed `CALLM`/`RTM` from the
   68030 and all later parts; they trap as illegal there. Most of the
   Amiga 68020+ fleet (A2630/A3000/A4000 = 030/040) would treat them as
   illegal regardless, so "illegal" is the majority-correct behaviour for
   the family.
3. **No software uses them.** No compiler emitted `CALLM`; the module
   scheme never saw adoption. No Amiga software (or any 68k software in
   the catalogue) exercises them.
4. **Type 1 needs hardware no Amiga has.** A Type-1 `CALLM` negotiates
   access levels with external CPU-space hardware. On a real Amiga 68020
   that hardware is absent, so the CPU-space access bus-errors and the
   instruction takes a format exception anyway — i.e. it does not
   complete on real Amiga hardware either.

Implementing Type-0 execution would be a large, descriptor-driven state
machine (read descriptor, validate `opt`/`type`, build/tear down the
module stack frame, save/load the data-area-pointer register selected by
the module entry word) with no way to verify correctness, serving an
instruction nothing runs. That is gold-plating an unverifiable corner.

## Revisit if

- A concrete consumer appears (e.g. curriculum content that teaches the
  module mechanism, or a ROM/program that issues `CALLM`).
- A validated reference implementation becomes available to test against.

In that case, implement Type-0 `CALLM`/`RTM` per MC68020UM § 9.7 and
route Type-1 to a format exception (vector 14) for the missing
access-control hardware. Until then, illegal is correct and honest.

## Context

This is the final group of #114 (68020 ISA gap-fill). The other seven
groups — TRAPcc, PACK/UNPK, MUL/DIV memory-source, CHK2/CMP2, CAS, Bcc.L,
CAS2 — are implemented and validated 100% against a Musashi-generated
corpus. `CALLM`/`RTM` is the one group with no oracle, and this decision
closes it.
