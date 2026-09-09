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

## Explicit routine costs

`profile_cycles` optionally accepts named routine ranges. Debug198x has labels
and line spans but no routine extents. Asm198x's static label-to-next-label totals
can split a routine at internal labels, so the profiler does not infer boundaries
from them. The caller declares the instruction starts each routine owns:

```json
{"action":"profile_cycles","ticks":552,"routines":[
  {"name":"main","ranges":[{"start":49152,"end":49159}]},
  {"name":"work","ranges":[{"start":49168,"end":49174}]}
]}
```

The [routine fixture](../../test-data/sinclair/zx-spectrum/routine-profile/calls.asm)
loads at `$C000`, calls `work` twice and halts. On the 48K at an uncontended
boundary, `main` has 3 completed instructions and 152 exclusive master ticks;
`work`, including its internal loop label, has 12 instructions and 368 ticks.
The remaining 32 ticks are HALT waiting. The test loads the real Asm198x sidecar
and checks identical script and MCP output.

Ranges are half-open: `start` is inclusive and `end` exclusive. An instruction's
first byte selects its owner even if later bytes cross the boundary. A routine
may have multiple ranges. All ranges in the same coordinate space must be
disjoint, including ranges of the same routine. Names must be unique, nonblank
and at most 128 characters. Requests allow at most 128 routines and 512 ranges
in total; invalid definitions are rejected before execution or queued input.

The default coordinate space is flat CPU addresses, matching only flat captures.
Banked captures require physical page offsets, for example:

```json
{"name":"bank5_work","ranges":[
  {"start":16,"end":22,"space":{"kind":"page","memory":"ram","page":5}}
]}
```

Page identity uses the captured memory namespace (`ram`, `rom`, `rom_overlay`
or `unmapped`) and page number. Aliases in different CPU slots combine; different
pages and namespaces remain separate. Source annotation is independent of these
explicit declarations, so declaring a ROM range does not give it a source join.

The report returns `routines` in request order with names, ranges, completed
`instructions` and `exclusive_ticks`, including zero rows for unexecuted routines.
These are costs of instructions in the declared extents: a caller owns its CALL
instruction and a callee owns its RET. Callees outside the caller's ranges are
excluded. Instruction counts are not invocation counts. The separate `call_cost` fields
below use observed transfers rather than inferring calls from these ranges.

`unassigned_routine_ticks` accounts for completed instructions outside every
routine. Routine costs plus that value equal the completed-instruction total.
Interrupt entry, HALT waiting and partial intervals retain their separate buckets.
Omitting routines (or passing an empty list) omits routine and call-tracking fields and
preserves the existing report shape. Exclusive attribution indexes the bounded
address report after capture; call attribution consumes ordered events during
capture. Neither retains an instruction trace.

This work does not close #1372: additional CPU/runtime adapters outside the
Spectrum family and bank-aware static comparisons remain separate extensions.


## Call-tracking foundation

The Z80's optional observer additionally exposes `completed_execution_event()`.
It records taken CALL and RST operations with their pushed return address, RET
and RETI/RETN operations, and accepted interrupt entries. Each event includes
its existing interval identity, entry/exit stack pointers and retirement PC.
Conditional operations emit a transfer only on the executed path. Calls whose
target equals their fall-through PC remain calls; jumps are not inferred as calls.

Transfer metadata is attached by the core's existing operation handlers and
published at the canonical retirement point. This follows the existing staged
push/pop paths, checked against FUSE's CALL/RET/RST macros. It does not decode
memory again, alter the instruction sequence or consume bus edges. Prefixes
retain the first-byte identity. Interrupt mode 0 records the implemented RST
response or existing fallback, without broadening the CPU's IM 0 support.

Only the latest event is retained. Partial starts have no event, and disabling
observation or restoring a snapshot leaves no stale metadata. Tests compare
serialized CPU state on every half-cycle and written memory against an
unobserved CPU, alongside assertions for transfer identities and destinations.

## Inclusive routine accounting

Requests with nonempty `routines` now stream complete instruction and interrupt
intervals into a shared call tracker. Each routine receives `call_cost` containing
`inclusive_ticks`, `calls`, `completed_calls` and `incomplete_calls`, alongside its
existing exclusive cost. The fixture above gives `main` 520 inclusive ticks and
`work` 368, with two observed and completed calls to `work`. HALT waiting stays
outside both routines. Captures without routines use the original accumulator
and omit call fields.

The tracker seeds a root at the first complete instruction. It does not know the
pre-capture stack or count that root as a call. CALL/RST pushes a pending frame;
the first complete destination instruction resolves its owner using the mapping
of its actual first opcode read. This covers bank aliases and M1 overlays. A
callee can be outside every declared range and still contribute to a known
caller's inclusive cost. Each tick is charged once to each distinct active
routine: recursive frames do not multiply the same routine's inclusive total.
Different routines' inclusive totals overlap and must not be summed as elapsed
time. CALL belongs to the caller and RET to the callee, as for exclusive costs.

Interrupt entry creates a barrier. Its response ticks stay in `interrupt_ticks`;
handler instructions and calls accrue within the handler segment without charging
the interrupted routines. A matching return resumes the suspended chain. HALT
waiting and partial intervals are excluded from inclusive costs just as they are
from exclusive instruction costs.

A return closes the top frame only when its destination CPU PC and restored SP
match the observed call/interrupt entry. The next complete instruction checks
routine ownership against its physical mapping, so a different return bank
cannot silently inherit the previous routine's costs. A changed owner without a
tracked transfer (including an unmodelled tail jump), unexpected destination or
unmatched return discards the chain and seeds a fresh root at the next attributable
instruction. It does not infer a tail call or guess missing frames. Stack
manipulations are permitted inside a routine; a broken return chain is detected
at its return, not guessed from every PUSH or POP.

