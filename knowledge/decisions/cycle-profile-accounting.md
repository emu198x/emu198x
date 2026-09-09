# Cycle-profile accounting

**Date:** 2026-09-09
**Status:** Active
**Scope:** [Emu198x #1372](https://github.com/emu198x/emu198x/issues/1372)

## Contract

`profile_cycles` runs an exact, bounded window of authoritative machine ticks
and returns per-address execution costs plus Debug198x source-line totals.
The shared shell owns the command, report and source join. The runtime owns
measurement. Capture supports all eight original PAL Spectrum models: 16K, 48K, Spectrum+,
128K, +2, +2A, +2B and +3. Pentagon 128, Scorpion ZS-256, Timex TC2048,
TC2068 and TS2068 complete the thirteen-model Spectrum catalogue.

The report includes its clock unit and rational frequency. The 16K, 48K and Spectrum+
use 14 MHz master ticks: four ticks per CPU T-state. These are elapsed emulated
ticks, including contention, rather than host duration or instruction-table
estimates. The 128K and all +2/+3 variants use 17,734,475 Hz master ticks, five per
T-state. Capture follows the driver's existing two scheduled edges per T-state,
including the unequal intervals of an odd divider. We do not claim a separate
active-CPU/stall breakdown. Pentagon uses its 14,336,000 Hz clock and Scorpion
uses 14 MHz; both have four master ticks per CPU T-state. Timex TC2048 and
TC2068 use 14 MHz PAL timing; TS2068 uses 14.112 MHz NTSC timing. All three
Timex models have four master ticks per CPU T-state.

The optional Z80 observer records an instruction's first opcode/prefix address
and its identity at retirement. Interrupt responses and HALT refresh intervals
are distinct identities. It neither drives the bus nor changes the clock loop,
and is excluded from snapshots. The runtime uses the existing master-clock
advance path, including frame wrapping and machine audio flushing.

The 16K, 48K and Spectrum+ share one adapter over their existing machine core
and memory implementations. The 16K's disconnected upper 32 KiB remains
disconnected: attempted writes are dropped and instruction fetches read `$FF`.
Those executed instructions still accrue cost at their CPU addresses. Profiling
does not substitute a 48K memory map or restrict execution to installed RAM.

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
For banked machines, the runtime records the slot, physical page and RAM/ROM
namespace after the machine supplies the first opcode byte. A paging instruction keeps its original
mapping even if it replaces its own code bank. Prefix bytes remain part of that
first-byte identity. The accumulator keys on CPU address plus this mapping.

The +2A/+2B/+3 adapter asks the shared Amstrad memory implementation which
physical bank is selected. All four all-RAM configurations participate, including
RAM at address zero. Both paging registers and their lock are reflected in that
answer. Entering or leaving all-RAM mode during an instruction cannot relabel the
instruction's original bytes. Observation does not alter memory reads, writes,
contention or the machine's bus handling.

Pentagon and Scorpion can page TR-DOS in or out during an M1 fetch. Capture
samples the actual mapping after the first opcode read strobe, after the machine
handles its overlay trap. It observes raw pins without consuming bus transaction
edges. Later prefix fetches can change the overlay without relabelling the
instruction's first byte. `rom_overlay` is a separate memory namespace from base
`rom`: Pentagon overlay page 0 is its dedicated TR-DOS image; Scorpion overlay
page 1 is the ROM image its current implementation actually reads.

Scorpion profiling follows the current emulator, including its documented
unresolved differences from FUSE: `$1FFD` bit 0 selects the high RAM-bank bit,
the base ROM index combines two selector bits, and the overlay reads ROM 1.
The current machine I/O decoder also routes `$1FFD` writes to both paging
registers. This slice changes none of those behaviours and does not establish
them as hardware facts. The report measures the executed memory map; a matching
sidecar must name those RAM pages.

TC2048 uses a flat capture. TC2068/TS2068 capture their eight 8 KiB windows:
HOME ROM is `rom` pages 0–1, HOME RAM is `ram` pages 2–7, and EXROM is
`rom_overlay` page 0. The page number for HOME is its 8 KiB window number,
not a 16 KiB Spectrum bank. RAM sidecars use that number in `space.page` and
an offset relative to the window start. For example, HOME `$C123` is page 6,
offset `$123`; a section's expected slot does not constrain this source join.

The current TC2068/TS2068 core has no cartridge backing for DOCK windows.
Selected DOCK reads return `$FF` and writes are ignored. These executions use
`unmapped` with the window number as their page identifier and do not inherit
HOME source annotations. The memory implementation gives DOCK selection
priority over low-window EXROM, and maps EXROM at `$0000–$1FFF` when enabled
by port `$FF` bit 7. Profiling follows those existing semantics, including
paging changes caused by the executing instruction; it adds no cartridge or
paging hardware behaviour. Neither EXROM nor empty DOCK joins to RAM sources.

`counts.addresses` remains the CPU-address aggregate. `counts.mapped_addresses`
is its complete per-mapping decomposition, not additional elapsed time. In banked
reports, the annotated `addresses` list follows that decomposition. RAM source
lookup uses captured page and offset directly, independently of live paging or
manual section-base overrides. Aliases retain separate CPU addresses and slots;
their costs combine when they resolve to the same source line.

Only explicitly paged Debug198x sections participate in a banked source join.
Flat sections cannot prove which physical bank supplied a byte. ROM counts carry
their own namespace and page, but remain unmapped to source until the family has
a ROM source-space contract; RAM page 0 must never label ROM page 0.

## Surface and resource bounds

The `cycle-profile` capability registers the shared MCP tool. Scripts use the
same implementation. Unsupported live models refuse before advancing, including
when the Spectrum MCP catalogue advertises the tool for a supported alternative.
Budgets are 1–14,000,000 ticks. The 48K address space bounds the accumulator to
65,536 entries. The 128K-class mapping bounds the decomposition to 196,608
address/mapping identities (including RAM aliases and both ROM pages). The
Amstrad-class bound is 311,296 identities across its normal/all-RAM mappings and
four ROM pages. Pentagon is bounded by 212,992 identities and Scorpion by
376,832, including their distinct overlay namespaces. No instruction-by-instruction
trace is retained. TC2048 has the flat 65,536-address bound; each TC2068/TS2068
capture has at most 139,264 address/mapping identities across HOME, EXROM and
empty DOCK.

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

This work does not close #1372: additional CPU/runtime adapters outside the Spectrum family, routine/call accounting and comparisons with
Asm198x static ranges remain separate extensions.
