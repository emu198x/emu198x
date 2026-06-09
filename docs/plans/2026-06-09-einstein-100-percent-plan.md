---
title: "plan: Tatung Einstein TC-01 to 100% — disk boot, interrupt model, peripherals, preservation breadth"
type: plan
date: 2026-06-09
system: docs/systems/tatung/einstein.md
basis: code-grounded survey of machine-/runtime-/emu198x-tatung-einstein + ti-tms9918 + gi-ay-3-8910 + western-digital-wd1770 with live test runs, plus the established shared-chip findings (TMS9918, AY-3-8910, Z80), 2026-06-09
---

# Tatung Einstein TC-01 — road to 100%

What it would take to bring the Einstein to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and tests, not doc prose.
The system doc (`docs/systems/tatung/einstein.md`) is unusually accurate — it
already names the Ctrl-BREAK disk-boot stall, the single-line interrupt
approximation, the CTC stub, and the deferred snapshot — so this plan is mostly
the forward view, with the doc-drift section noting the few places code and doc
diverge.

## Executive summary

**The Einstein is a fourth distinct shape: a clean chip stack that *boots its
built-in monitor but cannot yet load an OS from disk*.** Every chip it needs
already exists and is wired — Z80 (at ceiling), TMS9918A, AY-3-8910, WD1770 —
and the hard extraction work (correct I/O map, `$24` ROM-toggle, AY-port
keyboard, exact 4 MHz : 5.369 MHz VDP clock) is done and tested. What it lacks is
not a chip but **a faithful interrupt model**: the real Einstein vectors
keyboard / ADC / fire / VDP / CTC interrupts through a **Z80 daisy chain**, and
the machine approximates that with a single `kbd_int_pending` line
(`machine-tatung-einstein/src/lib.rs:186-187,284,322-327`). That approximation
boots the MOS to `Ready` but stalls the **Ctrl-BREAK disk boot** — the keyboard
interrupt services once and the FDC is then never touched
(`docs/systems/tatung/einstein.md:32-43`; `tests/disk_boot.rs:9-13`).

**The long pole is the daisy-chain interrupt controller + CTC, because that —
not the WD1770 — is what stands between "boots the monitor" and "runs CP/M and
the game library."** Disk *reading* is already proven by tests
(`machine-tatung-einstein/src/lib.rs:901-1009`); the controller is not the
suspect.

Below that headline sit three smaller bodies of work:

- **A second, real defect surfaced by code: snapshot/rewind is hollow.** The
  runtime's `snapshot::encode`/`decode` serialise only `time` + `rom_bytes`
  (`runtime-tatung-einstein/src/snapshot.rs:10-44`); `decode` calls
  `rebuild_after_restore` → `Einstein::new`, throwing away all RAM, VDP, PSG,
  FDC and CPU state. A "restore" silently cold-boots the machine. The runtime
  advertises this as working (`MachineCore::snapshot`/`restore` are implemented,
  not stubbed), so it is a correctness bug, not an absent feature. The blocker is
  that `western-digital-wd1770` carries no serde (`grep -i serde` on its
  `src/lib.rs` is empty), exactly as the doc's "Snapshot deferred" note records.

- **Runtime plumbing gaps the machine core already supports.** `EinsteinRegion`
  exists with both NTSC and PAL and is honoured by the core
  (`lib.rs:212-216`), but the runtime hardwires PAL (`runtime.rs:125`) with no
  selector. `load_media` is a no-op and `media_slots` is empty
  (`runtime.rs:161-163`; `profiles.rs:56`), so the host has **no way to insert a
  disk through the runtime** — only the machine's direct `insert_cpc_dsk`/
  `insert_disk` APIs reach it, which the headless harness and tests use but a UI
  cannot. `command` returns `UnsupportedOperation` (`runtime.rs:213-217`).

- **The shared-chip accuracy debts ride along.** The AY-3-8910 envelope/noise
  octave-too-fast bugs and the broken alternating-envelope shapes
  (gi-ay-3-8910 defects 1-4) and the TMS9918 transparent-sprite collision /
  mid-frame backdrop defects are chip-level and shared; they are filed against
  the chip crates, not re-filed here. The Einstein inherits all of them.

