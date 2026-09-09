# Cycle-profile accounting

**Date:** 2026-09-09
**Status:** Active, first implementation slice
**Scope:** [Emu198x #1372](https://github.com/emu198x/emu198x/issues/1372)

## Contract

`profile_cycles` runs an exact, bounded window of authoritative machine ticks
and returns per-address execution costs plus Debug198x source-line totals.
The shared shell owns the command, report and source join. The runtime owns
measurement. The first implementation supports the PAL 48K Spectrum.

The report includes its clock unit and rational frequency. The 48K Spectrum
uses 14 MHz master ticks: four ticks per CPU T-state. These are elapsed emulated
ticks, including contention, rather than host duration or instruction-table
estimates. We do not claim a separate active-CPU/stall breakdown.

The optional Z80 observer records an instruction's first opcode/prefix address
and its identity at retirement. Interrupt responses and HALT refresh intervals
are distinct identities. It neither drives the bus nor changes the clock loop,
and is excluded from snapshots. The runtime uses the existing master-clock
advance path, including frame wrapping and machine audio flushing.

Retirement happens on a CPU edge. The interval closes before the next scheduled
CPU edge so the retiring half-cycle includes its remaining master ticks. The
capture budget itself is exact; it does not round up to an instruction boundary.

## Accounting boundaries

- Instructions, including HALT itself, accrue cost at their first byte.
- Repeating block instructions contribute one execution per retired iteration.
- Interrupt entry is a separate time bucket. Instructions inside the handler
  accrue their own addresses and source lines.
- Completed HALT refresh intervals accrue waiting time, not executions of the
  byte following HALT.
- An instruction already underway at capture start has no observed first-byte
  identity. Its remainder is leading partial time. A tail after an already
  observed retirement edge is also leading partial time in the next capture.
- An unfinished interval at the exact capture end is trailing partial time,
  even if its CPU has retired before that interval's final master tick.
- Every tick belongs to exactly one address or non-instruction/partial bucket.
  Unmapped code remains in the address map and in `unmapped_ticks`.

Source records describe the loaded build. The caller must load the matching
sidecar; this slice does not verify code hashes or reinterpret self-modified
instructions as new source. Exact labels are annotations, not routine extents:
Debug198x labels alone cannot establish inclusive function costs or a call tree.
Banked attribution must record mapping identity during execution before other
Spectrum models can opt in. A final paging map cannot reconstruct that history.

## Surface and resource bounds

The `cycle-profile` capability registers the shared MCP tool. Scripts use the
same implementation. Unsupported live models refuse before advancing, including
when the Spectrum MCP catalogue advertises the tool for its 48K alternative.
Budgets are 1–14,000,000 ticks. The 48K address space bounds the accumulator to
65,536 entries; no instruction-by-instruction trace is retained.

Queued input is applied before execution. The machine and runtime time advance,
but this is a debug operation: it does not deliver host frame/audio captures.
Active recordings are refused so a profile cannot silently cut a gap in a
recording. Ordinary frame execution can resume afterwards.

## Reproducible example

The [six-byte loop](../../test-data/sinclair/zx-spectrum/cycle-profile/loop.asm)
and its Asm198x-generated sidecar exercise taken and untaken `DJNZ` paths.
After loading the program at `$C000` and setting the execution entry there:

```json
[
  {"action":"load_debug_info","path":"test-data/sinclair/zx-spectrum/cycle-profile/loop.debug198x"},
  {"action":"profile_cycles","ticks":260}
]
```

At an uncontended boundary the four instruction addresses cost 28, 48, 136 and
16 master ticks respectively: 57 T-states. The remaining 32 ticks are HALT
waiting. Source lines 5–8 receive those same costs. The runtime integration test
checks the real sidecar and verifies identical script and MCP reports.

The first slice does not close #1372: banked attribution, additional CPU/runtime
adapters, routine/call accounting and comparisons with Asm198x static ranges
remain separate extensions.
