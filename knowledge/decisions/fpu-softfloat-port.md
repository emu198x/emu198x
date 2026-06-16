---
title: 68881/2 FPU arithmetic is a Rust port of Berkeley SoftFloat floatx80
date: 2026-06-14
status: binding
scope: motorola-68k-common (fpu / softfloat), #112
---

# 68881/2 FPU arithmetic is a Rust port of Berkeley SoftFloat floatx80

## Decision

The 80-bit extended-precision (`floatx80`) arithmetic backing the
68881/68882 FPU (#112) is a **faithful Rust port of the `floatx80`
routines from Berkeley SoftFloat** (release 2b, John R. Hauser) — the
same library Musashi vendors and runs. It lives in
`crates/motorola-68k-common/src/softfloat.rs` and operates directly on
[`FpReg`], whose `{high, low}` layout matches SoftFloat's `floatx80`.

The port is a deliberate transliteration of the original algorithm, not
a re-implementation: this makes results **bit-identical to Musashi by
construction**, so the Musashi-generated FP corpus validates the port at
100% rather than ~99%-with-a-tail.

## Why a port, not a dependency

Considered four options for the backend (see the #112 discussion):

1. **Port Berkeley SoftFloat floatx80 to Rust** — *chosen*.
2. Wrap the vendored C SoftFloat via FFI — fast, 100% match, but puts a
   C compile step in the shipped emulator crate (the test generator
   already compiles it; the emulator must not have to).
3. `rustc_apfloat` (pure-Rust, x87 80-bit) — no C, but IEEE-strict, so
   ~99% vs Musashi with an edge-case tail (NaN payloads, rounding ties,
   denormals) to chase, and no transcendentals.
4. Hand-roll our own from scratch — novel design, own quirks, no
   exact-match guarantee.

The port uniquely satisfies all the project's constraints at once:

- **Pure Rust, no C in the emulator** — the workspace stays C-free; no
  Windows/cross-platform build wrinkles (boring-tech / minimal-deps).
- **Bit-exact vs the oracle** — same algorithm → identical results, so
  "100% Musashi" is by construction, not aspiration (max authenticity).
- **In-tree, auditable, no external dependency to vet or track.**

The scope is bounded: only the `floatx80` functions Musashi's `m68kfpu.c`
actually calls — `add/sub/mul/div/sqrt/rem`, `is_nan`, and the
`int32 / float32 / float64 ↔ floatx80` conversions — plus their shared
helpers (128-bit shift/multiply, `normalizeFloatx80Subnormal`,
`roundAndPackFloatx80`). Roughly 600–1000 lines, ported and corpus-
validated incrementally.

## Licensing

Berkeley SoftFloat 2b carries a BSD-style notice (Hauser / UC Berkeley).
A port is a derivative work, so the notice is **retained in the module
header** (`softfloat.rs`). BSD is compatible with this project's
GPL-2.0-or-later licence.

## Caveats

- **Transcendentals are out of scope of SoftFloat.** FSIN/FCOS/FATAN/
  FETOX/etc. use the chip's internal CORDIC/polynomial approximations,
  which are not bit-exact to any IEEE library, to Musashi, or even across
  real chip revisions. They are handled separately and remain best-effort
  — they cannot be "100% validated" against anything.

## Status / revisit

Foundation landed: the module + exact `int32_to_floatx80` + extract/pack
helpers (8 unit tests). The rounding core (`roundAndPackFloatx80`) and
the arithmetic ops follow, each validated against the Musashi FP corpus
(once the generator emits FP-register fixtures). No reason to revisit the
approach unless a faithful port proves intractable for a specific op — in
which case fall back to wrapping the C SoftFloat for that op only.
