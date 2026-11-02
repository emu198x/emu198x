# Fixed A1000, A500 and A2000 regional owners

Adopted 2026-11-02 under the approved #2144 continuation and the remaining
Amiga split plan. Eight fixed owners replace the remaining aggregate machine:
A1000, A500 rev 5, A500 rev 6a and A2000CR rev 6, each PAL/NTSC. Shared wiring
lives in `common-commodore-amiga-ocs::OcsCore<C>`. Markers fix the board,
region, bootstrap fitting, installed Agnus, stock RAM, onboard clock and ID.
The generic runtime and family binary remain; dispatch selects the fixed owner.

This explicitly supersedes `amiga-machine-catalogue.md` sections 1 and 2
where they require chipset-only dispatch and forbid per-board Rust types,
and the corresponding Amiga grouping in `runtime-internal-shape.md`.
Rule 34 and `each-machine-has-its-own-machine-crate.md` require hardware
identity to belong to a machine crate. Chipset classification, shared driver,
CPU dispatch, composable configuration and accelerator separation remain.
This completes the narrow overrides begun by the A1200 and ECS decisions.

Commodore's A500 service manual PN 314981-04, PDF 14 / printed 2-9, distinguishes
rev 5 (8370/8371, 256K×1 DRAM) from rev 6a (8372, 256K×4). The rev-6a owner
starts with factory 512 KiB chip RAM. Its existing “maxed” profile explicitly
retains modified 1 MiB chip RAM, 512 KiB slow RAM and 8 MiB fast RAM; this is
not a factory configuration. A501 and GVP A530 remain add-on profiles of the
early A500 owner used by the existing implementation. No RAM size selects
silicon or silently changes board identity.

A2000CR rev-6 schematics PN 314135-02, August 1989, PDF 4 / printed parts 1,
identify PCB 312720-05 NTSC and -06 PAL, 8372 Agnus, 8362 Denise, Buster at
U800 and MSM6242B at U801. Service update issue 25, item 25/3-2.1, PDF 43,
says factory rev 6 is jumpered for 1 MiB chip RAM. Earlier German A2000A and
PCB 312720-01 populations are not qualified by these owners. Buster/Zorro
backplane coverage is not newly established by extraction. A1000 ownership
retains its bootstrap ROM, writable control store and lock transition;
it does not claim every unexamined PCB or chip population.

Primary library identifiers are `reference/by-system/commodore-amiga/`
`a500-service-manual-pn-314981-04-oct-1990-text`,
`a2000-system-schematics-rev-6-pn-314135-02-1989-08`,
`amiga-an-update-for-the-service-technician` and `a1000-service-manual`.
WinUAE is read-only implementation precedent. No chip, CPU or driver timing
is rewritten. The preceding A2000 clock correction and exact v66/v67-to-v68
migration policy are in `amiga-a2000-onboard-rtc-migration.md`.

All eighteen existing family profile IDs and aliases remain. Owner IDs are
new registry identities; legacy profile IDs and the Model enum order remain
unchanged in saves. Raw `OcsSnapshot<C>` values carry a compile-time owner
marker omitted from serialization. Candidate restore checks region, Agnus
revision, bootstrap fitting and mandatory onboard RTC before live mutation.
The family envelope supplies the persisted model identity: identical chip
bytes alone cannot distinguish an expanded A500 from an A2000.

The unchanged original engine at `c06321677cde6b4aea0211197eb8436ceae0a1a7`
produced 56 immutable v67 cases covering twelve profiles, both A1000 boot
phases, progressive/interlaced fields and half-CCK debugger cuts. ZIP is
storage only: every decoded identity, native packet and complete saved byte
is checked, allowing solely the approved header/RTC-presence migration.
Small direct-test firmware copies must equal those archived originals.
No replacement golden, tolerance or discarded state history qualifies a split.
