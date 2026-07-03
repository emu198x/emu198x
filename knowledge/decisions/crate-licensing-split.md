# Decision: Crate licensing — dual-license clean-room crates at publish time

**Date:** 2026-07-03
**Status:** Active. Records publishing *intent*; no relicense happens until a
crate's provenance audit passes. Companion to
[`versioning-strategy.md`](versioning-strategy.md) (lockstep by default, carve
out at publish time) and the umbrella
[`emu198x-best-in-class.md`](../../../decisions/emu198x-best-in-class.md)
(embeddable components as a moat).

## The decision

**GPL-2.0-or-later stays the workspace default, and that is deliberate, not a
constraint to escape.** The GPL default is what makes the project's porting
strategy frictionless: reSID, vAmiga/Mesen2-informed code, VICE-derived
logic, and planned *asset* lifts (e.g. FS-UAE's floppy-drive sound samples —
GPL binds assets as much as code) can all be taken with attribution and no
legal ceremony. Steve has no objection to GPL per se; dual-licensing is a
**narrow, targeted reach tool** for a handful of clean-room crates at their
publish moment — not a migration away from copyleft.

When chip/CPU/format crates are published to crates.io for other emulator
authors to consume:

1. **Clean-room crates dual-license `MIT OR Apache-2.0`.** Crates whose
   implementation derives from datasheets, primary references, test suites,
   and our own measurement — not from ported GPL code. Candidates (subject to
   audit): `mos-6502`, `zilog-z80`, the `motorola-68000` family, most
   format-* parsers.
2. **Ported/derived crates stay `GPL-2.0-or-later`.** Anything carrying code
   *or assets* from GPL sources keeps its obligations: `mos-sid-6581` (reSID
   port — DAC, filter tables, combined waveforms), any crate with substantive
   `Adapted from vAmiga/Mesen2/VICE/...` provenance, and any crate embedding
   the FS-UAE floppy-drive sound samples when that import lands (record the
   provenance in a code comment per RULES.md rule 27, and keep the samples in
   a GPL crate — e.g. the floppy peripheral crate — never in a dual-licensed
   one). These can still be *published* — GPL consumers (most emulators) can
   use them — they just cannot be dual-licensed.
3. **The app tier stays GPL** (`emu198x-*` binaries, `emu198x-shell`,
   `emu198x-ui`, runtimes). No reach argument applies there.
4. **No relicense without a per-crate provenance audit.** The audit walks the
   crate's history and `Adapted from …` comments (the RULES.md rule-27 audit
   trail exists for exactly this). Consulting a reference emulator to
   understand behaviour does not bind; porting its code does. The audit
   verdict is recorded in the crate's README and this record's log.
5. **Publish order: cleanest first.** `mos-6502` / `zilog-z80` /
   `motorola-68000` lead; don't gate the first publishes on auditing the
   whole workspace.

Copyright basis: single-author project (Steve, with LLM assistance), so
relicensing clean-room code needs no CLA archaeology. That convenience decays
the moment external contributions land — accepting a contribution to a
dual-licensed crate requires the contributor to grant the same terms (note in
CONTRIBUTING when publishing starts).

## Prerequisites (from the 2026-07-03 audit)

- Resolve the `isa-disasm` git dependency in `emu198x-shell` (publish it from
  Asm198x or make it dev-only/feature-gated) — blocks 63 crates.
- Packaging basics per published crate: README, `//!` docs, `keywords`/
  `categories`, one `examples/drive_cpu.rs` showing the pin-level driving
  contract; `publish = false` on internal-only crates.
- Per-crate version carve-out per [`versioning-strategy.md`](versioning-strategy.md).

## What this is NOT

- **Not a relicense of anything today.** Intent only; each crate flips at its
  own publish moment, after its audit.
- **Not a weakening of GPL where it's owed.** The reSID/vAmiga/Mesen2 debts
  are real and stay paid.
- **Not a promise to publish every crate.** Publish where there's a plausible
  consumer; the moat is a few excellent crates, not 198 mediocre listings.

## Drift triggers

- **Dual-licensing (or advising that we could) without the provenance
  audit** — especially the SID crate, which *looks* like a chip crate but is
  a reSID port and is GPL-bound.
- **"The workspace is GPL so we can't publish"** — false; GPL publishing
  serves GPL consumers today. Dual-licensing is about *reach*, not
  permission.
- **Accepting external contributions to a dual-licensed crate without the
  licence grant** — silently converts the crate back to undistributable.
- **Porting reference-emulator code into a clean-room crate** after its
  audit — re-runs the audit or re-binds the crate to GPL.

## Log

| Date | Event |
|------|-------|
| 2026-07-03 | Captured. Decided in the best-in-class strategy session (see umbrella record): dual-license clean-room crates at publish time, ported crates and app tier stay GPL, per-crate provenance audit mandatory, cleanest crates publish first. |
| 2026-07-03 | Rationale corrected same day (Steve): the GPL workspace default was never a burden — it was chosen deliberately to make non-clean-room ports frictionless, including planned asset lifts (FS-UAE drive noises). Framing adjusted: dual-licensing is a narrow reach tool, not an escape from copyleft; GPL binds assets as well as code. |
