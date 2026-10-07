# Decision: retain MC68020 instruction-prefetch long words

**Date:** 2026-10-05
**Status:** implemented and verified within the stated bounds

## Decision

MC68020/68EC020 FetchIRC requests an aligned logical long word through the
existing SIZ/DSACK stages. An 8-bit responder requires four physical phases,
a 16-bit responder two, and an aligned 32-bit responder one. The compatibility
Ready(u16) response retains its documented abstract-word meaning and completes
two phases. Each physical cycle is visible to the machine and arbitration.
There are no direct memory reads, CPU bus callbacks or extra clocks.

The complete long word is retained in a serialized holding register in the
existing instruction-cache state, even with CACR.E clear or CACR.F set.
Holding hits supply either word independently of cache enable. A complete
cache hit reloads the holding register. Only enabled, unfrozen external fills
update both words of a cache entry; a partial transfer cannot publish a line.
Address and supervisor/user program space distinguish holding hits. Reset and
CACR writes invalidate the holding register. Odd instruction targets are
rejected before alignment by the existing address-error gate.

The MC68020 wrapper reinstalls this capability after deserialization. MC68030
and MC68040 explicitly retain their previous word-prefetch binding until their
own protocols are audited. The shared compatibility cache retains per-word
validity for those bindings; MC68020 fills always set both words together.

This overrides only the instruction-prefetch deferral in
[motorola-68020-dynamic-bus-sizing.md](motorola-68020-dynamic-bus-sizing.md).
The data-transfer, port-width, fault and later-CPU boundaries remain in force.
It does not establish exact instruction overlap, burst timing, delayed
prefetch faults or Format-$A restart fidelity.

The user approved this bounded design and Amiga snapshot version 47, which
rejects version 46 before positional payload decoding. CPU snapshots preserve
both in-flight read accumulation and completed holding data.

## Evidence and verification

Primary facts and registered FS-UAE precedent are recorded in
`../../../../reference/by-topic/cpu-68020/2026-prefetch-holding-observations.md`.
The external probe first exits 1 when a fetched sibling instruction changes
execution. After correction it retains the original instruction with caching
both enabled and disabled.

Directed signal tests cover all responder widths and compatibility responses,
low-word entry, full cache fills, holding independence from cache contents,
freeze, program-space separation, reset, and tick-for-tick restore after each
byte phase. Runtime replay covers a split ROM transfer and completed holding
state. Existing odd-target tests remain required. The broader family and
real-ROM checks are recorded in the active accuracy plan.
