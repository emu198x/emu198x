# Decision: Debug198x sidecars are read through the shared crate, not reimplemented

**Date:** 2026-08-17
**Status:** ACTIVE
**Applies to:** every consumer of `.debug198x` debug info in this repo
**Issue:** #741

## The decision

Emu198x reads Debug198x sidecars by depending on the **`debug198x` crate from
the Asm198x repo**, pinned to a git revision. It does not parse the NDJSON
itself, and it does not vendor a copy of the reader.

The dependency direction is one-way and must stay so: **Asm198x writes, Emu198x
reads**, and `debug198x` depends on serde alone. Nothing in this repo may reach
into the assembler — no parser, no engine, no dialects.

## Why a git dependency

Issue #741 proposed a *path* dependency into the Asm198x workspace. That does
not survive CI: the runners check out this repo only, so
`../../Asm198x/asm198x/crates/debug198x` does not exist and every job fails.
Making it work would mean checking out a second repository in every job at a
matching revision — build-time coupling between two repos to import one
serde-only struct.

Publishing `debug198x` to crates.io would also work, and is the better long-term
answer once the format freezes at v1. It was not the right move *now*: this
integration is what triggers the freeze, so publishing first would pin a
pre-freeze contract in public.

A git dependency is neither of those. It is also not a new pattern here —
`isa-disasm` has been consumed from the same repo the same way since the
rung-1 wiring, so this follows the existing convention rather than inventing
one. `publish = false` across this workspace (cargo-dist ships binaries; no
crate goes to crates.io) removes the one real objection to git dependencies:
crates.io rejects them in published packages, and nothing here is published.

### Why its own revision

`debug198x` is pinned separately from `isa-disasm` rather than sharing one
revision. The `isa-disasm` pin is from 2026-06-04; `debug198x` did not exist
until 2026-07-06, so a shared pin would mean moving `isa-disasm` forward —
changing disassembly output across every 6502 and 6809 machine, and the golden
tests that assert it, as a side effect of adding debug info. Cargo checks the
repository out twice and is otherwise untroubled.

## Substitution is narrow on purpose

Disassembly is symbolised two ways: each instruction is **annotated** with the
label at its address and the line that produced it, and its operands are
**substituted**, so `JSR $C012` reads `JSR init`.

Substitution operates on text produced by four different disassemblers rather
than on a structured operand, so the rule is deliberately conservative:

- **Only four-digit `$XXXX` literals.** Two-digit ones are ambiguous —
  `!byte $05` is data, not an address — and a zero-page *label* (as against a
  constant) is vanishingly rare, so the trade goes in the safe direction.
- **Never immediates.** `LDA #$05` must survive even when its digits match an
  address. Substituting into a value is how an instruction acquires a symbol
  because something unrelated was equated to 5.
- **Never index displacements.** `(IX+$05)` is an offset from a register.
- **Never constants.** `symbol_at` resolves labels and entry points only, so a
  constant equal to an address cannot rename it. In the C64 fixture `border`
  is a constant equal to `$D020`, and the write to it stays `STA $D020` — a
  constant is not a location, so there is no label there to name.
- **Never part of a longer literal.** A fifth hex digit disqualifies the match;
  taking the first four of a 24-bit literal would name an unrelated address.

The effect is that substitution happens only when a label is defined at exactly
the address the instruction refers to — precisely when it is the right name.

This was reached the second time. The first pass annotated only, on the
argument that rewriting disassembler output is fragile. That was a real
concern, but it silently delivered less than #741 asked for ("disassembly
renders `jsr init` instead of `jsr $c012`"), and inspecting the disassemblers'
actual output showed the concern was answerable rather than fatal: immediates
are always `#`-prefixed and displacements always signed, so excluding both
removes the false-positive case entirely.

`DisasmInstruction` gained `symbol` and `source`, both skipped when absent, so
output for a build with no sidecar is unchanged byte for byte.

## Refusals over silence

Three cases report rather than degrade quietly, because each one otherwise
looks like a legitimate empty answer:

- A file whose header is not `debug198x` is **rejected**, not read as a build
  with no symbols. NDJSON is a common container; another tool's file parses
  cleanly and then answers every lookup with `None`.
- A `format_version` this build does not know is **rejected**. The format is
  pre-1.0, so there is no compatibility promise to lean on, and answering
  lookups from a file we may be misreading is worse than refusing it.
