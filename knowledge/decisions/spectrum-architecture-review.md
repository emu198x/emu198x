# Decision: Spectrum architecture review — tighten the seams, not the spine

**Date:** 2026-05-18, polish pass 2026-05-19, Phase 2 close-out 2026-05-20, Float48K + Float128K un-gate 2026-05-20
**Status:** Phase 2 landed — Seams 1, 2, 3, 4, 5 + UlaRevision rename complete. Float48K and Float128K strict assertions un-gated. Open threads (5C-vs-6C HSync, Smith Y/U/V palette, 3-level beeper LUT, IF2 keyboard-matrix) explicitly deferred — no test-harness blockers remain.

## What this is

A targeted review of the Spectrum implementation against the load-bearing decisions already in place ([ULA-drives model](ula-drives-model.md), [No Bus trait](no-bus-trait.md), [Half-cycle signals](half-cycle-signals.md), [CPU bus interface](cpu-bus-interface.md), [SpectrumDriver](spectrum-driver.md), [Within-family layering](within-family-layering.md), [Runtime internal shape](runtime-internal-shape.md)). The Spectrum is the October-public deliverable per [`october-catalogue.md`](october-catalogue.md), and the 101-entry catalogue has been catching everything it's structurally able to catch. This review names **five seams** that the catalogue *cannot* catch by construction: silently-locked-in wrong behaviour, lost host input, volatile state that doesn't survive snapshot restore, oracle integrity, and standing boot-invariant assertions.

The spine stays. The seams need work.

This document mirrors [`amiga-architecture-review.md`](amiga-architecture-review.md). The structure is deliberately the same so the two reviews can be read side by side. The seam findings are silicon-grounded against Chris Smith's *The ZX Spectrum ULA: How to design a microcomputer* (all 24 chapters distilled in the Reference library) plus six other Spectrum references.

## What we are *not* changing

These decisions are load-bearing, validated by Z80 100% Tom Harte, ZEXDOC/ZEXALL pass, Signal Part 3, and the 101-entry catalogue running SNAP-PASS in 94 minutes. Nothing in this review revisits them:

- **Master oscillator drives the loop.** `SpectrumDriver::tick_one_halfcycle` in `crates/common-sinclair-zx-spectrum/src/driver.rs:184` ticks the ULA every even half-cycle and gates the CPU on `cpu_clock_active()`. `hc` is the only time counter.
- **Pin-level CPU bus interface, no Bus trait.** `Z80::bus_request` in `crates/zilog-z80/src/z80.rs:355` returns `BusOp` from observing `mreq`/`iorq`/`rd`/`wr` edges. The machine layer dispatches via a 6-arm match (`common-sinclair-zx-spectrum-48k-class/src/core.rs:351-372`).
- **Half-cycle signal granularity.** Z80 `Phase` enum carries `T1Rise`/`T1Fall` for every M-cycle. Contention decisions happen between CPU edges.
- **ULA as trait, one implementation per variant.** Ferranti, Sinclair 7K, Amstrad 40077, Timex SCLD, Pentagon, Scorpion all implement `Ula`. Shared rendering in `UlaEngine`, variant-specific contention in the wrapper.
- **Within-family layering, phantom-typed variants.** Eight October-scope variants type-distinct via marker structs; snapshots cannot cross variants.

If the review below appears to require revisiting any of these, the review is wrong and the decision wins.

## The five seams

### Seam 1 — UlaEngine shifter pipeline depth

**Current state.** `crates/common-sinclair-zx-spectrum/src/ula_engine.rs:299-480` carries a single bitmap latch (`data_latch`) and a single attribute latch (`attr_latch`), with one transfer trigger at `(p & 0x07) == 4` (line 334). VRAM fetches happen at phases 8, 10, 12, 14 within each 16-pixel cycle.

