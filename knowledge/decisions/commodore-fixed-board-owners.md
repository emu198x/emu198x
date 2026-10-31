# C64 and C128 configurations have fixed machine owners

**Status:** Adopted under the approved #2144 continuation programme.

The implemented breadbin and C64C PAL/NTSC configurations have four fixed
machine owners over `common-commodore-c64`. C128 PAL/NTSC have two owners over
`common-commodore-c128`. The shared keyboard is common code, so C128 no longer
depends on another machine crate. The original engines and regression suites
stay common evidence; each thin owner has seven exclusive direct tests.

These names describe the existing chip/clock configurations. They do not
qualify every PCB revision, early NTSC VIC, CIA date code or C128D fitting.
The umbrella's C64/C64C service-manual parts tables distinguish boards and
regional VIC populations; `reference/by-system/commodore-c64/verification.md`
records the 250469-01 parts-list evidence and its limits. The C128 service
manual and `reference/by-system/commodore-c128/c128-reference.md` distinguish
the 8564/8566 VIC-IIe and VDC. Existing chip selections are retained; a case
name alone must not be used to claim that every unit has those chips.

The runtime keeps its existing `C64FamilyRuntime<M: C64FamilyMachine>` shape
and outer C64/C128 runtime dispatcher. Two closed owner dispatchers in
`boards.rs` supply its concrete machine parameters. This is a narrow addition
to `runtime-internal-shape.md`'s module list, not a replacement for its generic
runtime contract. Shared execution, input, queries and snapshots stay in their
existing per-concern modules. Fixed owners expose the original register and
peripheral operations but no replaceable whole-engine access.

Profile IDs, firmware IDs, variant aliases and original version-22 wire bytes
remain unchanged. Dispatch adds no serialized tag. C64 restore validates the
model and VIC/SID/CIA population on a candidate before replacing live state;
C128 binding validates model, VIC-IIe, CIA/SID population, RAM and output
geometry, then rehydrates the Z80 walker. A foreign board under an otherwise
matching runtime header is rejected without changing the selected machine.

Complete original saves and three native continuations cover all six current
configurations, with active pixels/audio and held controls at partial fields.
They retain exact first resumed fields, sample bits, clock deltas and final
state; direct and restored resets agree. C128 still lacks C64 mode, 2 MHz
execution, tape, drives and cartridges. The 80-column VDC remains a display
selection within each C128, not a separate machine identity.
