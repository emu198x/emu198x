# Save-state: serde the live machine, one pattern for the whole fleet

**Status:** binding (2026-06-26). Supersedes per-system bootstrap snapshots.

## Decision

Every system's save-state serialises the **live machine** via `serde` +
`postcard`, not a bootstrap envelope that cold-boots from ROM on restore.

The pattern (proven across 14 systems, PRs #655–668):

- The machine struct derives `serde::{Serialize, Deserialize}`. Fixed arrays
  with `N > 32` use `#[serde(with = "BigArray")]` (serde-big-array); arrays
  `≤ 32` (incl. nested like `[[bool; 8]; 10]`) serialise natively. Host-only
  buffers (audio output, `io_trace`, `ay_watch`) are `#[serde(skip)]` and
  default on restore.
- Shared **chip** crates derive serde too (additive — new trait impls, no
  behaviour change). Done so far: TMS9918, SN76489, Z80 CTC, Sega VDP,
  Intel 8255, Motorola 6845, MOS 6520 PIA, VIC-I; the Z80, 6502, 6522, and
  AY-3-8912 already did.
- The runtime snapshot envelope is a **borrowing** struct for encode (no clone)
  + an **owning** struct for decode, carrying `version`, `time`, `model_id`,
  and `machine: Option<M>`. `SNAPSHOT_VERSION` bumps (1→2) — the old bootstrap
  envelope is incompatible. Decode checks version + rejects a `model_id`
  mismatch.
- Restore installs the deserialised machine via a new `set_machine()` that
  **re-derives the host RGBA framebuffer** from the restored chip dims before
  repainting. This sizing is **load-bearing**: runtimes whose `blank()` starts
  with an empty framebuffer Vec panic on restore without it (caught on MSX).
- Dead cold-boot restore helpers (`set_*_bytes`, `rebuild_after_restore`) are
  removed; getters still used by `queries.rs` are kept.

Tests per system: a machine-level `snapshot_round_trips_live_state` (serialise →
advance → re-serialise differs; restore the first → re-serialise byte-identical;
a poked RAM byte survives) + a runtime `decode_rejects_unsupported_version`.

## Atari (decided 2026-06-26): use serde, same as everyone else

The Atari chips (TIA, RIOT/6532, ANTIC, GTIA, POKEY, MARIA) carry **hand-rolled
`save_state()`/`load_state()` byte methods** from an earlier design. We serde
them like every other chip rather than wiring those byte methods into the
runtime — one consistent path across the fleet, no per-family special case.

The hand-rolled byte methods are **left in place** (out of scope to remove, same
as the TMS9918/SN76489/CTC/6845/PIA which also had them). Removing the now-dead
methods is a separate cleanup, not part of the save-state conversions.

## Known gap

Round-trip tests are **machine-level** (postcard on the struct). The runtime
`set_machine` + RGBA-repaint path is verified by mirroring `rebuild_machine` +
code review, not by a runtime-level restore test. A small per-system
run→snapshot→restore→run test would close this — tracked as a follow-up.
