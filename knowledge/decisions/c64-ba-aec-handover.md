# Decision: Derive VIC-II Phi2 ownership from the aggregate BA-to-AEC handover

**Date:** 2026-08-08
**Status:** BINDING
**Implementation revision:** `9176e269`
**Follow-up qualification:** `d140a36f`

## The question

How does Emu198x decide whether a PAL 6569 matrix access owns the Phi2 bus,
and what data does a forced late badline receive while the CPU still owns it?

## Evidence

BA and AEC describe different phases of one bus handover. BA warns the 6510
that VIC-II access is approaching and drives its RDY input. AEC changes only
after the warning interval and determines which chip owns the address bus. An
ordinary badline has the necessary BA lead before its first cycle-15 matrix
access. A badline created by a cycle-15 `$D011` write does not.

VICE 3.10 represents the resulting invalid prefix in
`vicii-fetch.c::vicii_fetch_matrix`. Its `num_0xff` entries receive `$FF` in
the video matrix and the low nibble of the CPU-side byte selected at the
program counter. The remaining entries come from screen and colour RAM.

VirtualC64 makes the ownership test explicit in `VICII::cAccess`. Its delayed
BA line permits a memory access only after BA has remained low for the warning
interval. Before then it stores `$FF` and the low nibble of the byte at the
CPU program counter. The implementation notes explain that the VIC-II data
drivers remain disconnected while AEC is high.

Hoxs64 independently maintains a three-step `vicAEC` delay in `SetBA`.
`C_ACCESS` selects real matrix and colour memory only after that delay;
otherwise it combines `$FF` with the low nibble of `cpu_next_op_code`. Its
delay follows the aggregate BA state rather than restarting for each possible
DMA source.

The staged VICE `colorfetchbug` programs distinguish the disconnected matrix
byte, the CPU-derived colour nibble, leakage through the display path and the
first valid access after the handover. They therefore provide an output oracle
for both the ownership boundary and the supplied data.

## The decision

The VIC-II owns Phi2 only after four consecutive ticks with aggregate BA low.
The first BA-low tick sets the handover age to one. The age saturates at four,
resets to zero when BA returns high and does not restart when one continuous
BA-low interval moves between badline and sprite-DMA causes.

`ba_low` remains the warning signal used by the machine to drive CPU RDY. The
CPU may complete a write while RDY is low, as required by NMOS 6502
semantics. `cpu_stalled` is the AEC-equivalent ownership state and becomes
true when the aggregate handover age reaches four.

For each attempted matrix access:

- while AEC is high, store `$FF` in the matrix row and the low nibble of the
  sampled CPU Phi2 byte in the colour row without reading screen or colour
  RAM; and
- while AEC is low, read the ordinary screen and colour values through the
  VIC-II memory mapping.

The relevant PAL sequences are:

| Case | BA-low warning interval | Matrix access result |
| --- | --- | --- |
| Ordinary badline | Cycles 12–14 | Cycle 15 is the fourth consecutive BA-low cycle and reads valid matrix and colour data. |
| Badline created by a cycle-15 `$D011` write | Cycles 16–18 | Cycles 16–18 store `$FF` and the CPU-side low nibble. Cycle 19 is the first valid matrix and colour read. |
| Overlapping BA causes | Any continuous low interval | The age remains saturated at four until aggregate BA returns high. |

This ownership decision is independent of the entering-Phi1 display-state
decision. Cycle 16 of a forced late badline still performs idle graphics
behaviour, as specified by
[the late-badline display-phase decision](c64-late-badline-display-phase.md),
while its attempted Phi2 matrix access stores invalid bus data.

## CPU/VIC bus contract

The C64 machine samples the CPU side of the shared data bus before ticking the
VIC-II and passes that byte as `VicPhi2Bus`. For a CPU read, the sample is a
side-effect-free read at the live CPU address through the current PLA mapping.
For a CPU write, it is the byte on the CPU data pins.

The VIC-II does not know about the 6510 implementation and does not call back
into it. This keeps the pin-level CPU bus decision intact while making the
otherwise hidden CPU-side byte an explicit machine-level input. A directed
`STA $D011` test verifies that the following opcode byte, rather than the
retained write byte, supplies the low nibble at the forced-badline boundary.

## Persistence and inspection

The consecutive BA-low age is delayed machine state. It is serialised with
the VIC-II. Snapshot envelope version 5 first preserved it; the current
version 6 also preserves the source-resolved BA latches, c-access activity,
pending `$D011` completion phase and explicit far-edge window. Regression
snapshots taken within the three-cycle handover and at the far-edge boundary
compare restored execution with an unforked machine so restoring cannot make
an access valid one cycle early or late.

The runtime query surface now exposes the state needed to inspect this
contract. In addition to the existing `cpu.addr`, `cpu.data` and `vic.ba_low`
paths, revision `9176e269` adds:

- `cpu.data_in`;
- `vic.aec_low`, `vic.badline`, `vic.ba_low_cycles`, `vic.cpu_stalled` and
  `vic.idle_state`;
- `vic.last_bus_data`; and
- `vic.rc`, `vic.vc`, `vic.vcbase` and `vic.vmli`.