- A step needing symbols with no sidecar attached **errors naming
  `load_debug_info`**, rather than behaving as "symbol not found". A
  source-line breakpoint that never fires because nobody loaded the sidecar is
  an expensive afternoon.

A source line that emitted no bytes — blank, comment, bare label — returns no
address, and the breakpoint is *not* moved forward to the next line that did.
A breakpoint that silently lands somewhere other than where it was asked for is
worse than one that reports it cannot be set; a caller wanting that policy can
walk forward itself.

## The base map is the paging state

Absolutely-located builds (C64, Spectrum 48K) carry their base in the sidecar
and resolve with no help. Everything else supplies bases, and the rule for
doing so is the same in both cases that need it:

**Map only the sections that are actually live.** Amiga hunks are placed by the
loader at run time, so the consumer maps them where they landed. A banked
machine maps only the sections whose page is currently in a slot — a bank that
has paged out stops being mapped. A section with no base contributes no address
and cannot answer, and that is the mechanism, not a shortfall: it is what keeps
a paged-out bank out of the lookup.

This is why [`DebugSymbols::set_paging`] replaces the base map wholesale.
`set_section_base` accumulates, and an accumulating map lets a caller hold two
banks of one slot mapped at once — a machine that cannot exist — at which point
the lookups answer by record order. Rebuilding on every paging change makes
that state unreachable.

### I got this wrong first, and it is worth recording why

I read this as a defect in the shared reader: two symbols at one address
distinguished in the file only by `space`, which the lookups never consult, so
whichever record came first won. I filed it as asm198x#71 against the v1
freeze.

The premise was false. It only held because my test set *both* sections of slot
3 to `$C000` simultaneously — my own comment said "page **either** bank into
slot 3" and the code then mapped both. Map one, as a real machine does, and
both banks resolve correctly today on the unchanged API. asm198x#71 is closed
as not-a-defect; `banked_fixture_resolves_per_paging_state` had been asserting
the correct behaviour in the Asm198x repo since 2026-07-06.

The failure was not reading the fixture wrong — it was inventing a consumer
model instead of finding the stated one. I read the crate source, which did not
state it; the spec page did, in *The consumer model*. **A contract this repo
depends on is not learned from the type signatures.** The lookups have a shape
that admits an impossible base map, so the shape alone cannot tell you the
rule.

Two real things came out of it: `Section` now carries the same optional `space`
as address-kind symbols, so a consumer holding a live paging state can derive
the base map by lookup rather than by scraping a symbol out of each section
(asm198x#72), and the banked contract now lives in the crate's own rustdoc
rather than only on the spec page.

## Test provenance

The C64 fixture is a real `asm198x --dialect acme --prg --debug` build, not a
hand-authored sidecar: a hand-written one would test this reader against my
idea of the format rather than against what the writer emits.

The end-to-end test needs no Commodore ROMs. `C64Runtime::new` validates ROM
*sizes*, not contents, so the test supplies an 8 KiB KERNAL that is zero
everywhere except the reset vector, which points at the address the fixture
loads to. The machine comes up executing the program under test. This matters
beyond convenience — see
[`a-gate-nobody-runs-is-a-silent-gate.md`](a-gate-nobody-runs-is-a-silent-gate.md):
a test that skips whenever a corpus is absent is green because it did not run.
This one always runs.

## Drift triggers

Re-read this entry when any of these come up:

- "Just parse the NDJSON here, it's only a few lines" — the format is a shared
  contract about to freeze; two readers means two interpretations of it.
- "Vendor the reader so we don't need the git dep" — same problem, plus drift.
- "Substitute symbols for two-digit or immediate operands too" — see
  *substitution is narrow on purpose*; each exclusion is preventing a specific
  wrong rename.
- "Fall back to the next line with code when a breakpoint line is empty" — see
  *refusals over silence*.
- "Bump `isa-disasm` so both deps share a revision" — that changes disassembly
  output; it needs its own change and its own golden review.
- Anything proposing a dependency **from `debug198x` back to `asm198x`**, or
  from this repo to the assembler. That is the circularity the format exists
  to avoid.
- "Two symbols share an address, so the reader must be page-aware" — check
  first whether the base map is describing an impossible machine. See *I got
  this wrong first*.
- Accumulating into the base map across a paging change, or reaching for
  `set_section_base` on a banked machine. Use `set_paging`.
