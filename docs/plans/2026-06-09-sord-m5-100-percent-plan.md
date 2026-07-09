> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Sord M5 to 100% — system wiring finish, media breadth, peripheral preservation"
type: plan
date: 2026-06-09
system: docs/systems/sord/m5.md
basis: code-grounded survey of machine-sord-m5 / runtime-sord-m5 + in-crate test runs, the established Z80/TMS9918/SN76489 shared-chip assessments, and the outstanding-work / current-system-usability / drivability-assessment status docs, 2026-06-09
---

# Sord M5 — road to 100%

What it would take to bring the Sord M5 to feature- and accuracy-complete,
grounded in a code-level read of `machine-sord-m5`, `runtime-sord-m5`, and the
shared `zilog-z80` / `ti-tms9918` / `ti-sn76489` / `zilog-z80-ctc` crates it
wires, plus the system's rows in the status docs. The shared-chip findings
(Z80 at-ceiling; TMS9918 collision/backdrop/sprite-register defects; SN76489
period-0 + serde) are established elsewhere and are referenced here, not
re-derived.

## Executive summary

**The Sord M5's hard part is done, and what remains is almost entirely
system-specific breadth plus one real save-state defect.** This is the same
shape as the Spectrum, not the C64: the core that mattered — getting a Japanese
Z80 + TMS9918A + SN76489 machine with a Z80-CTC interrupt spine to *boot through
to a rendered screen* — is finished. The M5 reaches BASIC-I's `Ready` prompt and
renders Dig Dug's title screen (`current-system-usability.md:77`,
`outstanding-work.md:1083-1098`). The CTC wiring that carries it there is real
and was hard-won: the TMS9918 `/INT` line is inverted into CTC channel 3's
`CLK/TRG`, the CTC supplies the IM 2 vector, and the machine watches the opcode
stream for `ED 4D` to release the daisy chain (`machine-sord-m5/src/lib.rs:241-313`).

The chips underneath are in good shape. The **Z80 is at-ceiling** (Tom Harte
1,604,000/1,604,000, FUSE per-cycle) — no M5 Z80 work. The **TMS9918** and
**SN76489** carry shared accuracy gaps (sprite collision ignoring transparent
sprites; mid-frame backdrop one frame late; SN76489 period-0 not clamped) that
land on the M5 along with six/seven other systems — those are filed at the chip
level and are **not** re-filed here.

So "100% M5" is **finishing the system wiring (cart banking, save-state),
validating the provisional peripherals against a known-good trace, and the
preservation long tail (cassette, printer, RAM expansion)** — none of it a core
rewrite.

Two things are genuinely unsettled and need a real machine or MAME trace, not
more code-reading: the **Dig Dug round-freeze** (narrowed to the cart's
round-init state machine, stalled at RAM `$754A`, three diagnostic passes,
stopped per the anti-thrash rule — `drivability-assessment.md:300-348,369-376`)
and whether that freeze is an M5 wiring bug or a shared-chip defect manifesting.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | real save-state (cold-boot stub today), Dig Dug round-freeze root-cause, keyboard/joystick CI validation against a trace, doc/reference distillation | **~2–3 weeks** |
| B — System core finish | cart bank-switching mapper (cart ceiling is a flat 20 KB today), 64KBF RAM-expansion paging latch, region/PAL end-to-end validation | **~1.5–2.5 weeks** |
| C — (no M5-specific audio/video core work) | the audible/visible accuracy items are the shared TMS9918 + SN76489 chip issues, filed at chip level | **—** |
| D — Preservation breadth | cassette load/save, Centronics printer, cart-RAM carts (FALC-class), `.m5`/cartridge container formats, EM-5 / printer-plotter peripherals | **~3–5 weeks** |

**True 100% of everything ≈ 6.5–10.5 weeks** of M5-specific work, on top of the
shared-chip fixes landing across the TMS9918/SN76489 consumer fleet. It is
**front-loaded** onto cheap, high-confidence wins (save-state, validation,
docs) with the harder, lower-demand preservation tail (cassette, printer)
deferrable. The launch-relevant slice (Tier A) is small.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (system wiring + validation)