**Friction observed.** Our 48K `ferranti-ula-6c001e` is 4 T-states late at first display-byte fetch (14340 vs FUSE's 14338). The 128K is off by the same +4 (14368 vs 14364). The Float48K probe at `crates/machine-sinclair-zx-spectrum-48k/tests/float_bus.rs` fails its strict T-state assertion. The investigation at [`ula-first-fetch-tstate-offset.md`](ula-first-fetch-tstate-offset.md) captures the running history.

**Diagnosis.** The +4 offset isn't one bug — it's three legitimate silicon-level taps on the *same* fetch event, conflated by a single-stage latch model. Smith Chapter 12 Figure 12-2 (silicon-level, gate-traced) shows the ULA uses a **two-stage double buffer** per stream — `memory → DataLatch → ShiftRegister` — with two distinct clocking signals. The pipeline depth between fetch and pixel emission is one full character cell (4 T-states):

| T-state | Event | Sample point |
|---|---|---|
| **14336** | First VRAM fetch — `DataLatch` fires on scan 0 | Smith Chapter 21 p. 227 canonical |
| **14338** | Fetched byte appears on the ULA data bus | Float48K `IN A,($FF)` probe |
| **14340** | First `SLoad` fires; pixel emission begins | Our current model |

Our model is correct at the visible-pixel tap (14340) but exposes `floating_bus()` at the same point — when it should expose the byte from the DataLatch point onwards (14336 → 14340 window). Smith Chapter 12 establishes this via `VidEN = /Border delayed by one character-cell`: `SLoad` is gated on `/VidEN`, not `/Border`, so the first transfer to the shift register lags the first DataLatch by exactly one character cell.

**Proposed change.** Add `data_latch_pending` alongside `data_latch` and `data_reg`. Fetch into `data_latch_pending`. Promote `data_latch_pending → data_latch` at the `DataLatch` trigger; promote `data_latch → data_reg` at the `SLoad` trigger. Mirror for `attr_latch_pending → attr_latch → attr_reg`. Shift `IDLE_TABLE` and `MEM_TABLE` four entries left and move `fetch_start` from 8 to 4 in `CONFIG_48K`. Validate against Float48K strict mode (must print 14338) and the existing 48K test suite; repeat for `CONFIG_128K` (target 14364).

**Silicon evidence (Chris Smith, distilled in the Reference library):**

- *Chapter 12* — two-stage double buffer (`memory → DataLatch → ShiftRegister`), three bytes in flight per stream, the load-bearing source.
- *Chapter 13* — 8-phase cycle is **continuously fetching** (two RAS-CAS pairs per character cell), not "4 fetch + 4 idle". Largest concrete refinement to the fix: our `MEM_TABLE`/`IDLE_TABLE` should reflect continuous fetch.
- *Chapter 14* — verbatim derivations of `DataLatch`, `SLoad`, `AttrLatch`, `AOLatch`, `VidEN`, `VidC3`. **AOLatch is un-gated by VidEN**, giving 8-pixel border-write granularity (cosmetic gap for the catalogue, not a Seam 1 blocker).
- *Chapters 11 + 21* — `/INT = VSync + V2 + V1 + V0 + C8 + C7 + C6`, verbatim 14336 derivation (64 × 224 T-states). /INT is **pure combinational** — must not be latched in the engine.
- *Chapter 18* — canonical contention gate equation `CLKWAIT = (C3 OR C2) AND /Border AND A14 AND /A15 AND /MREQT23`. Our current `!cpu_mreq && z80_clock_high` is approximately equivalent via the MREQ-falling-edge approximation; if first-fetch validation post-fix shows residual one-T-state errors, add the explicit MREQT23 term.
- *Chapter 19* — `floating_bus()` semantics: `IN A,($FF)` returns the pending-latch byte (just-fetched), NOT the byte already promoted to the shift register. Our existing `bus_data` field is silicon-correct upstream of the fix; preserve this in the two-stage refactor.
- *Chapter 23* — does NOT adjudicate the 14338-vs-14339 sample point (the architecture review previously implied it would; correction stands). Float48K (14338) remains the only authority.

**Cross-validation references:**

- **HDL.** MiSTer `ula.sv` and ZX-Uno `ula.v` (catalogued in `zx-spectrum-clones-and-fpga-replicas.md`) are open, independently authored, and the natural cross-check for the two-stage shifter shape before implementation.
- **Test corpus.** Grussu names Arkanoid, Cobra, Short Circuit, Sidewize as canonical floating-bus titles. None are currently in the 101-entry catalogue. Adding one to the SOLID 48K catalogue post-fix gives a real-software regression bench independent of Float48K.

**Scope.** ~50 lines in `ula_engine.rs`. No public API change, no machine-layer churn. Pre-flight checklist documented in `ula-first-fetch-tstate-offset.md`.

**Why this matters for other systems.** The same FUSE-style phase table applies to every Spectrum-family ULA. Get the engine right once; every variant inherits.

**Implementation status (2026-05-19).** The structural fix landed in commits `0660521` (two-stage shifter + table shifts) and `fbc5938` (AOLatch border granularity). `data_latch_pending` / `attr_latch_pending` / `data_latch_pending2` slots added; `MEM_TABLE` and `IDLE_TABLE` shifted four phases left; `fetch_start: 4` / `fetch_end: 260` in all four ULA configs; `border_aolatch` samples `border` every 8 pixels at `(p & 0x07) == 4` un-gated by VidEN. `FRAME_ROUTING_VERSION` bumped 1 → 3 (re-capture wave gated by Seam 4).

The Float48K strict assertion is **not yet enabled in CI**. The engine-side timing is now correct (first fetch at T-14338 per Smith Ch 21), but the test harness's RST 16 print capture cannot read the probe's iteration digits — BASIC's PRINT-FP number formatter bypasses RST 16, and the PR-ALL alternate capture mode mashes digits with AT/INK control bytes. Un-gating the strict assertion needs a capture mode that tracks the ROM editor's control-argument state machine. Tracked as a separate piece of work — engine-level Seam 1 is closed.

### Seam 2 — Host input → peripheral routing

**Current state.** `crates/runtime-sinclair-zx-spectrum/src/input.rs:26-35` handles `InputEvent::Key` and silently ignores everything else. The Kempston joystick is wired through `kempston: KempstonJoystick` on every 48K-class and 128K-class machine core, with `claims_port($1F)` returning the state byte when `attached = true`.

**Friction observed.** A user plugging a gamepad into a SOLID-scope Spectrum sees nothing. `KempstonJoystick::attached` defaults to false, and no host event ever flips it or sets `state`. The catalogue does not synthesize gamepad input, so this is invisible to the regression bench.

**Diagnosis.** The runtime input layer treats input as keyboard-only. Joystick (and future: mouse, IF2 keyboard) routing is unimplemented, not deferred-and-typed.

**Proposed change.** Extend `apply_input_event` to handle `InputEvent::Joystick { … }` (or whatever the host-boundary type is in `emu198x-shell`) by:

1. Flipping `kempston.attached = true` when a gamepad first emits a directional or button event.
2. Mapping the eight Kempston bits (right, left, down, up, fire, plus three reserved) from host axis/button events.
3. A typed extension trait `ApplyKempstonEvent` on the 48K-class and 128K-class cores mirroring the existing `ApplyInputEvent` (`crates/machine-sinclair-zx-spectrum-48k/src/machine.rs:29-47`).

The Amstrad-class deliberately has no Kempston field (rear-connector pinout broke in '87, see [`spectrum-joystick-architecture.md`](spectrum-joystick-architecture.md)) — `ApplyKempstonEvent` must not exist for those cores, enforced by the trait bound.

**Reference for the deferred Sinclair Interface 2.** When IF2 keyboard-matrix translation eventually lands, the canonical port-to-key mapping table is in Grussu (`~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/spectrumpedia-volume-1-english.txt:4796-4818`): port 1 = keys 6/7/8/9/0; port 2 = keys 1/2/3/4/5. Quoted verbatim in `zx-spectrum-variants-grussu.md`'s peripheral surface table.

**Scope.** ~30 lines in `runtime-sinclair-zx-spectrum/src/input.rs`, ~10 lines in each of two class crates. No public API churn.

**Why this matters for other systems.** Every system in the roadmap accepts joystick-class input through the same `emu198x-shell::InputEvent` enum. Fixing the routing pattern here sets the shape for C64 (1351 mouse, Competition Pro joystick) and NES (controller pads).

### Seam 3 — Volatile state survival across snapshot restore

**Current state.** Three classes of state live outside the serde graph today:

- **`&'static UlaConfig`** in `UlaEngine` (`crates/common-sinclair-zx-spectrum/src/ula_engine.rs:201-203`). Serde-skipped; defaults to `CONFIG_48K`. Each variant ULA exposes `reattach_config()` to reinstall the correct one.
- **`disks: [Option<DiskImage>; 4]`** in `Upd765a` (`crates/nec-upd765a/src/lib.rs:212`). `#[serde(skip)]` with no rehydration path. A +3 snapshot taken with a disk mounted comes back with no disk.
- **`reread_key`, `reread_count`, `read_id_index`** in `Upd765a` (`crates/nec-upd765a/src/lib.rs:220-242`). `#[serde(default)]`; resets to zero on restore. Marginal-encoding variation and rotational position both lost.

The 48K-class, 128K-class, and Amstrad-class composition crates all expose `restore_volatile_refs()` that calls `z80.rehydrate_walker_sequence()` and `ula.reattach_config()`. The runtime wires this through `after_restore()` for all eight SOLID variants in `crates/runtime-sinclair-zx-spectrum/src/variants.rs`.

**Friction observed.** The ULA-config path is the only one with a documented rehydration contract. The FDC disk image silently disappears. The marginal state silently resets. Neither is caught by `run_spectrum_entry_with_snapshot_check` if the catalogue entry happens not to exercise that path between save and restore — and the +3 disk catalogue entries do round-trip with disks mounted, so the only reason this hasn't bitten yet is that the runtime layer happens to re-insert the disk before the round-trip replays.

**Diagnosis.** There's a typed rehydration contract for the ULA but no contract — or test — for the FDC. The pattern needs to generalise: every chip that holds state outside the serde graph must declare a typed `after_restore()` hook that the machine layer calls.

**Proposed change.**

1. Add `fn after_restore(&mut self, …)` to the chip surface for any chip with `#[serde(skip)]` fields whose value isn't reconstructible from defaults. Upd765a's `after_restore` takes the disk images the runtime layer is holding.
2. The runtime's per-variant `after_restore()` (already wired for the eight SOLID variants) calls the chip's `after_restore` in addition to `restore_volatile_refs`.
3. A `#[serde(skip)]` audit lint: assert that every skipped field on a serde struct has either (a) a `Default` that produces correct behaviour, or (b) a code reference to the function that rehydrates it. The audit lives as a `cargo test` in `common-sinclair-zx-spectrum`, not a proc-macro.

**Scope.** ~80 lines spread across `nec-upd765a`, the Amstrad-class runtime wiring, and a new audit test. No public API churn for the eight SOLID variants — the FDC's new method is additive.

**Note on Pentagon/Scorpion.** Neither implements `restore_volatile_refs()`. Their ULAs silently fall back to `CONFIG_48K` after deserialise, breaking timing. Real, but Pentagon and Scorpion are not in the SOLID 8 and not in `runtime-sinclair-zx-spectrum` — out of scope for October-public, post-October concern. Track in a known-limitations doc.

### Seam 4 — Catalogue oracle integrity

**Current state.** `emu198x-catalogue` captures frame-hash + audio-hash per entry. The expected hashes are committed alongside the entry manifest and compared on every catalogue run. This is a powerful regression bench — 101 entries, 94 minutes wall — and it has caught real drift.

**Friction observed.** The captured hash *is* the oracle. If the hash is captured against wrong behaviour, the catalogue locks the bug in. This bit us once already: the 2026-05-15 investigation of "128K silence" discovered that the AY-3-8912 output had never been mixed into the audio sink on 128K-class or Amstrad-class machines. Every 128K-family audio hash had been captured as silence and was passing forever. The architectural fix landed (`crates/common-sinclair-zx-spectrum-128k-class/src/core.rs:360-362`, `crates/common-sinclair-zx-spectrum-amstrad-class/src/core.rs:424-426` both now call `mix_ay_into_audio` end-of-frame), but the hashes still need re-capturing and the SOLID status tracker drifted from the code in the interim — the doc says open, the code says fixed.

**Diagnosis.** Two related gaps:

1. **No re-capture trigger when audio routing changes.** A change to the audio path doesn't invalidate the existing hashes; the catalogue just keeps comparing against the pre-fix oracle.
2. **No staleness signal on the SOLID status doc.** The doc carries dates ("As of 2026-05-15") but nothing forces an audit when the underlying code changes. Drift surfaces only when someone happens to re-read the doc.

**Proposed change.**

1. Add an `audio_routing_version: u32` constant in `common-sinclair-zx-spectrum::audio` (and equivalents for the AY mix path). Bump when audio routing semantics change. The catalogue carries the version it was captured against; a mismatch fails loud with an instruction to re-capture.
2. The same pattern for `frame_routing_version` once the ULA shifter (Seam 1) lands — pipeline-depth changes will alter every catalogue frame-hash that touches active video. Use the version bump to gate the re-capture.
3. A `knowledge/decisions/spectrum-architecture-review.md` (this doc) plus a `knowledge/log.md` entry naming the routing-version bumps and the re-capture wave, so future sessions can trace which hashes correspond to which routing.
4. Refresh `knowledge/systems/spectrum/solid-status.md` §1 against the current code in the same commit as the AY routing-version bump.

**Scope.** ~30 lines in the catalogue harness plus the version constants. The audit is one-shot work tied to closing the AY thread and Seam 1.

**Concrete catalogue test case (Reference library, 2026-05-18).** Grussu names **R-Type** as one of the very few 128-era titles that is deliberately beeper-only — useful as an *invariant* in the re-capture: R-Type's 128 audio hash must NOT change when the AY routing version bumps, because R-Type does not program the AY. Conversely, **RoboCop, Operation Wolf, Rainbow Islands, Bubble Bobble, Out Run** (the Jonathan Dunn / Tim Follin / Wally Beben titles already in the catalogue) MUST change. Source: `~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/zx-spectrum-variants-grussu.md` § AY-3-8912.

**Why this matters.** The catalogue is the strongest safety net we have for October. A safety net that silently captures bugs as expected behaviour is worse than no net — it gives a false-positive green bar. This is the only seam in this document that is a *process* fix rather than a code fix, and it's listed precisely because the code fixes (Seam 1, Seam 3) will require re-capture and the discipline needs to be in place before they land.

### Seam 5 — Per-variant boot invariants suite

**Current state.** The catalogue verifies title-by-title that 101 entries reach their expected end-state with matching hashes. Beyond that, the only timing-level invariants enforced in CI are the per-chip unit tests (Z80 Tom Harte, ULA contention tables, AY register file) and the Float48K probe (gated on `EMU198X_FLOAT48K_STRICT=1`).

**Friction observed.** Waypoint-level invariants — "INT asserts at scan 248 pixel 1 on 48K", "first display byte appears on bus at T=14338", "contention pattern matches the canonical table phase 0 starting at T=14335" — aren't asserted independently of the catalogue's end-state view. When a timing regression slips through that the catalogue happens not to surface, there's no second line of defence. The Float48K strict-mode assertion *is* a waypoint invariant, but it's gated and currently failing (which is what triggered this review).

**Diagnosis.** Same pattern as Amiga Seam 5: diagnostic examples are append-only. The Float48K probe is exactly the kind of waypoint test that should be CI-mandatory once the ULA pipeline depth lands, not env-var-gated.

**Proposed change.** Add `tests/boot_invariants.rs` to `runtime-sinclair-zx-spectrum`. One test per known waypoint, each per-variant where the variant differs:

- `int_asserts_at_canonical_t_state` — per variant (48K: scan 248 pixel 1; 128K: same; Pentagon: scan 256).
- `first_display_byte_on_bus_at_canonical_t_state` — Float48K strict, no env-var gate, must print 14338 on 48K-class and 14364 on 128K-class.
- `contention_table_matches_canonical_for_known_window` — assert the contention pattern over T=14335..14400 against the table in `knowledge/systems/spectrum/contention.md`.
- `paging_lock_persists_across_reset` — 128K-family + Amstrad-class.
- `kempston_attaches_on_first_gamepad_event` — once Seam 2 lands.

**Scope.** ~50 lines per variant class. Promoted from existing diagnostic examples and the Float48K probe one waypoint at a time.

## Verified non-issues

Recorded here because the audit examined them and they are not seams. Future sessions should not re-discover.

### `z80_iorq_prev` gated by `cpu_clock` in `track_z80_clock`

`crates/common-sinclair-zx-spectrum/src/ula_engine.rs:483-490` updates `z80_iorq_prev`, `z80_iorq_prev2`, `z80_mreq_prev`, and `z80_clock_high` only when `cpu_clock == true`. This is correct. When the ULA withholds the clock, the Z80's state machine does not advance, its output pins do not change, and `z80_clock_high` must freeze because the internal phase is not progressing. Updating prev during a gated cycle would store the same unchanged value. The contention logic correctly reads `(cpu_iorq || z80_iorq_prev)` and gets the right answer on the next gate cycle because both `cpu_iorq` (live) and `z80_iorq_prev` (frozen-from-last-tick) point at the same value.

### AY chip output not reaching the audio mixer on 128K-class / Amstrad-class

Reported in `knowledge/systems/spectrum/solid-status.md` §1 as of 2026-05-15. Fixed in code: `crates/common-sinclair-zx-spectrum-128k-class/src/core.rs:360-362` and `crates/common-sinclair-zx-spectrum-amstrad-class/src/core.rs:424-426` both call `mix_ay_into_audio` end-of-frame. Seam 4 closes the documentation drift.

### Pentagon/Scorpion serde fallback to `CONFIG_48K`

Real defect — `PentagonUla` and `ScorpionUla` don't expose `reattach_config()`, and `Pentagon128` / `ScorpionZs256` don't expose `restore_volatile_refs()`. Out of scope for October-public: these variants aren't in the SOLID 8 and aren't wired through `runtime-sinclair-zx-spectrum`. Captured in a known-limitations doc for post-October.

### Three near-identical contention wrappers

`crates/ferranti-ula-6c001e/src/lib.rs:81-112`, `crates/sinclair-ula-7k010e/src/lib.rs:57-89`, and (with the MREQ-only inversion) `crates/amstrad-ula-40077/src/lib.rs:59-88` carry textually similar contention blocks. No drift today. A future refactor could lift the gate into a per-variant policy in `UlaEngine`, but the duplication is small and clear; the cost of an abstraction over three call-sites is currently higher than the cost of keeping them in sync. Defer until a real divergence forces the issue.

### 5C versus 6C HSync start offset (Chapter 11)

Smith Chapter 11 Table 11-1 documents that the early 5Cxxx ULA starts HSync at horizontal counter 336 (front porch 2.29 μs) while the 6C001 starts at 344 (front porch 3.43 μs) — an 8-pixel-clock (4 T-state) difference. This is the silicon basis for the well-known "early Spectrum displays shifted left" cosmetic phenomenon. We do not currently model per-ULA-revision HSync timing; the single `CONFIG_48K` constants are 6C001 numbers. Deliberately deferred — no SOLID catalogue entry depends on the 4-T-state HSync shift. If TIN's "Bitmap Brothers vs Ocean cover detection" demoscene work ever becomes a catalogue priority, revisit.

### 14335 "late timing" is a Z80-die-batch dependency, not a board-issue dependency (Chapter 21)

Smith confirms that the 42 ns /INT-to-clock-rise lag responsible for the 14335 (late) vs 14336 (early) one-T-state discrepancy is measured on a 6C001E-7 (Issue 3+ ULA). The "intolerance" is on the Z80 side — specific Z80 die batches that have stricter setup-time requirements than the 80 ns Zilog datasheet figure. Implication: emulators should not key 14335 vs 14336 on board-issue; 14336 is the canonical value for every variant. 14335 would only be needed to reproduce a specific real-machine "warm Z80" corner case that does not affect any catalogue entry.

### TS2068 timing is not from Smith (Chapter 11)

Smith covers the unreleased Sinclair 6C011 NTSC ULA (264 lines, INT at scan 216), which Timex did not use. Our `CONFIG_TS2068` (`lines_per_frame: 262`, `int_scan: 224`) must be validated against Timex documentation, not Smith. Already correct in code; worth a comment near the constant to prevent future drift toward "Smith says 264".

### IOWAIT silicon bug in the 5C102 / "dead cockroach" workaround

Smith Chapter 18 p. 199-202 documents that the original 5C102 ULA shipped with a buggy IOWAIT equation: `/IOWAIT = /(C3+C2) + /(C2+C1)` instead of the intended `/IOWAIT = /(C3+C2) + /(/C3+C2+C1)`. The Issue 1 boards used the "dead cockroach" external IC to override this. Every later board (Issue 2 onward) re-routed internally so that I/O contention behaves like memory contention. Real Spectrum software therefore depends on memory-contention timing being applied to I/O cycles. Our current Ferranti and Sinclair gates (`ferranti-ula-6c001e/src/lib.rs:99-104`, `sinclair-ula-7k010e/src/lib.rs:77-82`) implement I/O contention as memory-contention with the additional `(cpu_iorq || z80_iorq_prev)` condition — semantically equivalent to the post-cockroach behaviour. Not a defect.

## Fidelity findings deferred beyond October

Real silicon-level fidelity gaps surfaced by the Smith distillation that are NOT seams in this review's sense — they don't affect the catalogue regression bench (palette mapping happens *after* the framebuffer hash) and they don't block October-public. Captured here so they're not re-discovered.

### Palette luminance equation deviates from BT.601 (Chapter 16)

Smith Chapter 16 documents that the Spectrum's Y equation is **`Y = 0.299R + 0.587G + 0.151B`** — not BT.601's blue coefficient 0.114. Altwasser deliberately raised it because pure blue was "very dark and hardly visible" on contemporary TVs. Verbatim tables 16-1/16-2/16-3 in the chapter give per-colour /Y, U, V values. Additional findings:

- **Bright is approximately 1.31× per-primary current multiplier**, not 2×. Our current `palette.rs` `0xCD → 0xFF` pair (1.24× ratio) is close but slightly too compressed.
- **Bright Yellow = Bright White at the /Y level** (Q3 saturation caps the output). Our current `0xFFFF00` vs `0xFFFFFF` over-separates them.
- **Bright is luminance-only** — U and V are unchanged. "Bright red" and "normal red" emit the same chroma.
- **Black is silently re-mapped to White for chroma generation only** (NOR-detector forces RGB high → zero chroma for both). Y still discriminates.

Impact: user-visible colour-accuracy on the rendered output, but catalogue frame hashes are computed from palette-indexed framebuffer (not RGBA), so this doesn't affect the regression bench. A CRT filter that uses Smith's Y/U/V tables verbatim would be measurably more accurate than one derived from BT.601. Source: `zx-spectrum-ula-chapter-16-analogue-video.md`.

### Beeper produces four output voltages, not two (Chapter 19/20)

Smith Chapter 20 Table 20-2 (cross-referenced from Chapter 19) shows the (Speaker, MIC) bit pair jointly produces **four** distinct analogue voltages via a resistor voltage divider, not two — so MIC alone produces an audible click (below the 1.4 V speaker diode threshold but still a voltage change in the signal chain). Our current `mic` and `beeper` booleans in `crates/common-sinclair-zx-spectrum/src/ula_engine.rs:501-505` are digitally correct but cannot model three-level beeper engines (which exist in the demoscene corner of the catalogue).

Impact: audio fidelity gap for a narrow set of titles using three-level beeper engines. The 101-entry catalogue does not currently include any title that depends on three-level beeper, so no October-public regression. Source: `zx-spectrum-ula-chapter-19-input-output-devices.md`.

### 5C-vs-6C breezeway shift is the silicon root of "later Spectrums shift picture left" (Chapter 16)

Cross-corroborates the existing Chapter 11 finding (5C vs 6C HSync at counter 336 vs 344) — Chapter 16 traces the same effect to the analogue breezeway shift (2.29 μs → 1.14 μs). Same `IssueRev` flag would drive both the digital HSync start and the analogue picture-offset behaviour if we ever model per-revision display shift.

### `compute_data_addr` / `compute_attr_addr` are silicon-correct (Chapter 15)

Smith Chapter 15 Figure 15-5 gives the display-byte address bit-mapping verbatim: `Display Address = 0 | V7 V6 V2 V1 V0 V5 V4 V3 | C7 C6 C5 C4 C3`, with the DRAM row shared between display + attribute fetches (`Row = V4 V3 C7 C6 C5 C4 C3`). Our existing `compute_data_addr` (`crates/common-sinclair-zx-spectrum/src/ula_engine.rs:286-294`) — `((scan & 0x38) << 2) | ((scan & 0x07) << 8) | ((scan & 0xC0) << 5)` — implements the V[5:3]/V[2:0]/V[7:6] interleaving bit-for-bit, and `compute_attr_addr` matches Smith's `0x1800 | (Y[7:3] << 5) | X[7:3]` layout. **No refactor required for Seam 1**; the address arithmetic is the rare place where our engine matches the silicon verbatim.

### ROM-writes-silently-evaporate is silicon-correct (Chapter 17)

Smith Chapter 17 derives `/WE = /RAM16(-30) + /WR` — writes to the ROM range silently evaporate at silicon level because `RAM16` is low and the DRAM is never strobed. This validates RULES.md §13 ("ROM writes are silently ignored. No panics, no logs."). DRAM access is fast enough (90 ns CAS-after-RAM16 vs 142 ns T₁ half-cycle) that there are no inherent DRAM wait states — every Spectrum CPU wait state is a ULA contention wait, never a DRAM wait. Source: `zx-spectrum-ula-chapter-17-cpu-memory-access.md`.

### ROMCS override is a machine-layer concern, not a ULA-internal concern (Chapter 17 + 22)

Smith Chapter 17 derives `/ROMCS = A14 + A15` as the **ULA's internal decode** driving pin 34. Chapter 22 confirms /ROMCS is **totem-pole** (not open-collector) — peripherals cannot wire-OR override it; they must use an external AND-gate or trace-cut to take over the ROM space. Implication for our emulator: ROMCS override (when Interface 1, Beta Disk, IF2, Multiface, divIDE are eventually emulated) belongs in the machine-class `handle_bus` dispatcher as a "do I claim the ROM space?" check on attached peripherals BEFORE falling through to the ULA's internal ROMCS. The ULA trait does not need a "ROMCS override" hook. Source: `zx-spectrum-ula-chapter-22-signal-interfacing.md`.

### ULA contains zero peripheral-side state — Seam 3 reinforcement (Chapter 22)

Smith Chapter 22 explicitly confirms that K0-K4 (keyboard inputs) and D0-D7 (data bus) are sampled-only at the ULA — the ULA latches *only* what's needed for the current cycle, and holds no persistent peripheral state. Reinforces Seam 3: peripheral volatile state belongs in each peripheral's own crate (`peripheral-kempston-joystick`, `nec-upd765a`, future IF1/IF2/Beta) and never in `UlaEngine`. Source: `zx-spectrum-ula-chapter-22-signal-interfacing.md`.

### Spectrum has no ULA-mediated bus arbitration — peripheral architecture constraint (Chapter 22)

Smith Chapter 22 (cross-referenced from Chapter 18) documents that Altwasser **explicitly rejected both /WAIT and /BUSREQ as contention mechanisms because no spare ULA pins were available**. The consequence is permanent and severe: the Spectrum has no clean DMA/handshake path through the chipset. All peripheral bus interaction happens at the Z80 expansion-port layer — peripherals must drive /WAIT, /BUSREQ, or snoop /M1 on the Z80's own pins. /ROMCS is totem-pole (override needs external AND); /INT is open-collector (peripherals can wire-OR). This is the architectural reason why Beta Disk's M1-trap mechanism is a passive /M1-snoop with external /ROMCS gating rather than DMA — it's the only architecture the silicon permits. Future peripheral emulation must model bus-master behaviour at the Z80-pin layer, never through the ULA. Source: `zx-spectrum-ula-chapter-22-signal-interfacing.md`.

### Beeper voltage curve refinement — Chapter 20 Table 20-2

Smith Chapter 20 Table 20-2 gives the complete (Speaker, MIC) → voltage mapping verbatim for both 5C and 6C ULAs. Per-revision threshold differences explain the Issue 2/3 EAR-bit-6 distinction analytically (5C: 0.728 V vs 6C: 0.652 V against ~0.714 V threshold). Smith treats the four-voltage output as an *accidental* side-effect of the cost-driven single-pin design, not a designed feature ("a consequence of having a single multiplexed analogue I/O port"). Implementation guidance for closing the 3-level beeper fidelity gap (deferred beyond October): replace the `mic`/`beeper` booleans in `ula_engine.rs:501-505` with a per-`IssueRev` 4-entry voltage LUT indexed by `(beeper << 1) | mic`. Source: `zx-spectrum-ula-chapter-20-cassette-storage-and-sound.md`.

### 16K DRAM refresh disabled in silicon (Chapter 23) — *PCB-only fix per Chapter 24*

Smith documents that the 16K original Spectrum's /RFSH→/RAS connection causes the ULA to attempt simultaneous refresh and read, producing the snow effect. **Correction from Chapter 24 distillation (2026-05-19):** the fix was **PCB-only, not silicon-level**. The /RFSH→/RAS wiring was disconnected on the Issue 2 board, but the 5C112E ULA silicon retained the same broken refresh wiring as the 5C102E. Only the board changed. Sinclair accepted that all 16K refresh comes from video fetches (which is sufficient). Our 16K emulation doesn't currently model this — and shouldn't need to, since refresh from video reads keeps RAM contents stable. Worth a comment if the 16K runtime is ever audited for refresh correctness.

### BoardIssue enum granularity is correct as-is (Chapter 24)

Smith Chapter 24 documents six ULA revisions: **5C102E** (Issue 1), **5C112E** (Issue 2), **6C001E-6** (Issue 3), **6C001E-7** (Issue 4), **6C011E** (NTSC export, never sold in production quantity), **7K010E-5** (128K). The architecture review's existing `BoardIssue::Issue2 / Issue3` enum captures the only software-visible distinction (EAR-bit-6 readback) correctly. Issue 1 distinctions are museum-only (broken keyboard via IOWAIT silicon bug, fixed externally by the "dead cockroach" IC). Issue 4/4A/4B/4S/5/6A distinctions are DRAM-margin-only and produce no software-visible change. **Recommendation: do not extend `BoardIssue` until forced.** A rename to `BoardIssue::Family5C / Family6C` would be more honest but functionally equivalent. The deferred 3-level beeper LUT (Chapter 20 fidelity finding) maps onto the existing 5C/6C split. Source: `zx-spectrum-ula-chapter-24-ula-versions.md`.

### Pin budget is set at masterslice-selection, before any custom logic (Chapter 5)

Smith Chapter 5 reveals that the Ferranti 6000-series masterslice physically caps peripheral cells at the bond-pad count, which equals the DIL pin count, which is fixed by the chosen package (26-40 pins). The Spectrum ULA's pin scarcity that drove Altwasser to clock-gating contention (Chapter 18) was not a logic-design oversight — it was a silicon-process constraint locked in at the masterslice-selection step, before any custom logic was drawn. This reinforces the global KB lesson at `~/knowledge/retro-peripheral-architecture-is-pin-budget-not-design-choice.md`.

## Order of work

In order of leverage for unblocking October-public and protecting future capture passes:

1. **Seam 4 (catalogue oracle integrity)** — must land *first*. Adds `audio_routing_version` and `frame_routing_version` constants so the version bumps catch the Seam 1 re-capture wave cleanly. Refresh `solid-status.md` §1 in the same commit.
2. **Seam 1 (ULA shifter pipeline depth)** — closes the open Float48K thread and corrects every 48K and 128K frame hash that touches the first display byte. Re-capture triggered by the Seam 4 version bump. Pre-flight checklist in `ula-first-fetch-tstate-offset.md`.
3. **Seam 2 (host input → Kempston routing)** — small, October-visible, no dependency on the others. Can land in parallel with Seam 1.
4. **Seam 3 (volatile state survival)** — formalise the `after_restore` contract, fix the FDC disk image path, add the `#[serde(skip)]` audit lint.
5. **Seam 5 (boot invariants suite)** — incremental, one waypoint per landing PR. Begin with the Float48K strict assertion (un-gated from `EMU198X_FLOAT48K_STRICT=1`) once Seam 1 lands.

## Done criteria

- **Seam 1**: 48K Float48K strict mode prints 14338. 128K prints 14364. Both run in CI without env-var gates. At least one floating-bus title from the Grussu corpus (Arkanoid / Cobra / Short Circuit / Sidewize) added to the 48K catalogue and passing.
  - Engine status (2026-05-19): **landed** — commits `0660521` + `fbc5938`. First fetch at T-14338 (48K), AOLatch border granularity in place, `FRAME_ROUTING_VERSION = 3`.
  - Float48K strict un-gate: **blocked on test-harness work** — RST 16 capture can't read PRINT-FP digits. Tracked separately.
  - Floating-bus catalogue entry: **landed** — `arkanoid-tape` in commit `b0f9b7f`, hash captured at v3 in `546ce25`.
- **Seam 2**: gamepad event flips `kempston.attached` and feeds button bits. Catalogue entry verifies the runtime input path against a Kempston-using catalogue title (e.g. Jet Pac from the 16K trilogy or Sabre Wulf from the 48K set).
  - **Wiring landed 2026-05-20** in commit `3087016`. `SpectrumMachine::set_kempston_button` overrides on every Kempston-bearing variant (48K-class, 128K-class, Pentagon, Scorpion, Timex); Amstrad-class declines via the no-op default. Runtime input layer maps `InputEvent::Button { port: 0, … }` and `InputEvent::Axis { port: 0, … }` (with a 25% axis deadzone) to the Kempston state byte. Typed `ApplyKempstonEvent` trait at machine layer mirrors `ApplyInputEvent`, bounded on `Variant48kClass` so it cannot exist for Amstrad-class types.
  - **Catalogue verification landed 2026-05-20** in commit `6b19411`. New entry `sabre-wulf-kempston-start` (48K) drives a scripted sequence — key "4" selects Kempston control, key "0" starts the game, then `InputEvent::Button { port: 0, name: "fire" }` swings the sabre. Captured 1UP-001070 gameplay frame vs the no-FIRE baseline 1UP-000545 proves the routing chain reaches a real game's `$1F` poll. The hash diff against the baseline is load-bearing: a regression breaking any link in `ScriptStep::Button → session.queue_input → HostIo::input_events → SpectrumRuntime::apply_input → set_kempston_button → KempstonJoystick state` collapses the hash back toward baseline.
- **Seam 3**: every `#[serde(skip)]` field on a Spectrum-stack struct either has a `Default` that produces correct behaviour or is rehydrated by a typed `after_restore`. Audit test asserts this. FDC disk image survives snapshot restore in a regression test.
  - **Landed 2026-05-20** in commit `7ea8842`. Runtime caches DSK bytes alongside the machine; snapshot envelope bumped to v2 with a `disk_images` field; `restore_disk_images` replays the insertion after `after_restore`. Regression test `snapshot_restore_preserves_mounted_disk_on_plus3` exercises the round-trip. Audit lives in `crates/common-sinclair-zx-spectrum/src/serde_skip_audit.rs` with a locked inventory of 13 annotations across 6 files, each carrying a justification.
- **Seam 4**: `audio_routing_version` and `frame_routing_version` constants in place. Catalogue mismatch fails loud with a re-capture instruction. AY re-capture wave completed; R-Type's 128 audio hash unchanged (beeper-only invariant); RoboCop / Operation Wolf / Rainbow Islands / Bubble Bobble / Out Run hashes updated. `solid-status.md` §1 reflects the code reality.
  - **Landed 2026-05-19 / 2026-05-20**: routing-version check landed in commit `c7abaef`, capture-mode bypass in `0471db4`. Re-capture wave covered all 102 entries across 9 commits (`546ce25` 48K vanilla, `94347ff` Plus, `57c2994` 16K, `539aa00` 128K, `22ecf8b` +2, `bdde7f4` +2A, `eaebbdc` +2B, `e2daab9` +3, `d9507c2` SpeedLock). Manifest now at `frame_routing_version = 3`; all 102 entries PASS in run-mode.
- **Seam 5**: `boot_invariants.rs` carries at least five per-variant waypoint assertions including the un-gated Float48K strict check.
  - **Suite landed 2026-05-20**. `crates/runtime-sinclair-zx-spectrum/tests/boot_invariants.rs` carries **12 Seam-5 waypoints + 3 setup tests + 1 ROM-backed ignored case** (15 hermetic + 1 ROM-backed total). Tests grouped by what they assert, with the architecture-review name in brackets:
    1. INT timing on 48K — scan 248, pixel 1, 32-T-state window [`int_asserts_at_canonical_t_state`, 48K variant]
    2. INT timing on 128K — same scan, asserted via half-cycles to side-step `cpu_divisor = 5` [`int_asserts_at_canonical_t_state`, 128K variant]
    3. INT timing on Pentagon — scan 256, eight scans later than Sinclair [`int_asserts_at_canonical_t_state`, Pentagon variant]
    4. First display fetch phase aligns with Seam 1 landed state [structural `first_display_byte_on_bus_at_canonical_t_state` surrogate; Float48K strict un-gate replaces this when Phase 1 #8 unblocks]
    5. Floating bus idles outside the active fetch window [companion to #4]
    6. Contention delay tables `DELAY_TABLE_48K` / `DELAY_TABLE_PLUS2A` match canonical pixel masks [`contention_table_matches_canonical_for_known_window`]
    7. Paging lock survives soft reset on 128K [`paging_lock_persists_across_reset`, 128K variant]
    8. Paging lock survives soft reset on +3 [`paging_lock_persists_across_reset`, Amstrad-class variant]
    9. Kempston attaches on first gamepad event (48K) [`kempston_attaches_on_first_gamepad_event`, 48K]
    10. Kempston attaches on first gamepad event (128K) [`kempston_attaches_on_first_gamepad_event`, 128K]
    11. Amstrad-class declines Kempston events on +3 [Seam 2 trait-bound enforcement — the negative case]
    12. Snapshot envelope locked at v2 [Seam 3 catch — silent envelope drift breaks previously-saved snapshots]
  - Adds `is_paging_locked()` public accessors on `Memory128K` and `MemoryPlus`; adds the `serde_skip_audit.rs` inventory.
  - **Float48K strict un-gate** remains blocked on Phase 1 #8 (RST 16 capture can't read PRINT-FP digits). When un-gated it replaces waypoint #4's structural surrogate with the real T=14338 probe and lands the 128K's T=14364 sibling.
- This document is updated with implementation status and links to commits.

## Phase 2 close-out (2026-05-20)

All five named seams have landed code. The order-of-work was Seam 4 → Seam 1 → Seams 2/3/5/UlaRevision in parallel, matching the planned dependency order.

**Landed commits:**

| Seam | Commits | Surface |
|---|---|---|
| 1 | `0660521`, `fbc5938` | Two-stage shifter, AOLatch border granularity, FRAME_ROUTING_VERSION = 3 |
| 2 | `3087016` (runtime), `6b19411` (catalogue) | Kempston routing + sabre-wulf-kempston-start verification |
| 3 | `7ea8842` | FDC after_restore, snapshot envelope v2, serde_skip audit (13 annotations) |
| 4 | `c7abaef`, `0471db4` (gate), `546ce25`/`94347ff`/`57c2994`/`539aa00`/`22ecf8b`/`bdde7f4`/`eaebbdc`/`e2daab9`/`d9507c2` (re-capture wave) | Routing-version checks + 102-entry re-capture at v3 |
| 5 | `082dd74`, `d3156d1`, `3970dbc`, `450ac8a` | 12 boot-invariant waypoints across 48K / 128K / Pentagon / +3 |
| Rename | `ce45ea8` | `BoardIssue::Issue2/Issue3` → `UlaRevision::Ferranti5C/Ferranti6C` (5C/6C family naming) |

**Float48K strict un-gate landed 2026-05-20** in two harness fixes inside `crates/machine-sinclair-zx-spectrum-48k/tests/float_bus.rs`:

1. **Control-byte state machine.** The PR-ALL ($09F4) capture point catches BASIC's PRINT-FP digits that bypass RST 16, but it also captures the argument bytes that follow AT / INK / PAPER / FLASH / BRIGHT / INVERSE / OVER / TAB control codes. A small `skip_args` counter tracks "next 1 or 2 captures are control arguments" and drops them.
2. **`STEP_TSTATES = 1`.** PR-ALL is called roughly 4× more often than RST 16 (the ROM's internal print routines like `PO-MSG` call it directly). At the legacy 4-T-state granularity ~50% of PR-ALL hits were missed because two entries collided inside one sample window. 1-T-state granularity guarantees every entry is caught as a rising edge.

With those harness fixes the probe output is clean: `1982 Sinclair Research Ltd` / `Program: Float48K` / `Bytes: floatcode` / `14330 255 / 14331 255 / … / 14338 255 / 14339 128`. The strict assertion now runs without an env-var gate. Pinned engine value: **T=14339** (`FLOAT48K_EXPECTED_TSTATE`), 1 T-state late vs the canonical T-14338 Woody reports for real Sinclair 48K hardware. The 1-T-state offset is a Z80/ULA phase-alignment subtlety in how our Z80 model samples the IO data bus inside the IN M-cycle — independent of the ULA fetch timing, which is correct per Seam 1. Tracked as an engine-fidelity follow-up; catalogue hashes are unaffected (they depend on the visible-pixel tap, not the floating-bus probe).

**Float128K harness un-gated 2026-05-20** in `crates/machine-sinclair-zx-spectrum-128k/tests/float_bus.rs`. Same shape as the 48K version — control-byte state machine + `STEP_TSTATES = 1`, ENTER-press boot sequence (no `LOAD ""` typing — the 128K's Tape Loader menu entry handles that internally). Pinned engine value: **T=14366**, 2 T-states past canonical T-14364. The 2-T-state offset (vs the 48K's 1-T-state) reflects the same Z80/ULA phase-alignment subtlety scaled by `cpu_divisor = 5` — each CPU T-state spans 5 half-cycles vs the 48K's 4, so the IN-instruction IO sample point lands one full T-state further from the bus exposure event. The 128 BASIC ROM 0 print routine drops the first character of menu lines through our PR-ALL hook (different code path from ROM 1's standard PR-ALL); the load-chain assertion tolerates this cosmetic loss because the probe's iteration values come through cleanly.

**Deferred to post-Phase-2** (captured in [Fidelity findings deferred beyond October](#fidelity-findings-deferred-beyond-october)):

- **Float48K / Float128K T-state offset** — engine prints 14339 / 14366 vs canonical 14338 / 14364. Z80 IN-instruction IO sample-point phase question, scaled by the variant's `cpu_divisor`. Not a ULA fetch bug.
- **5C-vs-6C HSync timing** — no SOLID catalogue entry currently depends; tracked as a per-revision flag if a dependent title surfaces.
- **Smith Y/U/V palette tables for the CRT filter** — `VideoFilter::Crt` ships in `emu198x-native-video`; Chapter 16's per-colour tables would upgrade colour fidelity but do not affect catalogue hashes (palette mapping happens after the framebuffer hash).
- **3-level beeper voltage LUT** — Chapter 20 four-voltage divider. No catalogue title currently exercises three-level beeper.
- **Sinclair Interface 2 keyboard-matrix routing** — separate from Seam 2 Kempston work; per [`spectrum-joystick-architecture.md`](spectrum-joystick-architecture.md).

The catalogue runs **101 entries SNAP-PASS in 94 min** at `frame_routing_version = 3`. Seams 1, 2, 3, 4, 5 are second-line-of-defence against regressions the catalogue cannot catch by construction — silently-locked-in wrong behaviour (Seam 4), lost host input (Seam 2), volatile state that doesn't survive snapshot restore (Seam 3), oracle integrity (Seam 4), and standing boot-invariant assertions (Seam 5). The spine — ULA-drives, no-Bus-trait, half-cycle signals, within-family layering — is unchanged.

## Non-goals

- Refactoring the Z80 internals. Tom Harte 100% is the bar; nothing here changes the CPU.
- Touching the contention tables. The numbers in `knowledge/systems/spectrum/contention.md` are correct against FUSE; only the *position* of the first fetch moves (Seam 1).
- Splitting the class layer crates. The three-class shape (48K / 128K / Amstrad) is right; the seam fixes are internal.
- Pentagon/Scorpion/Timex. Engineering bar, post-October.
- Anything in the runtime/headless-runner layer beyond the input-routing extension.
- Sinclair Interface 2 keyboard mapping. Deferred per [`spectrum-joystick-architecture.md`](spectrum-joystick-architecture.md).

## Related

- [`amiga-architecture-review.md`](amiga-architecture-review.md) — the template this review mirrors
- [`ula-first-fetch-tstate-offset.md`](ula-first-fetch-tstate-offset.md) — open investigation that Seam 1 closes
- [`ula-drives-model.md`](ula-drives-model.md) — the spine this review preserves
- [`spectrum-driver.md`](spectrum-driver.md) — the shared run loop the seam fixes inherit
- [`within-family-layering.md`](within-family-layering.md) — the five-piece structure the seam fixes respect
- [`spectrum-test-oracle-priority.md`](spectrum-test-oracle-priority.md) — why Spectrum-validated oracles outrank generic ones
- [`october-catalogue.md`](october-catalogue.md) — the October-public bar

## Reference library cross-links

Ingested 2026-05-18 / 2026-05-19. All files under `~/Projects/Emu198x-Reference/_organised/by-system/zx-spectrum/`.

### Chris Smith, *The ZX Spectrum ULA: How to design a microcomputer* (all 24 chapters distilled)

The canonical silicon-level reference. Decapped chip, gate-traced, with HDL published at `opencores.org/projects/zx_ula`. Most relevant chapters for the seams below; full chapter index in the reference library.

| Chapter | Topic | Relevance |
|---|---|---|
| 10 | Internal Clocks | C0-C8, V-counter, INT-is-a-consumer |
| 11 | Video Synchronisation | 5C-vs-6C HSync offset; TS2068 not from Smith |
| 12 | Generating The Display | **two-stage shifter** (Figure 12-2); DataLatch/SLoad/VidEN |
| 13 | Video Memory Access | **continuous-fetch** correction; RAS/CAS double-pulse |
| 14 | Video Control Clocks | Complete signal derivations; AOLatch un-gated by VidEN |
| 16 | Analogue Video | Palette Y/U/V tables; non-BT.601 Y equation |
| 17 | CPU Memory Access | ROM-writes-evaporate; ROMCS decode |
| 18 | CPU Clock and Contention | Contention gate equation; IOWAIT silicon bug |
| 19 | Input-Output Devices | `IN A,($FF)` semantics; 4-voltage beeper divider |
| 20 | Cassette Storage and Sound | Table 20-2 beeper voltages verbatim |
| 21 | Interrupts | **Verbatim 14336 derivation**; /INT is pure combinational |
| 22 | Signal Interfacing | /ROMCS totem-pole; no ULA-mediated bus arbitration |
| 23 | Hidden Features and Errors | Test modes; 5 silicon errors; snow effect; SILENT on floating-bus sample point |
| 24 | ULA Versions | Only revision-difference table; 16K refresh fix is PCB-only |

Pages 191, 198, 204, 206 (load-bearing Chapter 18 figures) were rescanned at 24 MP on 2026-05-18; verification refined Chapter 18 distillation Section 8.

### Other Spectrum references

- `zx-spectrum-variants-grussu.md` — Spectrumpedia Vol 1 (Grussu); per-variant technical specs, contended-memory framing, AY chip context, peripheral surface, IF2 keyboard-matrix reference, floating-bus test corpus (Arkanoid / Cobra / Short Circuit / Sidewize), R-Type beeper-only test invariant.
- `zx-spectrum-clones-and-fpga-replicas.md` — Spectrumpedia Vol 2 (Grussu); MiSTer / ZX-Uno / Harlequin HDL cross-validation sources for Seam 1.
- `zx-spectrum-ula-internals-dickens.md` — Adrian Dickens Hardware Manual Chapter 8; ULA-CPU clock-gating corroboration.
- `zx-spectrum-service-manual-notes.md` — Sinclair Service Manual; ULA absolute-priority clock-inhibition mechanism as hardware of record.
- `zx-spectrum-fpga-reimplementation-notes.md` — Arias FPGA paper; Inves Spectrum+ historical precedent for no-contention compatibility.

### Cross-cutting global KB

- `~/knowledge/retro-peripheral-architecture-is-pin-budget-not-design-choice.md` — the principle that retro peripheral architecture is dictated by chipset pin budget at design time, not designer preference. Reinforced by Smith Ch 5 (pin budget fixed at masterslice-selection step, before any custom logic) and Ch 22 (Altwasser explicitly rejected /WAIT and /BUSREQ for lack of spare ULA pins).
