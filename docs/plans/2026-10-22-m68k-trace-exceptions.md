# 68000 trace exceptions

Lane: engineering frontier (Amiga compatibility, #1966). Re-consulted
RULES.md: the reference emulators for this CPU work are vAmiga's Moira core
(`emulators/amiga/vAmiga/Core/Components/CPU/Moira`) and the MC68000 User's
Manual, 9th edition, §6.3.8, in `reference/by-topic/cpu-68000/`. No archive
port.

## Problem

The core never took a trace exception. Setting T1 changed nothing, except in
STOP. Prince of the Yolkfolk's CPU check pushes a decoder onto the stack,
points vector 9 at it and sets T1, so each instruction decrypts the next.
Without trace it ran into the still-encrypted code and looped forever.

SingleStepTests/680x0 has no row that starts with T1 set (0 of 1,000,060).
The MAME-generated SingleStepTests/m68000 corpus has many, but its rows end
before trace processing, so they agreed with a core that never traced.

## Rules (manual §6.3.8, Moira `Moira::execute` and `execException`)

- T1 as an instruction begins decides whether a trace follows it. The
  instruction that sets T1 is not traced, and the one that clears it is.
- Illegal, privileged, line-A/F and format-error instructions never execute,
  so they are not traced. Address error, bus error and reset cancel the trace.
- An exception the instruction raises itself (TRAP, TRAPV, CHK, divide by
  zero) is processed first. Trace follows it, stacking the handler's address.
- Trace comes before a pending interrupt. The interrupt is then taken before
  the trace handler's first instruction.
- The stacked PC is the next instruction's address.
- The traced instruction completes, including its final prefetch, before
  trace processing begins. MAME's traced rows show this: a traced NOP
  records its one prefetch read and ends there, with T1 still in SR.

## Steps

1. Latch `trace_pending` (the traced instruction's address) from T1 as each
   new instruction decodes. Verified by `tests/trace.rs`, which fails
   without the change.
2. In this core the prefetch that completes an instruction is queued when
   the next one is promoted. So trace is taken at the next instruction's
   decode, in its place, as Moira does at the next `execute()`. Neither
   `PromoteIRC` nor the idle path takes an interrupt while a trace is
   pending. The 68020's Format $2 frame records the traced instruction's
   address.
3. Cancel it in the exception entry points listed above. STOP also traces
   when T1 was set as it began.
4. A snapshot can fall mid-instruction, so `trace_pending` is serialised. The
   Amiga envelope advances from 63 to 64, and the savestate decision record
   gains the amendment.
5. Verify the 680x0 crate suites, the Amiga machine and runtime suites, both
   SingleStepTests sweeps (the MAME agreement sweep now checks the boundary
   on its traced rows), Clippy and format. Then run the five #1966 games.

## Not in this change

- The 68020's T0 (trace on change of flow) is not modelled.
- Group-1 and group-2 exception processing runs 6 clocks short of Table 8-14
  (no 4-clock internal step before stacking, no 2-clock gap between the
  handler's prefetches). That affects TRAP, ILLEGAL, privilege, line-A/F and
  trace alike, so it gets its own fix, using the SingleStepTests transaction
  lists as the oracle.
