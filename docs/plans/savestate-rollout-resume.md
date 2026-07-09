# Save-state rollout — resume plan

> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.


**Status (2026-06-28): COMPLETE — all 28 runtimes serialise live state; zero
bootstrap envelopes remain.** Verified by a fleet-wide audit of every
`runtime-*` crate's `snapshot`/`restore` path (each deserialises a live machine,
not a cold-boot-from-ROM envelope).

The last "remaining" entries in this plan were stale, not unfinished:
- **Einstein** was a genuine bootstrap envelope — converted this session
  (PR #677): live machine + serde WD1770, disk rides in the snapshot.
- **Spectrum** was **already done** — converted 2026-05-20 as Seam 3 of the
  Spectrum architecture review (commit `7ea8842`), under "Seam" terminology
  rather than the rollout's PR sequence, so it was miscounted. One deferred
  residual (Pentagon/Scorpion/Timex ULA-config reattach on restore) was closed
  this session (PR #679).
- **Amiga** was **already done** — the snapshot round-trip discipline landed at
  commit `09a265c` (postcard envelope, model + version validation, complete
  per-variant chip-stack snapshots for OCS/ECS/A1200-AGA, disk re-mounted
  separately). Tested by `runtime-commodore-amiga/tests/snapshot_roundtrip.rs`
  (fixed-point + bit-identical forward-run + reject paths) plus a real
  Workbench-1.3 snapshot round-trip boot test in the diagnostic harness.

This doc is now a completion record rather than a resume plan; the recipe and
gotchas below stay as reference for any future system added to the fleet.

The binding pattern + rationale live in
[`knowledge/decisions/savestate-live-machine-serde.md`](../../knowledge/decisions/savestate-live-machine-serde.md).
Read that first; this doc is the *state + playbook*.

## The problem

Most runtimes shipped a **bootstrap snapshot**: a `{version, time, model_id,
rom/cart bytes}` envelope whose `restore` **cold-booted** the machine, throwing
away all live CPU/RAM/chip state. The fix: serialise the **live machine** via
`serde` + `postcard`.

## Done (PRs #655–675 + Einstein, all verified + merged/armed)

| Track | Systems |
|-------|---------|
| Templates | Jupiter Ace #655, Aquarius #656 |
| VDP | SG-1000 #657, ColecoVision #658, Sord M5 #659, Master System #660, SVI-328 #661, MTX #662, MSX #663 |
| Acorn | Atom #664, BBC Micro #665, Electron #666 |
| Commodore | PET #667, VIC-20 #668 |
| Atari | 2600 #669, 800XL #670, 5200 #673, 7800 #674 |
| Sinclair | ZX81 #672, ZX80 #675 |
| Oric | Oric-Atmos #671 |
| Tatung | Einstein #677 |
| Sinclair (Spectrum) | 16K/48K/128K/+/+2/+2A/+2B/+3 + Pentagon/Scorpion/Timex TC2048/TS2068 — Seam 3, `7ea8842` (envelope v2, FDC disk-replay, serde-skip audit, `after_restore` contract) |

**Shared chips now derive serde** (additive — new trait impls, no behaviour
change): TMS9918, SN76489, Z80 CTC, Sega VDP, Intel 8255 PPI, Motorola 6845
CRTC, MOS 6520 PIA, VIC-I, TIA (+ TiaAudio), RIOT/6532, ANTIC, GTIA, POKEY,
MARIA, ZX81 ULA, WD1770 (+ its Disk). (Z80, 6502, 6522, AY-3-8912 already did.)

The **Einstein disk seam** was settled this way: the WD1770's `Disk` holds its
backing bytes, which Write Sector mutates in place, so the disk **rides in the
snapshot** — capturing the live machine has to capture a partially-written disk.
That's the precedent for the next FDC-bearing system.

The **Spectrum** was the fleet's most thorough save-state work and predates this
plan: a generic envelope over the live machine, an `after_restore` contract
(Z80-walker rehydration + ULA `&'static UlaConfig` reattach), FDC disk-replay,
and a compile-locked `serde_skip_audit` inventory. The plan's three feared "hard
parts" (variants, tape, ULA) were already solved — tape is a serialised field,
all 12 variants round-trip. The one residual the architecture review *deferred*
(Pentagon/Scorpion/Timex didn't reattach their ULA config on restore, so they
fell back to 48K timing) is **closed here**: those four cores gained
`restore_volatile_refs` and their ULAs a `reattach_config`, with per-variant
INT-timing regression tests. See
[`../../knowledge/decisions/spectrum-architecture-review.md`](../../knowledge/decisions/spectrum-architecture-review.md).

## Remaining

Nothing. The rollout is complete — see the status block above. The
**Amiga** (the last entry once flagged here) was already done: per-variant
chip-stack snapshots (Paula, Agnus, Denise, Copper, Blitter, the 680x0/68020
core incl. FPU/MMU, 2× 8520 CIA) for OCS/ECS/A1200-AGA, model + version
validation, disk re-mounted separately, and a real Workbench-1.3 boot
round-trip in the diagnostic harness. Implementation: commit `09a265c`;
discipline locked in
[`../../knowledge/decisions/amiga-full-family-architecture-review.md`](../../knowledge/decisions/amiga-full-family-architecture-review.md)
§ "Snapshot round-trip discipline".

## The conversion recipe (per system)

Mirror an existing converted system of the same CPU family:
- Z80 + VDP/PSG shape → `machine-sega-sg-1000` + `runtime-sega-sg-1000`.
- 6502 shape → `machine-atari-2600` or `machine-acorn-bbc-micro`.
- Big-array machine → `machine-mattel-aquarius`.

1. **Chips first.** Any chip the machine stores that lacks serde: add
   `serde = { workspace = true }` (+ `serde-big-array` if it has an array >32) to
   its `Cargo.toml`, `#[derive(Serialize, Deserialize)]` on the chip struct + all
   local types it stores (compiler-driven), `#[serde(with = "BigArray")]` on
   arrays >32, `#[serde(skip)]` host-only audio/sample buffers. **Leave any
   hand-rolled `save_state()`/`load_state()` byte methods untouched** (out of
   scope to remove). Additive change → run the chip's **full consumer chain**
   (`grep -rl <chip> crates/*/Cargo.toml`), not just the chip's own tests.
2. **Machine crate.** Add serde (+ serde-big-array) + dev-dep `postcard`. Derive
   on the machine struct + every local type/enum (compiler-driven). BigArray on
   fixed arrays >32 (nested arrays count per-dimension; `[[bool;8];10]` is
   native). `#[serde(skip)]` host-only fields (`io_trace`, `ay_watch`, audio
   buffers). Add a `snapshot_round_trips_live_state` test (serialise → advance →
   re-serialise differs; restore the first → re-serialise byte-identical; a poked
   RAM byte survives).
3. **Runtime crate.** Rewrite `snapshot.rs`: borrowing `…SnapshotRefV2<'a>`
   (Serialize) + owning `…SnapshotV2` (Deserialize) carrying
   `{version, time, model_id, machine: Option<M>}`; bump `SNAPSHOT_VERSION` to 2;
   version check + `model_id`-mismatch reject; `set_time` + `set_machine`. Add a
   `decode_rejects_unsupported_version` test (use the real `Model::` variant).
   In `runtime.rs` add `pub(crate) fn set_machine(...)` and **remove only the
   compiler-flagged-dead** restore helpers (keep getters `queries.rs` uses).

## Hard-won gotchas (the things that bite)

- **`set_machine` MUST replicate `rebuild_machine`'s RGBA-buffer sizing.** Most
  runtimes' `blank()` starts with an empty framebuffer Vec; `rebuild_machine`
  sizes it from `framebuffer_width()/height()`. If `set_machine` skips that,
  **restore panics** (index OOB). Caught on MSX. Use the *same* getters
  `rebuild_machine` uses — the Atari 2600 uses **visible**-window getters
  (cropped raster), not the full framebuffer. Fixed-framebuffer machines (Oric)
  don't resize — match whatever `rebuild_machine` actually does.
- **`Box<[u8; N]>` (N>32)** (Atari 5200 `dma_mem`): neither plain BigArray nor
  `#[serde(skip)]` works (no `Default`). Use a custom `#[serde(with = "mod")]`
  that serialises the slice and deserialises `Vec<u8> → Box` via `try_into`. See
  `machine-atari-5200/src/lib.rs` `boxed_dma_mem`.
- **Deterministic lookup tables that are `Vec`** (POKEY poly tables): do NOT
  skip — `#[serde(skip)]` defaults them to empty and breaks the chip on restore.
  Only skip the true host-output pipeline (sample buffers, downsample
  accumulators, DC-filter state).
- **Dependency ordering between PRs.** A system that needs a chip's serde derive
  is blocked until the PR carrying that derive **merges to main** — branching a
  dependent system off main before then either fails to compile or duplicates
  the derive and conflicts on merge. Sequence: chip-deriving PR → merge → then
  dependent systems. (This blocked 5200/7800 on the 800XL/2600 chip derives, MTX
  on Sord M5's CTC, MSX on SVI-328's PPI, ZX80 on ZX81's ULA.)
- **`Cargo.lock` must be staged** with the commit (adding a serde dep changes the
  per-crate lock entry); CI builds `--locked`.
- Some machines carry their **own crate-local copy** of a chip type (Atari 7800
  has its own `TiaAudio`, distinct from `atari-tia::TiaAudio`) — derive both.

## Verification bar (every system)

`cargo test -p <machine> -p <runtime> [-p <chips>]` green (round-trip +
version-reject pass, no regressions, chips' hand-rolled save_state tests still
pass) + `cargo clippy … --all-targets -- -D warnings` clean + the shared-chip
**consumer chain** green for any chip touched. The pre-push hook re-runs
workspace clippy; CI requires Format/Clippy/Build(macOS+Windows)/Coverage.

## Known gap (worth a follow-up)

Round-trip tests are **machine-level** (postcard on the struct). The runtime
`set_machine` + RGBA-repaint path is verified by *mirroring* `rebuild_machine` +
code review, **not** by a runtime-level run→snapshot→restore→run test. A small
per-system runtime restore test would close it.

## Operational note

The 1Password SSH/signing agent **idle-times-out during the multi-minute
pre-push clippy gate** — commits failed ~5× this rollout. Per house rule, never
blind-retry git on a 1Password failure; the work stays staged, ask the user to
unlock, then re-run. Consider extending the agent unlock duration for long runs.