`calls` counts observed CALL/RST entries with a complete destination instruction;
interrupt entries and capture roots are excluded. `completed_calls` counts those
closed by matching returns. `incomplete_calls` includes known calls left open at
capture end or discarded after uncertainty. These are capture-window costs, not
whole-invocation durations. A CALL at the end without a complete destination
instruction is unresolved and is not assigned a guessed routine or duration.

The top-level `call_tracking` reports `max_depth`, `open_frames`,
`discarded_frames`, `discontinuities`, `unresolved_calls`, `unassigned_calls` and
`depth_limit_reached`. Open frames include capture roots and interrupt barriers.
Discontinuities make ancestry-based totals incomplete even if individual call
returns were observed. At 256 retained frames the tracker stops, flags depth
exhaustion and marks outstanding known calls incomplete. Inclusive costs then
cover only the tracked prefix; exclusive address/routine accounting continues
for the entire exact capture window. No trace is retained, and range/stack bounds
apply equally to scripts and MCP across all thirteen Spectrum-family profiles.


## Execution-weighted static comparison

`profile_cycles` accepts optional `static_cycles` with `cpu` and `listing` fields.
`listing` is the JSON object produced by Asm198x `--listing-json`; `cpu` must match
the runtime's declared CPU and loaded sidecar header (`z80` for this adapter). The listing does not include
CPU identity, so the caller must declare it. Load the matching Debug198x sidecar
first. The caller remains responsible for using the same assembled build; neither
the sidecar nor this listing establishes a code hash or validates self-modified
code. No assembler or ISA timing table is duplicated in the emulator.

The runtime supplies machine ticks per static CPU cycle from its existing driver
divisor: four ticks per Z80 T-state for the flat Spectrum models, five on the
128K/Amstrad clock. Comparison uses integer machine ticks throughout. Divide
`measured_ticks` by `ticks_per_cpu_cycle` for CPU-cycle equivalents and retain the
remainder; this is elapsed emulated time, including stalls, not active CPU time.
The conversion is not inferred from host time or oscillator frequency alone.

Only an exact instruction-start address and file/line match participates. Each
static min/max is multiplied by the completed execution count. The report returns
`static_comparison` with the compared tick total, execution-weighted CPU-cycle
range, per-address results and exclusive per-routine subtotals. Routine subtotals
cover only comparable instructions and have a separate `uncomparable_ticks` value.
Inclusive call costs and static label-to-next-label totals are not compared:
they measure different spans and loop repetition is absent from straight-line
static totals. Different outcomes of a conditional instruction remain a range.

Per-address `relation` is `below_range`, `within_range` or `above_range`. An excess
is not automatically labelled contention: it could also indicate a mismatched
build, a spec problem or an emulator timing discrepancy. Static ranges are a
comparison, not validation of every execution path. Interrupt entry, HALT waiting
and capture partials remain outside the instruction comparison and retain their
existing report buckets.

Uncomparable instruction time is grouped into `banked`,
`ambiguous_or_missing_row`, `missing_cycles` and `source_mismatch`. Flat listing
addresses cannot establish physical bank identity. Native/linked section rows,
repeated file/line records, overlapping byte spans and spans containing multiple
executed instruction starts are also left uncomparable. Absence of a static
estimate is never a zero-cost estimate. The `coverage`, `labels` and `areas`
metadata are not used as evidence that individual rows can be compared.

Requests allow at most 16,384 listing rows, 1,024 bytes per source path, positive
byte spans within the u32 address space and positive min/max cycles up to
1,000,000. Invalid bounds, CPU conversion mismatches and a missing loaded sidecar
are rejected before execution. Comparisons are optional and captures without them
omit `static_comparison`, preserving the prior report shape.

### Reproduce the bridge

Use Asm198x with `--listing-json` support (fixture generated with 0.0.57):

```sh
asm198x --dialect pasmo --listing-json=test-data/sinclair/zx-spectrum/routine-profile/calls.listing.json test-data/sinclair/zx-spectrum/routine-profile/calls.asm -o /tmp/calls.bin
cmp /tmp/calls.bin test-data/sinclair/zx-spectrum/routine-profile/calls.bin
cargo test -p runtime-sinclair-zx-spectrum --test routine_cycle_profile asm_listing_compares_execution_weighted_exclusive_costs_in_script_and_mcp
```

The test loads the [real listing](../../test-data/sinclair/zx-spectrum/routine-profile/calls.listing.json),
program and sidecar, declares `main` and `work` as above and submits a 552-tick
capture through both scripts and MCP. Its `static_cycles` field is formed as
`{"cpu":"z80","listing":<the parsed listing object>}`; the rest of the request is
unchanged. Expected results:

| Span | Weighted static T-states | Measured ticks | Measured T-states |
|---|---:|---:|---:|
| main | 38–38 | 152 | 38 |
| work | 82–102 | 368 | 92 |
| All compared instructions | 120–140 | 520 | 130 |

The other 32 ticks are HALT waiting. DJNZ executes four times across the two
calls, contributing a static range of 32–52 T-states and a measured 42 T-states.
The listing's standalone `work` label total is only 7 T-states because `.loop`
starts the next static span; the explicit routine extents keep this distinction
visible. Bank-aware static listings and additional CPU/runtime adapters remain
follow-up work under #1372.
