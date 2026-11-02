# Fixed ECS owners and A500+ clock migration

Adopted 2026-11-02 under the approved #2144 continuation. The fixed-owner
pattern follows the A1200 decision: shared ECS wiring belongs in a common
crate, with separate A500+ PAL/NTSC and A600 PAL/NTSC owners. Markers fix region,
Gayle fitting and stock 1 MiB RAM. RAM, ROM and add-on choices remain profiles.
The existing generic runtime and family binary remain. This narrowly extends
`a1200-regional-board-owners.md`'s override of the older chipset-tier grouping
in `runtime-internal-shape.md`; OCS remains grouped pending its own evidence.

The A500+ service manual (Commodore, October 1991, PDF page 3 / printed 1-1)
explicitly specifies onboard battery-backed RTC and 1 MiB stock RAM. Its
clock must be decoded even without slow RAM. Primary source:
`reference/by-system/commodore-amiga/a500-plus-service-manual-1991-10-commodore-text`.
The A600 schematic manual (Commodore, April 1992, PN 400422-02, PDF page 3 /
printed 1-1) describes the Gayle, ECS and PAL/NTSC configurations. It is a
separate board; these owners do not qualify every revision or chip population.
WinUAE's `built_in_chipset_prefs` independently enables `cs_rtc` for CP_A500P;
A600 clock fitting is expansion-dependent. Vendored source is precedent,
not the primary hardware authority.

Steve approved v67 output with an explicit v66 reader. The saved field layout
is unchanged. On v66 A500+ restore, enable only the persisted RTC-presence flag;
keep its already-saved, already-advancing RTC history and all other fields.
Other boards retain their v66 payloads unchanged. The new writer emits v67 for
the family; v65 and older remain rejected. V67 cannot claim an A500+ without
its fitted clock. Restore validates a candidate before touching live state.

Original complete v66 initial/final states and native packet observations are
frozen before correction in `test-data/commodore-amiga/ecs-v66/`. Compare every
field and packet exactly, permitting only the documented version and RTC flag
changes. The A1200 corpus permits only the new envelope version; its complete
payload and final-state hash are compared after explicit header translation.
No tolerance, dropped history, re-recorded golden or altered chip clock is
introduced. Existing optional RTC/expansion modelling is not newly qualified.