So "100% Einstein" is **the daisy-chain/CTC interrupt model (the long pole) +
two system bugs (hollow snapshot, hardwired region/no media slot) + peripheral
and preservation breadth**, on top of shared-chip fixes owned elsewhere. There
is no CPU work (Z80 at ceiling) and no new chip core to write.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | Z80 daisy-chain interrupt model + CTC channels → Ctrl-BREAK disk boot; runtime media slot + region selector; full snapshot/rewind (needs WD1770 serde) | **~4-6 weeks** |
| B — System-accuracy finish | CTC channel timing for disk-loaded software; VDP/CPU sub-dot phase exactness audit; AY tick-cadence verification against MAME at the machine level | **~2-3 weeks** |
| C — reSID-equivalent / chip fidelity | inherited: AY envelope+noise rate fix, alternating/hold envelope shapes (gi-ay-3-8910); TMS9918 collision + backdrop (ti-tms9918) — *owned by chip-crate issues, listed for traceability only* | **— (chip-owned)** |
| D — Preservation breadth | cassette load, printer/RS-232 (8251), second floppy, raw-MFM/`LOST DATA` FDC timing, write-back to host image, DSK-parser extraction to a format crate | **~4-6 weeks** |

**True 100% of everything ≈ 10-15 weeks of Einstein-specific work**, plus the
shared-chip fixes. It is **front-loaded onto the interrupt model**: Tier A's
daisy-chain rewrite is both the hardest single item and the gate on the entire
disk-software library, so almost all "feels finished" value lives there. The
launch-relevant slice (boot-to-monitor + keyboard) already ships; the next
meaningful jump is the whole of Tier A as one arc.

Effort key: **S** = hours · **M** = a few days · **L** = 1-2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (the long pole lives here)

| Item | Effort | Notes |
|------|--------|-------|
| **Z80 daisy-chain interrupt model** | **L-XL** | Replace the single `kbd_int_pending` line (`lib.rs:186-187,284,322-327`) with a real prioritised daisy chain across keyboard / ADC / fire / VDP / CTC, supplying per-device IM 2 vectors instead of the hardwired `$F7` (`lib.rs:322-327`). This is what unblocks Ctrl-BREAK disk boot: today the keyboard ISR services once and the FDC is never touched (`docs/systems/tatung/einstein.md:32-43`). The clearest **system blocker** on the road to 100%. Pair with a CPU-PC trace of where the boot parks (the doc names this as the missing diagnostic). |
| **Z80 CTC channels** | **M-L** | Channel 0 is a single read/write register stub at `$28` (`lib.rs:189,401,435`); the real CTC has 4 channels with down-counters, prescalers and interrupt generation. The MOS boots under IM 1 so the monitor doesn't need it, but disk-loaded software and very likely the Ctrl-BREAK path do (`docs/systems/tatung/einstein.md:44-46`). A CTC crate may exist in the fleet to reuse; if not, this is a focused chip wiring. |
| **Ctrl-BREAK disk boot end-to-end** | **M** | The integration target: with the daisy chain + CTC in place, drive `set_control(true)` + `press_key(0,0)` and confirm the OS loads off a real `.dsk`. Convert the ignored `disk_boot.rs` into a green end-to-end boot test gated on an asset (the image is now in hand per the status doc). |
| **Runtime media slot + `load_media`** | **M** | `media_slots` is empty and `load_media` is a no-op (`profiles.rs:56`; `runtime.rs:161-163`), so a host cannot insert a disk through the runtime — only the machine's direct `insert_cpc_dsk` reaches it. Add a disk `MediaSlot`, route `load_media` to `insert_cpc_dsk`, and surface eject. Required for any non-test consumer to boot from disk. |
| **Region selector (NTSC/PAL)** | **S** | The core fully supports both regions (`lib.rs:212-216`) but the runtime hardwires `EinsteinRegion::Pal` (`runtime.rs:125`) and `profiles.rs` only declares one PAL `Model`. Plumb region through `Model`/profile + a CLI flag, mirroring the C64/NES region pattern. The Einstein was a PAL machine, so this is low-priority correctness, not a library blocker. |
| **Full snapshot / rewind** | **M** (blocked on WD1770 serde **M**) | `snapshot::{encode,decode}` capture only time + ROM (`snapshot.rs:10-44`); restore cold-boots via `Einstein::new`. Serialise RAM, `rom_paged_in`, keyboard/modifier/fire state, ADC, CTC, and the Z80/VDP/PSG/FDC. Gated on adding serde to `western-digital-wd1770` (no serde today) — the same blocker the doc records as "Snapshot deferred." Until then, advertise snapshot as unsupported rather than silently lossy. |

