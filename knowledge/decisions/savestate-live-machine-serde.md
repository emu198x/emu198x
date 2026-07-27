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
  and `machine: Option<M>`. The initial live-machine rollout bumped
  `SNAPSHOT_VERSION` from 1→2 because the old bootstrap envelope was
  incompatible. Decode checks the leading version before decoding the full
  payload, then rejects a `model_id` mismatch.
- Restore installs the deserialised machine via a new `set_machine()` that
  **re-derives the host RGBA framebuffer** from the restored chip dims before
  repainting. This sizing is **load-bearing**: runtimes whose `blank()` starts
  with an empty framebuffer Vec panic on restore without it (caught on MSX).
- Dead cold-boot restore helpers (`set_*_bytes`, `rebuild_after_restore`) are
  removed; getters still used by `queries.rs` are kept.

Tests per system: a machine-level `snapshot_round_trips_live_state` (serialise →
advance → re-serialise differs; restore the first → re-serialise byte-identical;
a poked RAM byte survives) + a runtime `decode_rejects_unsupported_version`.

## Z80 interrupt-response amendment

Z80-based live-machine runtimes use snapshot version 3. The core now preserves
the identity of an accepted NMI, IM 0, IM 1, or IM 2 response so the skipped
static walker sequence can be rehydrated mid-response. Version 2 payloads did
not carry that identity and are rejected before full postcard decode.

The core regression suite serialises and resumes every externally observable
half-cycle of all four response sequences. The Spectrum runtime additionally
round-trips and continues a machine snapshot taken during NMI acknowledgement.
The sequence representation is defined in
[Z80 interrupt snapshot identity](z80-interrupt-snapshot-identity.md).

## MC68000 level-7 amendment

MC68000 live state includes both the most recently sampled IPL value and
a pending lower-to-level-7 transition. The first prevents restore from
inventing a new transition while level 7 remains held. The second prevents
restore from losing a transition that arrived before an instruction
boundary.

The Amiga runtime envelope advances to version 20 and rejects version 19
before decoding its positional postcard payload. Stock Amiga interrupt
logic produces levels 0-6, but every Amiga snapshot embeds the CPU layout,
including the A1200's nested MC68020 wrapper. Raw CPU and machine
postcards remain unversioned; durable save states use the runtime
envelope.

The recognition rule and reset boundary are defined by
[MC68000 level-7 transition recognition](motorola-68000-level-7-transition.md).

## MC68010+ acknowledged-vector amendment

The shared MC68010-and-later interrupt path now has a reachable
continuation between interrupt acknowledge and Format/Vector frame
construction. At that boundary the selected vector may be retained in
`exc_vector`, and the pending PC in `exc_pending_pc` must survive until
the PC stack writes begin. The latter field was previously skipped by
serde even though formatted synchronous exceptions also rely on it.

The Amiga runtime envelope therefore advances from version 20 to version
21. The positional postcard layout changes to include the pending PC, and
a version-20 reader does not understand the new continuation tag. All
Amiga models reject version 20 so the common runtime retains one
compatibility boundary.

The frame rule is defined by
[MC68010+ acknowledged interrupt vectors](motorola-68010-acknowledged-interrupt-vector.md).

## MC68020+ master interrupt-stack amendment

MC68020-and-later live state includes whether a master-mode interrupt still
owes its Format-$1 throwaway frame, the SR and PC buffered by an in-flight
`RTE`, and the USP/ISP/MSP bank from which that frame is being consumed.
An in-flight `UNLK` likewise retains the exact bank associated with its saved
original pointer. This preserves its serialized continuation identity; it does
not claim an MC68020 master-stack fault-recovery path. Format-$A entry and
return retain their step counter and saved frame PC. That preserves the current
structural frame continuation; it does not imply that precise Format-$A
pipeline or fault-rerun state is implemented. The live SR alone cannot
reconstruct these continuation boundaries: interrupt entry switches from MSP
to ISP between frames, while Format-$1 `RTE` applies an intermediate SR before
restarting on whichever of the three stacks it selects.

The Amiga runtime envelope therefore advances from version 21 to version 22.
A positional version-21 payload does not contain those fields. Every Amiga
model rejects it before decoding the machine payload so the shared runtime
retains one compatibility boundary.

The stack-selection and frame rules are defined by
[MC68020 master-mode interrupt stacks](motorola-68020-master-interrupt-stacks.md).

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

## Related documents

- [Z80 interrupt snapshot identity](z80-interrupt-snapshot-identity.md)
- [MC68000 level-7 transition recognition](motorola-68000-level-7-transition.md)
- [MC68010+ acknowledged interrupt vectors](motorola-68010-acknowledged-interrupt-vector.md)
- [MC68020 master-mode interrupt stacks](motorola-68020-master-interrupt-stacks.md)
- [Runtime internal shape](runtime-internal-shape.md)
- [Spectrum architecture review](spectrum-architecture-review.md)
