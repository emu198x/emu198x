# Fixed regional A1200 owners

Adopted 2026-11-01 under the approved #2144 continuation after #2224.

PAL and NTSC A1200 configurations have separate private machine crates. Their
shared AGA wiring lives in `common-commodore-amiga-aga::A1200Core<C>`, where
`C` fixes the construction region, stock RAM default and owner ID. Each machine
crate supplies one marker and one alias. No mutable raw-engine replacement
surface is exposed. The snapshot struct retains its v66 fields; the marker is
not serialized. Foreign-region raw restores return an error before mutation.

The existing generic `AmigaRuntime<M>` and per-concern files remain. Its family
dispatcher has distinct AGA PAL/NTSC cases. This supersedes only the older
three-chipset-tier Amiga grouping described in `runtime-internal-shape.md`:
fixed owners require distinct generic runtime types even when chips are shared.
OCS/ECS remain grouped until their separately evidenced board extractions.
No trait-object runtime, separate regional runtime crate, CPU loop, chip timing
or schema change is introduced. Runtime construction validates that its selected
profile matches the fixed region; blank construction takes no model argument.

Both original profile IDs and aliases remain. The family binary stays
`emu198x-amiga`; the new registry owners are `commodore-amiga-a1200-pal` and
`commodore-amiga-a1200-ntsc`. Original v66 continuations are frozen at
`test-data/commodore-amiga/a1200-v66/` before this extraction.

Primary scope: Commodore's A1200 service schematics record different X1 oscillator
fittings and PAL-only video components. The scanned 1992 drawing, sheet 2 / PDF
page 6, explicitly notes the PAL 28.37516 MHz oscillator; the NTSC oscillator is
28.63636 MHz. See `reference/by-system/commodore-amiga/1992-a1200-schematics`
and its source sidecar. The service-addendum index (PDF page 2) prints 28.37512
MHz for PAL; this extraction preserves the existing 28.37516 MHz nominal clock,
not a claim to have measured a particular crystal. PDF page 4 shows the separate
PAL colour-burst section. These sources evidence regional configurations, not
qualification of every PCB revision or fitted chip population.

No A1000 bootstrap construction is exposed by these A1200 owners. Existing RAM
and expansion modelling is retained, without claiming new physical add-ons.
The current web constructor selects A500+A501; public A1200 pages require an
exact qualified module/profile or omit the embed.