## Tier B — System-accuracy finish

| Item | Effort | Notes |
|------|--------|-------|
| **CTC channel timing for disk-loaded software** | **M** | Once CTC channels exist (Tier A), validate their down-counter/prescaler timing against MAME for the titles that program them. Listed separately because Tier A only needs CTC *present*; this is *exact*. |
| **VDP/CPU sub-dot phase exactness** | **M** | The exact 5.369318 MHz : 4 MHz accumulator is correct and unit-tested (`lib.rs:843-860`), but there is no machine-level VDP/CPU phase oracle — the doc flags "sub-dot VDP/CPU phase exactness" as the distance to full cycle-accuracy (`einstein.md:86-87`). Build a comparator only if a raster-timed Einstein title demands it; **needs runtime verification** that any real software is sensitive. |
| **AY tick-cadence machine-level check** | **S** | The core ticks the AY every other T-state for the 2 MHz CPU÷2 clock (`lib.rs:278-281`); the established AY finding confirms the per-machine cadence is right and the bugs are chip-internal. Add a machine-level assertion that the AY sees its true input clock so a future refactor can't silently regress it. |

## Tier C — Chip fidelity (inherited, owned by chip-crate issues)

These are **not Einstein issues** — they are shared-chip defects already
established and filed against the chip crates. Listed here only so the Einstein's
road-to-100% is honest about what its players will hear and see:

- **AY-3-8910 envelope runs an octave too fast, noise an octave too bright, and
  alternating (shapes 10/14) + continue-hold (11/13) envelope shapes are broken**
  (gi-ay-3-8910 defects 1-4). Every Einstein title using AY envelopes or noise
  is affected. Owned by the `gi-ay-3-8910` crate issues.
- **TMS9918 transparent-sprite collision flag never fires, and mid-frame
  backdrop (VR7) changes land one frame late** (ti-tms9918 items 1-2). Affects
  collision-based game logic and any raster-bar border effect. Owned by the
  `ti-tms9918` crate issues.

No Einstein-specific work; do not re-file.

## Tier D — Preservation breadth

| Item | Effort | Notes |
|------|--------|-------|
| **Cassette load** | **M** | Cassette port unwired (`einstein.md:47,100`). The Einstein shipped software on tape as well as disk; add the tape-input path. |
| **Printer / RS-232 (Intel 8251)** | **M** | The profile summary names an Intel 8251 USART (`profiles.rs:49`) but no 8251 is wired in the machine — printer and serial are absent (`einstein.md:47,99-102`). A focused USART + port wiring. |
| **Second floppy drive** | **S** | The WD1770 supports drives 0-3 and `$23` selects among them (`lib.rs:427-434`), but only one is ever populated in practice. Expose a second mountable drive through the runtime media slots. |
| **Write-back to host image** | **M** | `start_write_track` is a no-op and writes are not persisted to the host `.dsk` (`western-digital-wd1770/src/lib.rs:580-581`). For CP/M users who save files, add a write-back/flush path on the WD1770 write commands, riding the same decision as the C64 disk-save write-back. |
| **Raw-MFM / `LOST DATA` FDC timing** | **L** | The WD1770 uses a relaxed cycle-countdown model with a synthesised INDEX pulse, not bit-cell MFM, so `LOST DATA` and copy-protection timing are absent (`einstein.md:83-84`). Preservation-grade only. |
| **DSK parser → format crate** | **S-M** | `parse_cpc_dsk` is inline in `machine-tatung-einstein/src/lib.rs:638-742`, duplicating DSK-container logic that `format-amstrad-dsk` already implements for the CPC/Spectrum+3 — but that crate is bound to `nec-upd765a`, not the WD1770, so it cannot be reused as-is. Extract the Einstein's WD1770-targeted DSK reader into a shared `format-*` crate (or generalise `format-amstrad-dsk` over the controller) so the two parsers don't drift. Tidy, not a blocker. |

