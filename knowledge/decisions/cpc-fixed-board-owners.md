# CPC model configurations have fixed owners

**Status:** Adopted under the approved #2144 split programme.

The implemented CPC464, CPC664 and CPC6128 configurations have separate
`machine-amstrad-cpc464`, `machine-amstrad-cpc664` and
`machine-amstrad-cpc6128` owners over `common-amstrad-cpc`. The 464 has no
built-in FDC, the 664 adds it, and the 6128 adds the HAL16L8 banking PAL.
The primary source is `reference/by-system/amstrad-cpc/cpc-reference.md`
§§1,3,10–12 at the umbrella. These owners retain the engine's HD6845S
CRTC and generic Gate Array baseline; they do not establish independent
implementations of every CRTC/Gate Array revision, regional board or
manufacturer-ID link configuration.

The family runtime uses a closed enum in `machine.rs`, following NES and
Mega Drive. This narrowly overrides `runtime-internal-shape.md`'s generic
runtime requirement for CPC, preserving one public session/runtime type.
The other per-concern modules retain their roles. Each fixed owner checks
model, memory and ROM lengths, banking state, CRTC selection, clock phase
and framebuffer geometry before accepting a raw state. No mutable whole
engine is exposed by a fixed owner.

Profile IDs, firmware IDs and version-4 wire bytes remain unchanged.
Machine IDs now match the three concrete profile IDs. A foreign hardware
payload is rejected before mutating a runtime, even under a matching
header. A blank save retains the selected profile.

Restoring into a blank runtime previously succeeded, but reset removed the
machine: reset relied on host firmware and tape caches absent after
restore. The runtime derives these caches from restored hardware. Expansion
ROMs and modified discs retain their contents and write protection across
reset. This repairs host lifecycle state without changing chip algorithms
or the save schema. Original full v4 states and three native-field
continuations guard pixels, sample bits, clock deltas and final state for
all three models.