| Item | Effort | Notes |
|------|--------|-------|
| **Real save-state** | **M** | The snapshot is a **cold-boot stub**: `runtime-sord-m5/src/snapshot.rs` serialises only `version`, `time`, `model_id`, and the ROM/cart *bytes*, then `decode` calls `rebuild_after_restore` → `rebuild_machine` (`runtime.rs:118-140`), which constructs a **fresh** `SordM5::new(rom, cart, region)` and sets the tick counter. Live RAM (`$7000-$7FFF`), CPU registers, VDP/VRAM, PSG, CTC channel state, and the keyboard matrix are **all lost** — "restore" cold-boots and fast-forwards the clock value without re-running. `SordM5` exposes no `save_state`/`load_state` path at all (unlike the chips it wraps, which do). Add a real machine snapshot (the Z80/TMS9918/SN76489 already have save/load; the CTC and machine RAM/keyboard/joystick/phase counters need adding) and serialise it. Status doc calls this "Snapshot deferred (shared family pattern)" (`outstanding-work.md:1100`). |
| **Dig Dug round-freeze root-cause** | **M** (trace-gated) | Boots to title → ROUND 01 but the round never spawns: sprites parked at `(194,0)`, score stuck `00`, the cart reads *no* I/O in the round (`drivability-assessment.md:300-348`). Ruled out: not CPU (RAM mutates), not sprite render (display + 16×16 enabled), not interrupt delivery (~16 IM2 acks/frame at boot *and* in the freeze). Gated diagnostic at `machine-sord-m5/tests/digdug_freeze_probe.rs`. Needs a MAME execution trace of the same ROM to localise the `$754A` gate — explicitly stopped per anti-thrash. **Open question:** is this M5 wiring, or a shared-chip defect (e.g. the TMS9918 transparent-sprite collision gap, or sprite-register-mid-line) manifesting on this title? Resolve the trace first, then route the fix to machine-or-chip. |
| **Keyboard / joystick CI validation** | **S–M** | The keyboard model was rebuilt from MAME `sord/m5.cpp` (direct row ports `$30-$36`, active-high, no strobe; `$37` joystick; A3 mirror) and the donor's `$30`-write/`$40`-read scheme proven fiction by an `io_trace` of the Monitor ROM (`drivability-assessment.md:309-330`, `lib.rs:360-372`). But the three confirming tests (`bios_boot.rs`, `digdug_freeze_probe.rs`, `keyboard_io_trace.rs`) are **all `#[ignore]`-gated on copyrighted ROMs** — verified: `cargo test -p machine-sord-m5` runs 0 / ignores 4. Only the 6 fixture-free unit tests (memory map, port routing, joystick byte) run in CI. Capture a one-shot trace/fixture (or a synthetic ROM that exercises the scan) so the keyboard map is regression-guarded without the BIOS. |
| **Reference distillation** | **S–M** | There is **no** primary-source distillation for the M5: `reference/by-system/sord-m5/` holds only `magazines.md` (a pointer page, Japanese/French press); there is no `knowledge/systems/sord-m5.md` and no datasheet/service-manual extract. The memory-map, I/O-map, and CTC-vector facts live only as prose in `machine-sord-m5/src/lib.rs` doc-comments and the status docs. Capture a `knowledge/systems/` distillation (memory map, I/O map, CTC channel-3 vectoring, keyboard matrix) citing the primary library, so the system facts are anchored rather than code-comment-only. Note the lib doc-comment self-flags the keyboard ports as "provisional / not yet trace-confirmed" while the code and drivability doc say they *were* trace-confirmed — reconcile that drift (`lib.rs:53-67` vs `drivability-assessment.md:309-330`). |

## Tier B — System core finish (memory map + region)

