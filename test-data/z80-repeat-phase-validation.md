# Z80 repeat-phase and bus-edge validation

## What the experiment distinguishes

Original pin-driving adapters align the first opcode-read strobe at half-cycle
zero, then record 57 consecutive half-cycles without replacing the next opcode.
The reference is Perfect Z80, hoglet67/perfect6502 branch Z80 revision
`9b0d2e5e826c3a5fae3b5c6669bba1cd5d3b4217`. The five selected inputs are the
INIR/INDR/OTIR/OTDR/CPDR cases from the FUSE-disagreement investigation.

Architectural registers do not mirror every physical register-node transition:
the core applies repeat AF/WZ/PC at relative phase 30. The netlist sets repeated
WZ at 34, rewinds PC at 38 and writes AF at 46, during the next opcode fetch.
Both start the next M1 at 41 and assert its read strobe at 42. Physical-node
writeback and logical instruction retirement are different observation surfaces.
The core retains its architectural update schedule; the evidence below checks
continuation and interrupt consequences rather than treating node visibility
alone as a functional error.

## Corrected external strobes

The investigation exposed independently observable pin errors. Memory MREQ and
RD/WR now release at T3 falling, half a T-state before the following cycle can
change address/data. I/O IORQ and RD/WR assert at T2 rising and release at final
T3 falling (called T4Fall in this engine because it counts automatic TW).
Instruction durations, opcode/refresh timing and the WAIT-handling algorithm
are unchanged. The 48K ULA's answered-port contention lookup now recognises
the first visible IORQ arming edge through stalled-clock history. Its former
second-strobe-free-edge test depended on the old CPU timing and missed 18,432
full-frame oracle samples once IORQ moved. With the consumer corrected, the
full-frame I/O contention oracle returns to zero differences.

The five corrected traces match all 285 sampled rows of address, M1, MREQ,
IORQ, RD and WR. SpecIde's explicit memory/I/O cycle states corroborate the
edges. Zilog UM0080's Memory Read Or Write prose requires WR to release half a
T-state before address/data changes. The old waveform comments misinterpreted
that requirement. Six revised waveform regressions fail against the old core.

The exact I/O assertion-to-latch lead is five half-cycles, exported as
`IO_READ_DATA_LATCH_LEAD_HALF_CYCLES`. The existing T-state constant remains a
whole-T-state projection for coarse raster predictors, not an exact duration.
The latch consumes data supplied before its edge; post-tick strobes are released.
Existing raster origins are not recalibrated by this change.

## Snapshot and NMI results

Snapshots at all 43 positions from instruction start through the repeat boundary
resume with identical serialized CPU state (including pins) and memory on every
subsequent half-cycle through two iterations: 215 snapshot positions total.

The independent NMI sweep asserts an edge after each relative phase 0–42, then
lets the handler PUSH AF. Of 215 comparisons, 205 agree on handler timing,
stacked return address and AF. At phases 39 and 40 in every selected case the
core accepts NMI one iteration earlier than the netlist. This is a separate
interrupt-cutoff defect, preserved explicitly in the private investigation queue.
Delaying flag writeback does not correct that sampling/latching decision.

## Validation and limits

On 2026-09-24: 196 ordinary Z80 tests pass; 1,604,000 Tom Harte cases are exact;
FUSE retains 1,350 exact cases plus six pinned differences, zero unexpected.
All six pinned Rak 1.2a tape exercisers pass. All five ROM-backed 48K
floating-bus oracle tests pass, together with the three I/O contention oracles
and the falling-edge lookup test. Formatting and affected-crate Clippy pass.
Earlier cold ZEX results are not claimed as a rerun of this change.

Original adapters, transition records and log hashes are retained privately in
the umbrella at `ops/experiments/z80-repeat-phase/`; the shared source note is
`reference/by-topic/cpu-z80/z80-repeat-phase-evidence.md`. No third-party source
or binaries are committed here. This is selected no-WAIT die-derived simulation,
not a physical-chip measurement or a CMOS claim. RFSH and data-bus transients
are not part of the new differential assertion. Snapshot equivalence is a
continuation test, not proof of physical register-phase equivalence.