Revision `d140a36f` adds `vic.badline_ba_low`, `vic.sprite_ba_low`,
`vic.c_access_active`, `vic.pending_d011_write_cycle`,
`vic.late_badline_window` and `vic.late_badline_fetches_remaining`. These
separate ownership age from the causes and access attempt being inspected.

The ownership correction advanced `FRAME_ROUTING_VERSION` to 4 because the
invalid matrix and colour values are observable pixels. The later far-edge
window correction advances it to 5 without changing this ownership contract.
All 13 catalogue entries retain their existing frame and audio hashes and pass
both ordinary and fresh-runtime snapshot verification at routing version 5
and snapshot version 6. These are Emu198x compatibility and determinism
checks, not independent hardware evidence.

## Verification

At revision `9176e269`, directed tests establish that:

- a forced late badline stores `$FF` and three distinct supplied CPU nibbles
  during cycles 16–18 without reading matrix or colour memory;
- cycle 19 performs the first valid read;
- an ordinary badline retains its valid cycle-15 access after the cycles
  12–14 BA lead;
- continuous BA across badline and sprite-DMA causes does not restart AEC;
- a completed `STA $D011` exposes the next opcode through the sampled bus
  contract; and
- a snapshot taken during the handover resumes with the same ownership age.

The clean revision-keyed PAL 6569 report is
`target/accuracy/c64-vicii-survey/9176e2690fe25c069fe2b4cb4529a0de4f22f23d/report.json`.
All five registered `colorfetchbug` programs match all 104,448 classified
pixels and have byte-identical indexed-plane hashes relative to their
references. The strict five-program colour-fetch lane also passes.

Cycle traces from VICE and Emu198x show the stable-raster handler, complete
initial sprite-DMA stall and the critical `$3B` write to `$D011` at the same
effective cycles once pre-tick pins and post-access monitor reports are
compared at the same boundary. Revision `d140a36f` closes the separate
far-edge window error without changing the ownership rule. `sequencer-bug`
improves from 96,266 to 104,394 matching pixels, while all five exact
`colorfetchbug` outputs remain unchanged. Its remaining 54 pixels are now
classified as delayed C-data output sequencing rather than an upstream IRQ,
CPU-stall or ownership error.

## Evidence boundary

This decision governs matrix c-access ownership and its invalid data only. The
number of attempts remaining after a far-edge write is governed by
[the far-edge badline-window decision](c64-far-edge-badline-window.md).
Revision `d140a36f` closes that question without changing which attempted
accesses own the bus.

This decision also does not define whether sprite Phi2 bytes 0 and 2 must
observe the same invalid-access sideband during the BA-to-AEC interval.
Reference emulators
contain AEC-sensitive sprite paths, but a selected external output oracle has
not yet fixed their exact Emu198x contract.

The invalid matrix access deliberately does not update the simplified
`last_bus_data` latch. Whether disconnected Phi2 activity changes the
CPU-visible or VIC-II-visible open-bus value is a separate evidence question.
Exposing `vic.last_bus_data` makes that state inspectable; it does not settle
its physical meaning.

The implementation still combines fetch and display more closely than the
reference sequencers. It has no distinct delayed C-data output state for the
remaining forced-badline carry behaviour exposed by `sequencer-bug`.
VirtualC64 delays the combined g-access result before loading the graphics
sequencer; Hoxs64 retains current and previous C-data values plus an explicit
carry state. Selecting the smallest hardware-shaped Emu198x contract that
preserves the exact colour-fetch lane is the next display-pipeline question.

The exact-output evidence is for the PAL 6569 profile and the five pinned
staged programs. It does not establish equivalent behaviour for 6567R8,
6567R56A or 8565, and it remains software-comparison evidence rather than a
physical-hardware capture.

## Drift triggers

Reject changes that:

- derive AEC from a fixed raster-cycle table instead of consecutive aggregate
  BA-low state;
- restart the handover while BA remains continuously low across DMA causes;
- make a forced cycle-16, 17 or 18 matrix access read screen or colour RAM;
- make an ordinary cycle-15 access invalid after its cycles 12–14 BA lead;
- fabricate the invalid colour nibble inside the VIC-II instead of supplying
  observable CPU-side bus data;
- update `last_bus_data` from invalid Phi2 activity without an independent
  open-bus evidence contract;
- hide the handover age from snapshots or inspection; or
- change the ownership rule to repair the separate C-data latch residual.

Any change to BA aggregation, CPU write scheduling, Phi2 bus sampling,
matrix access, sprite sideband access or snapshot state requires the directed
tests and all five strict colour-fetch cases to be rerun.

## Related Documents

- [PAL 6569 late-badline display phase](c64-late-badline-display-phase.md)
- [PAL 6569 far-edge late-badline DMA window](c64-far-edge-badline-window.md)
- [C64 accuracy closure campaign](c64-accuracy-closure-campaign.md)
- [C64 architecture review](c64-architecture-review.md)
- [CPU bus interface](cpu-bus-interface.md)
- [Save state format](save-state-format.md)
- [MOS 6569 / 6567 VIC-II](../chips/mos-vic-ii.md)
- [C64 VIC-II reference survey](../processes/c64-vicii-vice-survey.md)
- [VIC-II survey fixture notes](../../test-data/commodore/c64/vicii-vice-survey/README.md)