| Item | Effort | Notes |
|------|--------|-------|
| **Cartridge bank-switching mapper** | **M–L** | `mem_read` maps cart ROM **flat** into `$2000-$6FFF`, capped at 20 KB (`lib.rs:318-322`, `unwrap_or(0xFF)` past the end). Larger M5 carts and expansion carts that bank through the `$30` latch can't be represented. The `$30` *write* is acknowledged in-code as "the 64KBF memory-paging latch (expansion RAM, not modelled)" (`lib.rs:411`) and is currently a no-op. Add a cart mapper abstraction + the `$30` paging latch so banked/expansion carts load. **Needs runtime verification** of which M5 carts actually exceed 20 KB or bank — the commercial library is small and mostly fits the flat window; confirm against a cart dump survey before sizing this. |
| **64KBF / EM-5 RAM-expansion paging** | **M** | The `$30` latch (above) and cart RAM at `$8000-$BFFF` (`set_cart_ram_size`, capped 16 KB, `lib.rs:226-229,324-328,336-341`) are the building blocks for the M5's RAM-expansion units (64KBF, EM-5). The latch is unmodelled and `set_cart_ram_size` is never called from the runtime (`runtime.rs` constructs via `SordM5::new` only) — so expansion RAM is allocated-but-unreachable from the drivable surface. Wire the latch + a runtime path to enable expansion RAM. Lower priority — BASIC-I and the cart library run without it. |
| **Region / PAL end-to-end validation** | **S** | `M5Region::{Ntsc,Pal}` is plumbed through the machine (`lib.rs:174-186`: VDP region, PSG clock 3_579_545 vs 3_546_893, 262 vs 313 scanlines) and the runtime exposes `M5Ntsc`/`M5Pal` profiles (`profiles.rs:8-42`). The two frame-timing unit tests pass. But there is **no** validated PAL boot — the gated `bios_boot.rs` runs NTSC only. Confirm a PAL BIOS boots and times correctly once a PAL ROM image is available. **Needs runtime verification.** |

## Tier C — Audio / video core accuracy

**No M5-specific work.** Every audible/visible accuracy gap on the M5 is a
property of the shared `ti-tms9918` or `ti-sn76489` crate and is filed at the
chip level (the M5 is one of seven TMS9918 consumers and one of six SN76489
consumers). The M5-relevant items, for cross-reference only:

- TMS9918: sprite collision ignores transparent (colour-0) sprites
  (contradicts its own comment); mid-frame backdrop renders one frame late;
  sprite registers evaluated once per line. These affect M5 game logic exactly
  as they affect ColecoVision / SG-1000 / MSX. **Filed against the chip.**
- SN76489: period N=0 not clamped to N=1 (M5 uses the SN76489A 16-bit LFSR, so
  the BBC-Micro LFSR-variant defect does **not** apply here). **Filed against
  the chip.**

If the Dig Dug round-freeze (Tier A) resolves to a TMS9918 defect rather than M5
wiring, it folds into the chip work — that routing decision waits on the trace.

## Tier D — Preservation breadth

| Item | Effort | Notes |
|------|--------|-------|
| **Cassette load/save** | **L** | The M5's primary mass storage. The Monitor-ROM I/O trace already shows the ROM touching `$50` as "cassette/printer status" (`drivability-assessment.md:317-318`) — currently an unmapped read returning `0xFF` (`io_read` default arm, `lib.rs:373`). No cassette state machine, no tape-format reader/writer, no `MediaKind::Tape` slot in the profile (`profiles.rs:68-74` declares only a `Cartridge` slot). Add the cassette port decode + a tape format + a media slot. The bulk of the M5's *type-in / saved-program* preservation value rides on this. |
| **Centronics printer** | **M** | The trace shows `$40` driven as "Centronics data latch, write-only" (`lib.rs:362-364,411`, `drivability-assessment.md:317`); currently a write no-op. Model the parallel-printer port (data latch + strobe/status) for print-to-host. Niche. |
| **Cart-RAM (FALC-class) carts** | **M** | `set_cart_ram_size` exists (`lib.rs:226-229`) but, per Tier B, is never reached from the runtime and the carts that need it can't be selected. Once the paging latch + runtime wiring land, validate a FALC-class cart that uses `$8000-$BFFF` RAM. |
| **Cartridge container / `.m5` format handling** | **S–M** | The runtime loads a raw cart byte blob via `MediaKind::Cartridge` (`runtime.rs:168-184`) with no header parse, no size validation against the 20 KB window, and no container/format crate (contrast the NES `.ines` / C64 `.crt` format crates). Add an M5 cart-format crate (size/bank metadata, container handling) feeding the mapper. |
| **EM-5 / printer-plotter / disk peripherals** | **S each** | The M5's expansion peripherals (EM-5 expansion box, the SP-5 printer-plotter, the FD-5 floppy via the I/O box). Genuine completionist long tail; demand-gated, no curriculum pull. **Needs runtime verification** of which were ever dumped/usable. |

