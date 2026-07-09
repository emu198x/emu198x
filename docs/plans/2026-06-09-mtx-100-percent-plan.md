> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Memotech MTX to 100% — boot-complete core, peripheral breadth, media loading, shared-chip accuracy"
type: plan
date: 2026-06-09
system: docs/systems/memotech/mtx.md
basis: code-grounded survey of machine-/runtime-/emu198x-memotech-mtx + shared-chip findings (TMS9918, SN76489, Z80), live test run 2026-06-09
---

# Memotech MTX — road to 100%

What it would take to bring the Memotech MTX (MTX500 / MTX512) to feature- and
accuracy-complete, grounded in a code-level survey of the actual crates and tests.
The MTX is a 1983 UK Z80A machine sharing the TMS9918A VDP and SN76489 PSG with
the ColecoVision / Sord M5 family. The core boots; the gaps are breadth (media,
peripherals) and shared-chip accuracy that the MTX inherits.

## Executive summary

**The MTX is a fourth shape again: a boots-to-`Ready` core with the hard wiring
already done, but with almost no breadth around it and a save-state that is a
stub.** The machine layer (`crates/machine-memotech-mtx/src/lib.rs`, 626 lines)
is a competent fresh-write: it has the correct MEMU-derived port-`$00` paging
model, the corrected I/O map (SN76489 on `$06`, not the donor's `$03`), a
drive/sense keyboard with a full physical matrix, joysticks through the matrix,
and — the last boot blocker resolved — the VDP `/INT` routed through a real Z80
CTC at `$08-$0B`. The gated `boots_to_basic_ready` smoke test (requires the
copyrighted OS+BASIC+ASSEM 24 KB ROM, not in-tree) asserts the machine renders
the BASIC `Ready` prompt.

So unlike the C64 (VIC-II long pole) or NES (finished core), the MTX's CPU is
**at the ceiling already** (the `zilog-z80` core is Tom Harte 100% / FUSE-exact —
no MTX-specific CPU work), and the *machine wiring* is largely done. What is
missing is everything **around** the boot:

- **No media path at all.** There is no `.mtx` / `.run` tape/snapshot loader, no
  cassette in/out, no format crate (`crates/` has only machine/runtime/emu198x —
  no `format-memotech-*`). `load_media` is a no-op
  (`runtime.rs:175-177`), `media_slots` is empty (`profiles.rs:66`). A learner can
  watch it boot to BASIC but cannot load a program. This is the long pole.
- **Save-state is a stub.** `snapshot.rs` persists only `time`, `model_id` and
  `rom_bytes` — **not** RAM, VDP VRAM/registers, CPU registers, PSG, CTC, paging
  byte or keyboard state. Restore rebuilds a fresh machine and replays nothing, so
  it cannot resume a running session. The doc calls this "snapshot deferred"
  (`outstanding-work.md:727`); the code confirms it is a non-functional stub.
- **Two peripherals unwired:** cassette (`$03`) is a no-op
  (`lib.rs:334,347`); the Centronics printer (`$00` in / `$04`) returns a constant
  `$FF` and drops data (`lib.rs:331`; `$04` is not decoded at all).
- **Shared-chip accuracy debt** the MTX inherits: the TMS9918 sprite-collision
  defect, mid-frame backdrop latency and once-per-line sprite evaluation; the
  SN76489 period-N=0 clamp. These are filed against the chips, not re-filed here —
  but they bound the MTX's display/audio accuracy and are noted under "inherited".

The headline: **the expensive part (boot, paging, interrupt routing) is done; the
cheap-but-numerous part (media, save-state, peripherals) is almost entirely
undone.** True 100% is front-loaded onto breadth, not a core rewrite.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | media loading (`.mtx`/`.run` + tape), functional save-state, keyboard-matrix verification, doc-drift fix | **~2–3 weeks** |
| B — Peripheral + accuracy finish | cassette in/out, Centronics printer, CTC-timing verification, PSG clock-cross audit | **~1.5–2 weeks** |
| C — Shared-chip accuracy (inherited) | TMS9918 collision/backdrop/sprite-eval, SN76489 N=0 — filed against the chips; MTX is a consumer | **(tracked on chip plans)** |
| D — Preservation breadth | CP/M (FDX/HDX disk), RS128, SDX disk, additional paged ROMs (Noddy, SuperPascal), MTX user-group software formats | **~4–6 weeks** |

**True 100% of everything ≈ 8–11 weeks of MTX-specific work**, plus the shared
TMS9918/SN76489 accuracy work which is amortised across seven systems. The
launch-relevant slice (Tier A) is small and cheap: get programs loading and
sessions resuming. The preservation long tail (CP/M, FDX disk subsystem) is the
bulk and is genuinely MTX-distinctive — the FDX/HDX expansion is what made the
machine "serious-business" and is a whole disk-controller subsystem.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (do first)

| Item | Effort | Notes |
|------|--------|-------|
| **`.mtx` / `.run` program loading** | **L** | No loader exists. The MTX's native software format is the `.mtx` tape image (and `.run` direct-load). MEMU loads these by injecting into RAM at the OS load vectors. Needs a `format-memotech-mtx` crate + a `MediaKind` slot + `load_media` wiring (`runtime.rs:175` is a no-op today). Highest leverage: without it nothing but BASIC type-ins run. |
| **Functional save-state** | **M** | `snapshot.rs` is a stub — persists only time/model/rom, not machine state. Add full state: RAM blocks, `page_reg`, VDP (VRAM + registers + beam), PSG, CTC, keyboard, CPU registers, `master_clock`/`frame_count`. The chips already have `save_state`/`load_state` (per shared-chip findings); the machine must serialise them. Without this, the snapshot/restore surface silently loses everything. |
| **Keyboard-matrix verification** | **S–M** | `input.rs` `MtxKey::matrix()` has a *full* physical key→(row,bit) map (lines 89+), but the status doc and knowledge doc both claim the matrix is "not yet aligned to MEMU's grid" (`outstanding-work.md:722-724`, `knowledge/systems/memotech-mtx.md`). Code and docs disagree — verify the map against MEMU `kbd2.c` and either confirm-and-fix-docs or correct the map. Needs-runtime-verification: typing accuracy. |
| **Doc-drift fix** | **S** | The knowledge doc's "Interrupts — VDP via the Z80 CTC (**not yet wired**)" section is stale: the CTC *is* wired (`lib.rs:194-196,245-251`, `boot_trace.rs` asserts it). Also reconcile the keyboard-matrix claim above. Free, ~half a day. |

## Tier B — Peripheral + accuracy finish

| Item | Effort | Notes |
|------|--------|-------|
| **Cassette in/out** | **M** | `$03` OUT (cassette write) and `$03` IN (`snd_in3`) are no-ops returning a constant `0x03` (`lib.rs:334,347`). Real cassette load/save needs the pulse model + tape mount; pairs with the `.mtx` tape loader in Tier A but the *hardware* cassette path (vs. a fast RAM-inject loader) is separate. |
| **Centronics printer** | **S–M** | `$00` IN returns a constant `$FF` (no-printer status, `lib.rs:331`) and port `$04` (Centronics data) is not decoded at all (falls through `io_write`'s `_ => {}`). Wire a printer sink for completeness; low game impact. |
| **CTC interrupt-timing verification** | **S–M** | The CTC is wired (channel 0 fed by VDP `/INT`, IM2 vectoring, RETI daisy-chain release). Verify the *timing* — the VDP-`/INT`-edge → `ctc_trigger(0)` → IRQ latency and the IM2 vector — against MEMU under a known program, not just the boot smoke. Needs-runtime-verification. |
| **PSG clock-cross audit** | **S** | The SN76489 runs at 4 MHz (`lib.rs:60,153`) ticked once per CPU T-state (`lib.rs:182`) then downsampled in-crate to 48 kHz. Confirm the per-tick cadence matches the chip's internal ÷16 and the consumer assumptions in the SN76489 findings. Needs-runtime-verification on audio fidelity. |

## Tier C — Inherited shared-chip accuracy (filed against the chips)

The MTX is a **consumer** of `ti-tms9918` and `ti-sn76489`; these are tracked on
the chip plans, not re-filed here. They nonetheless bound MTX display/audio
accuracy:

- **TMS9918 sprite-collision defect** — coincidence flag ignores transparent
  (colour-0) sprites, contradicting its own comment (`ti-tms9918/src/lib.rs:782-809`).
  Affects MTX collision-based game logic.
- **TMS9918 mid-frame backdrop latency** — VR7 border changes render one frame
  late (`lib.rs:459-468`). Affects MTX raster/border effects.
- **TMS9918 once-per-line sprite evaluation** — mid-line VR1/table writes not
  reflected (`lib.rs:338-340`). Needs-runtime-verification of MTX impact.
- **SN76489 period-N=0 not clamped to N=1** — diverges from the documented
  hardware rule (`ti-sn76489/src/lib.rs:189-194`). Affects MTX tone/noise.
- **TMS9918 distillation doc gap** — no `knowledge/chips/` entry for the TMS9918
  despite it backing the MTX and six other systems; MTX VDP accuracy work is
  unanchored to a primary-source distillation.

(MTX is **not** affected by the SN76489 BBC-Micro LFSR-variant detune nor the
Game Gear stereo stub — it uses the SN76489A 16-bit LFSR, which is correct for it.)

## Tier D — Preservation breadth (back-loaded, MTX-distinctive)

| Item | Effort | Notes |
|------|--------|-------|
| **CP/M mode + FDX/HDX disk subsystem** | **XL** | The paging model already implements `RELCPMH=1` all-RAM CP/M mode (`lib.rs:268-279`), but there is no disk controller behind it. The FDX/HDX expansion (FDC + its own ROM) is what made the MTX a "serious-business" machine; a whole disk subsystem. The biggest single preservation item. |
| **SDX / CFX disk + ROM** | **L** | The later SDX (floppy) and CFX (CF-card) interfaces with their own ROMs and paged-ROM subpages. |
| **RS128 variant** | **M** | The RS128 (128 KB, RS232) is a distinct model; add a `Model` variant with its RAM/ROM/serial layout. |
| **Additional paged ROMs** | **S–M** | The machine already supports N×8 KB paged-ROM subpages (`lib.rs:146,284-292`); wire the optional Noddy and SuperPascal ROMs as selectable subpages so the full firmware suite the marketing named is reachable. |
| **PIO / DART (`$07`, `$0C-$0F`)** | **M** | Reads return open bus, writes are dropped (`lib.rs:338,351`). The Z80 PIO and DART (serial) are present on expanded machines; needed for serial peripherals and some FDX paths. |

## Done as part of this plan (free, ~half a day)

Doc-drift eradicated. The knowledge doc
(`knowledge/systems/memotech-mtx.md`) still carries an "Interrupts — VDP via the
Z80 CTC (**not yet wired**)" section that the code contradicts: the CTC **is**
wired (`lib.rs:194-196`, `boot_trace.rs` asserts channel 0 running with
interrupts enabled). The same doc and `outstanding-work.md:722-724` claim the
keyboard matrix is "not yet aligned to MEMU's grid", but `input.rs` carries a
full `MtxKey::matrix()` map — so the claim is at least overstated; flagged for
verification rather than asserted fixed. Snapshot status corrected from
"deferred" to "stub that persists no machine state". CTC-wired row in
`current-system-usability.md` is accurate.

## Recommended sequence (highest leverage first)

1. **`.mtx` / `.run` program loading** (L) — the one Tier-A gap that stops any
   real program running. Highest leverage per week; everything else assumes you
   can get software onto the machine.
2. **Functional save-state** (M) — the snapshot surface silently loses all state
   today; cheap relative to the embarrassment of a no-op restore.
3. **Doc-drift fix + keyboard-matrix verification** (S + S–M) — correct the CTC
   and matrix claims; verify typing against MEMU.
4. **Cassette in/out + Centronics** (M + S–M) — the two remaining stock
   peripherals; cassette pairs with the tape loader.
5. **CTC-timing + PSG clock-cross audit** (S–M + S) — verify the wired-but-
   untimed interrupt path and the audio cadence.
6. **Inherited chip accuracy** — track on the TMS9918 / SN76489 chip plans;
   re-verify MTX display/audio once the chip fixes land.
7. **RS128 + additional paged ROMs** (M + S–M) — model and firmware breadth.
8. **SDX/CFX disk → FDX/HDX CP/M subsystem (XL)** — the completionist long tail;
   the disk subsystem is the MTX's distinctive preservation frontier.

## Key files

- Machine wiring (paging, I/O map, CTC, joysticks): `crates/machine-memotech-mtx/src/lib.rs` (`resolve` `:260`, `io_read`/`io_write` `:329-353`, CTC routing `:194-196`, IntAck/RETI `:210-251`, cassette no-op `:334,347`, Centronics `:331`).
- Keyboard: `crates/machine-memotech-mtx/src/keyboard.rs` (drive/sense), `crates/machine-memotech-mtx/src/input.rs` (`MtxKey::matrix` full map — verify vs MEMU `kbd2.c`).
- Runtime (media no-op, save-state stub): `crates/runtime-memotech-mtx/src/runtime.rs:175` (`load_media`), `crates/runtime-memotech-mtx/src/snapshot.rs` (stub — no machine state), `crates/runtime-memotech-mtx/src/profiles.rs:66` (empty `media_slots`).
- Tests: `crates/machine-memotech-mtx/tests/boot_trace.rs` (gated `boots_to_basic_ready`, needs 24 KB OS+BASIC+ASSEM ROM), `tests/rom_boot.rs` (gated 16 KB smoke); 15 in-crate machine tests + 2 runtime tests pass, 2 integration tests `#[ignore]` on the copyrighted ROM.
- Shared chips (consumed, tracked separately): `crates/ti-tms9918/src/lib.rs`, `crates/ti-sn76489/src/lib.rs`, `crates/zilog-z80/` (at ceiling), `crates/zilog-z80-ctc/`.
- Knowledge/docs: `knowledge/systems/memotech-mtx.md` (stale CTC + keyboard claims), `docs/status/outstanding-work.md:676-728`.
- Reference: MEMU (`github.com/Memotech-Bill/MEMU`) `src/memu/{mem.c,memu.c,kbd2.c,snd.c}`; MAME `mtx.cpp`; `reference/by-system/memotech-mtx/` (magazines pointer only — no datasheet/hardware reference in-library yet).

