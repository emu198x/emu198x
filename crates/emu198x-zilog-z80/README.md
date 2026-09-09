# emu198x-zilog-z80

Zilog Z80.

The CPU of the ZX Spectrum, Amstrad CPC, MSX, Game Boy's ancestor line, and
a generation of arcade hardware. Ticked in half-cycles so that signals which
change mid-cycle — MREQ, IORQ, RFSH, and the contended-memory behaviour
those drive — are observable at the right instant.

```rust
use emu198x_zilog_z80::Z80;

let mut cpu = Z80::new();
cpu.tick(); // one half-cycle
```

Undocumented flags and instructions are implemented, and the core is
validated against the Zilog documentation, FUSE's test suite, and Tom Harte's
single-step corpus.

## Optional execution observation

Call `start_execution_observation()` before a bounded capture and sample
`completed_execution_event()` whenever `instructions_retired()` changes. The
latest event includes the first opcode/prefix address, stack pointers before and
after the interval, the next PC, and any taken CALL, RST, RET, interrupt entry or
RETI/RETN operation. Untaken conditions, ordinary jumps and HALT refresh intervals
have no call/return operation. The existing `completed_execution()` accessor
continues to return just the interval identity.

Events come from the executing operation, not a later disassembly of memory.
Only the latest retirement is retained; there is no trace allocation. Starting
observation partway through an instruction gives no event for that partial
interval. `stop_execution_observation()` discards the observer, and snapshots
never contain it. A restored CPU requires observation to be enabled again.

These are CPU-coordinate events, not a call stack or inclusive cost report. A
machine must attach the physical mapping at the actual opcode fetch, account for
its own elapsed ticks and handle capture boundaries, interrupts and stack changes.
Interrupt mode 0 reports the core's existing RST response and fallback semantics.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