## Done as part of this plan (free, ~half a day)

Status-doc + code-comment reconciliation. The `machine-sord-m5/src/lib.rs`
module doc still describes the keyboard ports as "provisional (not yet
trace-confirmed)" and the keyboard section as a "10 rows × 8 columns" strobe
model (`lib.rs:53-67`), but the **code below it** implements the corrected
7-row direct-read MAME model (`NUM_KEY_ROWS = 7`, `lib.rs:95,360-372`) and the
drivability doc records the trace that confirmed it (`drivability-assessment.md:309-330`).
The doc-comment is stale relative to its own file. Correct it, and add the
three newly-named forward items the status docs lack: the **save-state
cold-boot stub**, the **unmodelled `$30` paging latch**, and the **absent
cassette/printer ports**.

## Recommended sequence (highest leverage first)

1. **Real save-state** (M) — the one outright defect on the drivable surface;
   restore currently throws away all live state. Highest leverage per week.
2. **Keyboard/joystick CI validation + reference distillation** (S–M + S–M) —
   cheap, lock in the trace-confirmed map so it can't regress, and anchor the
   system facts that currently live only in code comments.
3. **Dig Dug round-freeze trace** (M, trace-gated) — capture the MAME execution
   trace, localise the `$754A` gate, then route the fix to machine-or-chip.
   Bugs before features, but this one is *blocked on a trace*, not on code, so
   it runs in parallel with 1–2 rather than ahead of them.
4. **Cart bank-switching mapper + `$30` paging latch** (M–L + M) — the system
   memory-map finish; gate the sizing on a cart-dump survey first.
5. **Region/PAL validation** (S) — confirm the already-plumbed PAL path boots.
6. **Cassette** (L), then **Centronics printer** (M) — the preservation
   mid-tail; cassette carries the type-in/save value.
7. **Cart-format crate, cart-RAM carts, EM-5/SP-5/FD-5** (S–M / M / S each) —
   the completionist long tail, demand-gated.

## Key files

- Machine wiring: `crates/machine-sord-m5/src/lib.rs` (`tick_tstate` clock+CTC `:241-270`, `handle_bus` + RETI watcher + IntAck vector `:272-313`, `mem_read`/`mem_write` memory map `:315-344`, `io_read`/`io_write` port decode `:346-415`, keyboard/joystick `:475-512`, `$30` paging-latch no-op `:411`).
- Runtime: `crates/runtime-sord-m5/src/snapshot.rs` (the cold-boot stub), `crates/runtime-sord-m5/src/runtime.rs` (`rebuild_after_restore`/`rebuild_machine` `:118-140`, `load_media` `:168-184`, `run_until` `:186-228`), `crates/runtime-sord-m5/src/{input.rs,profiles.rs}`.
- Tests (all 4 ROM-dependent ones `#[ignore]`-gated; 6 fixture-free pass): `crates/machine-sord-m5/tests/{bios_boot,digdug_freeze_probe,keyboard_io_trace}.rs`, in-crate `#[cfg(test)]` in `lib.rs:577-715`, `runtime-sord-m5/src/input.rs` + `profiles.rs` unit tests.
- Shared chips (at-ceiling / chip-level filed): `crates/zilog-z80/`, `crates/ti-tms9918/src/lib.rs`, `crates/ti-sn76489/src/lib.rs`, `crates/zilog-z80-ctc/src/lib.rs`.
- Status docs: `docs/status/outstanding-work.md:1079-1102`, `docs/status/current-system-usability.md:77`, `docs/status/drivability-assessment.md:278,300-348,369-376,388`.
- Reference: `reference/by-system/sord-m5/magazines.md` (pointer only — no datasheet/distillation yet); MAME `sord/m5.cpp` is the I/O-map oracle cited throughout.

