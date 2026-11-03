# Fixed Atari 5200 board owners

Adopted 2026-11-03 for #2144. The manufacturer identifies CA018087 (original
four-port), CA020108 (four-port universal) and CA021374 (two-port universal).
Each has its own private machine crate and `machine_id`; shared hardware and
cartridge evidence live in `common-atari-5200`.

The fixed-owner pattern follows Atari XL/XE: a common core parameterized by
a concrete board marker and a closed runtime dispatcher. For this family,
this explicitly supersedes the requirement in `within-family-layering.md`
and `runtime-internal-shape.md` for a generic `Runtime<M>` with per-variant
runtime aliases. One family runtime and binary remain; host services still
use the shared shell contracts. Hardware timing is unchanged.

Steve explicitly chose to retire `atari-5200-ntsc` rather than assign its
uncertain physical identity to CA018087. The three explicit profile IDs are
`atari-5200-ca018087`, `atari-5200-ca020108`, and `atari-5200-ca021374`.
Native launch requires `--board ca018087`, `--board ca020108`, or
`--board ca021374`. No generic alias or implicit board mapping remains.

Snapshots keep version 20 and its positional hardware/input-cache layout.
The envelope profile selects the concrete owner. Generic and foreign-board
snapshots reject atomically. The controller-port count is derived from the
fixed owner, never saved as an independently mutable option. A two-port
payload with held nonexistent controls or pressed absent GTIA trigger lines
rejects before the runtime installs state. Existing cartridge-header evidence
survives restore, reset and BIOS replacement.

Independent v20 fixtures were recorded from source `d97b0d6f2` before moving
the engine. Tests re-label only their envelope model ID to compare both
four-port owners' complete original continuations, including frame/audio
packets. This is a test transformation, not a production legacy reader.

The service manual establishes the missing two-port external connections
(Figures 2-8 and 2-9, pp. 2-19 and 2-21). The connected-controller model
still uses the existing nominal analogue scale; no charging calibration or
unconnected-input voltage measurement is claimed. VCS adaptors, CX53
Trakball behaviour, discrete glue propagation delays, and switchbox/power
behaviour remain unqualified. Separate owners do not establish full-board
accuracy for those omitted surfaces.