## Done as part of this plan (free, ~half a day)

System-doc and reference-layer touch-ups surfaced while surveying:

- **Snapshot honesty.** The doc says "Snapshot deferred" but the runtime *ships*
  a `snapshot`/`restore` that silently cold-boots on restore
  (`snapshot.rs:10-44`). Recorded here as a **bug**, not a deferral — restore
  must either be lossless or report unsupported.
- **No reference-layer / knowledge distillation for the Einstein.** The primary
  library has only `reference/by-system/tatung-einstein/magazines.md` — no
  datasheet, service manual, or hardware distillation — and there is no
  `knowledge/systems/einstein` doc. The entire system spec is sourced from
  MAME's `tatung/einstein.cpp` (`lib.rs:38-39`; `einstein.md:66-69`). Accuracy
  work here is unanchored to a primary-source distillation; flag for the
  knowledge layer. (Mirrors the TMS9918 reference-gap finding.)
- **Profile names an Intel 8251 that isn't wired** (`profiles.rs:49` vs absent
  USART in `lib.rs`) — corrected to "planned" in the doc.

## Recommended sequence (highest leverage first)

1. **Z80 daisy-chain interrupt model** (L-XL) — the one item that converts the
   Einstein from "boots its monitor" to "can load software." Everything in the
   disk-software library is behind it. Capture the boot-park PC trace first
   (the doc names this as the missing diagnostic), then build the chain.
2. **Z80 CTC channels** (M-L) — the daisy chain's most important client and the
   likely co-dependency of the Ctrl-BREAK path. Do it alongside step 1.
3. **Ctrl-BREAK disk boot end-to-end** (M) — the integration proof; turn the
   ignored `disk_boot.rs` into a green boot test.
4. **Runtime media slot + region selector** (M + S) — the cheapest way to make
   disk boot reachable from a real host, not just the machine API.
5. **WD1770 serde → full snapshot/rewind** (M + M) — fix the hollow-restore bug;
   it needs the controller serde first.
6. **CTC timing + VDP/CPU phase audit** (M + M) — system-accuracy finish, gated
   on real software actually being sensitive (verify before investing).
7. **Cassette, printer/8251, second floppy** (M/M/S) — peripheral breadth.
8. **Write-back, raw-MFM FDC, DSK extraction** (M/L/S-M) — the preservation tail.

Throughout, the **shared-chip fixes** (AY envelope/noise/shape, TMS9918
collision/backdrop) land in the chip crates on their own schedule and lift the
Einstein for free.

## Key files

- CPU (at ceiling, no work): `crates/zilog-z80/` — Tom Harte 100%, FUSE per-cycle.
- Machine wiring (the bulk of the work): `crates/machine-tatung-einstein/src/lib.rs`
  — interrupt line (`:186-187,284,322-327`), CTC stub (`:189,401,435`), I/O map
  (`:361-440`), AY-keyboard (`:346-359`), ROM toggle (`:332-344,397-400,424`),
  VDP accumulator (`:267-287,843-860`), inline DSK parser (`:638-742`), ADC0844
  (`:118-165`).
- Runtime plumbing: `crates/runtime-tatung-einstein/src/runtime.rs` (hardwired
  PAL `:125`, no-op `load_media` `:161-163`, `command` unsupported `:213-217`),
  `src/snapshot.rs:10-44` (hollow snapshot), `src/profiles.rs:49,56` (single PAL
  model, empty media slots, 8251 named-but-absent), `src/input.rs`.
- Chips (Einstein-relevant gaps owned elsewhere): `crates/ti-tms9918/src/lib.rs`,
  `crates/gi-ay-3-8910/src/lib.rs`, `crates/western-digital-wd1770/src/lib.rs`
  (no serde; `start_write_track` no-op `:580-581`).
- Tests: `crates/machine-tatung-einstein/tests/{bios_boot,disk_boot,keyboard_type}.rs`
  (all `#[ignore]`, gated on a real ROM/disk); 23 in-crate unit tests pass.
- Reference: MAME `tatung/einstein.cpp` (sole spec source); the unrelated
  `crates/format-amstrad-dsk` (upd765a-bound DSK parser).

