# +3 read cadence: remove accumulated rounding error

The read stream now follows rounded absolute byte boundaries, rather than
rounding every byte up. At the emulated 3,546,895 Hz CPU clock, MFM alternates
113/114 ticks; FM normally uses 227 ticks, with occasional 228. Each field's
phase starts at the existing approximate pre-data interval. This does not add
physical rotation, index, PLL jitter or measured silicon service deadlines.

Source build: `be75aa2cf0222efa1eb9f0165487883d948a93a5`, suite manifests
0.38.0, Rust 1.99.0. Executable identity and lock hash are in `result.json`.
This source is newer than the v0.38.0 release commit; it is not that release
binary. No dependencies, public APIs or snapshot fields changed (version 6).
Writes, Scan and Format retain their existing timing approximation.

## Run

From an Emu checkout containing the change:

```sh
cargo test --locked -p nec-upd765a --test read_pacing
python3.14 docs/plans/2026-10-18-read-cadence/verify.py
```

The first command freshly exercises public registers. The second audits captured
evidence and does not execute an emulator. Full test/build commands are recorded
in `result.json`; logs and three complete ROM regression records are in
`runs.tar.gz`, with every member hashed.

## What passed

- 460 tests across controller, complete Spectrum runtime and +3 machine suites;
  63 ignored, excluded from qualification. Clippy, fmt and normal no-default-
  features release build pass.
- Six opcodes (Read Data, Read Deleted Data, Read Track, each FM/MFM), 512 bytes
  each, immediate and late reads: 6,144 request boundaries checked against
  independently calculated absolute times. All possible CPU-tick service
  offsets for each opcode's first two bytes preserve the following arrival.
- Missed nominal-slot deadlines at bytes 0, 1 and 511 report OR rather than EOT;
  results drain and a following command recovers. Native snapshot replay retains
  the shorter/longer remaining deadline and interrupted current-sector status.
- Two fresh cold-ROM +3 disk boots each pass 519 native-format, 8 firmware,
  3 loading and 7 execution assertions. Memory changes from `[0,0]` to `[90,91]`
  through four observed CPU writes; all 6,912 screen bytes match. Final sampled
  PC is 32822; the completion write occurs at 32820. Wrong expected signature
  `[90,92]` exits 1 with `expected [90, 92], observed [90, 91]`.

ROM regression configuration:
`spectrum-plus3-pal-40077-en40-rom-841d5454-6d6ccab2-c5590212-8ec5cb9f-drive-a-dsk180-v1`.
The source disk is mounted protected. Assembly/mastering are captured fixture
replay, runtime is fresh source execution; `qualified` remains false. No ROM,
native machine-state, or executable bytes are included in the evidence archive.
The normal ROM loader runs; no memory injection or restored state substitutes
for loading. Firmware hashes, command/environment and fixture identities remain
in each full regression record.

## Timing evidence and remaining work

NEC's December 1978 sheet p.12 gives 13/27 us service guidance at its standard
clock. The March 1979 application note p.1 distinguishes 16/32 us nominal
interrupt spacing from 13/29 us worst case. These are original-765 documents,
not measurements of the exact +3 µPD765A revision. Their provenance is in the
umbrella `reference/by-topic/fdc-765/`; the held PDFs are identified there.
Do not turn those worst-case figures into an exact fixed overrun cutoff.

Earlier independent Fuse and MAME observations remain in the adjacent
`2026-10-17-read-pacing/` and `2026-10-18-current-read-sector/` evidence. No fresh
reference emulator ran for this change. Fuse's coarse timeout and MAME's shared
FIFO prevent either establishing the silicon cutoff. Exact request latency,
clock/revision-specific cutoff, DMA/ND behavior, physical jitter and rotation
remain open under #1831. The nominal-slot timeout is still an approximation.
A passing PAL +3 diagnostic does not qualify other Spectrum models, disk
geometries, paging or peripherals.
